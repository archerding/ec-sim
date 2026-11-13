//! 最小 HTTP/1.1 GET 客户端（零第三方依赖，仅 `std`）。
//!
//! 只面向本机控制面（`ec-sim-esc --serve`）：明文 HTTP、`Connection: close`、
//! `Content-Length` 或读到 EOF。**只提供 GET**——没有写方法，这是 B47 的硬边界。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// 上游响应（只读观测值）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// 上游访问错误（映射为 MCP 工具错误码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamError {
    /// 连接失败 / 读写出错。
    Io(String),
    /// 超过超时时间。
    Timeout(String),
    /// 响应体超过上限。
    TooLarge(String),
    /// 响应不是可解析的 HTTP。
    BadResponse(String),
}

impl UpstreamError {
    /// 稳定错误码（供 MCP 工具错误文本使用）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "UPSTREAM_ERROR",
            Self::Timeout(_) => "TIMEOUT",
            Self::TooLarge(_) => "RESPONSE_TOO_LARGE",
            Self::BadResponse(_) => "UPSTREAM_ERROR",
        }
    }

    /// 错误详情。
    pub fn message(&self) -> String {
        match self {
            Self::Io(message)
            | Self::Timeout(message)
            | Self::TooLarge(message)
            | Self::BadResponse(message) => message.clone(),
        }
    }
}

/// 只读上游：这里只有 `get`，避免桥接层意外引入写路径。
pub trait Upstream {
    fn get(&mut self, path: &str) -> Result<HttpResponse, UpstreamError>;
}

/// 本机控制面 HTTP GET 客户端。
pub struct HttpClient {
    host: String,
    port: u16,
    base_path: String,
    token: Option<String>,
    timeout: Duration,
    max_bytes: usize,
}

impl HttpClient {
    /// 解析 `--api` 基地址（只支持 `http://`；默认只允许回环地址）。
    pub fn new(
        base_url: &str,
        token: Option<String>,
        timeout_ms: u64,
        max_bytes: usize,
        allow_remote: bool,
    ) -> Result<Self, String> {
        let rest = base_url
            .strip_prefix("http://")
            .ok_or_else(|| format!("--api 只支持 http://（本机控制面；当前 {base_url}）"))?;
        let (authority, base_path) = match rest.find('/') {
            Some(index) => (&rest[..index], rest[index..].trim_end_matches('/')),
            None => (rest, ""),
        };
        if authority.is_empty() {
            return Err("--api 缺少主机".to_string());
        }
        let (host, port) = split_authority(authority)?;
        if !allow_remote && !is_loopback(&host) {
            return Err(format!(
                "拒绝非回环地址 {host}：MCP 桥接默认只连本机控制面；确需远程用 --allow-remote"
            ));
        }
        Ok(Self {
            host,
            port,
            base_path: base_path.to_string(),
            token,
            timeout: Duration::from_millis(timeout_ms.max(1)),
            max_bytes: max_bytes.max(1024),
        })
    }

    fn get_raw(&mut self, path: &str) -> Result<HttpResponse, UpstreamError> {
        let full_path = format!("{}{}", self.base_path, path);
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|error| UpstreamError::Io(format!("连接失败：{error}")))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|error| UpstreamError::Io(format!("设置读超时失败：{error}")))?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|error| UpstreamError::Io(format!("设置写超时失败：{error}")))?;

        let mut request = format!(
            "GET {full_path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\nAccept: application/json, text/plain\r\n",
            self.host, self.port
        );
        if let Some(token) = &self.token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str("\r\n");
        stream
            .write_all(request.as_bytes())
            .map_err(|error| map_io_error("写请求失败", error))?;

        let mut bytes = Vec::new();
        let limit = self.max_bytes.saturating_add(64 * 1024);
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    bytes.extend_from_slice(&chunk[..read]);
                    if bytes.len() > limit {
                        return Err(UpstreamError::TooLarge(format!(
                            "响应超过上限 {:?} 字节",
                            self.max_bytes
                        )));
                    }
                }
                Err(error) => return Err(map_io_error("读响应失败", error)),
            }
        }
        parse_response(&bytes, self.max_bytes)
    }
}

impl Upstream for HttpClient {
    fn get(&mut self, path: &str) -> Result<HttpResponse, UpstreamError> {
        self.get_raw(path)
    }
}

fn map_io_error(context: &str, error: std::io::Error) -> UpstreamError {
    if matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ) {
        UpstreamError::Timeout(format!("{context}：{error}"))
    } else {
        UpstreamError::Io(format!("{context}：{error}"))
    }
}

fn split_authority(authority: &str) -> Result<(String, u16), String> {
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest
            .find(']')
            .ok_or_else(|| "IPv6 地址缺少右括号".to_string())?;
        let host = format!("[{}]", &rest[..end]);
        let port = match rest.get(end + 1..) {
            Some(after) if after.starts_with(':') => after[1..]
                .parse::<u16>()
                .map_err(|_| "端口非法".to_string())?,
            Some("") | None => 80,
            Some(_) => return Err("IPv6 地址后只允许 :port".to_string()),
        };
        return Ok((host, port));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => Ok((
            host.to_string(),
            port.parse::<u16>().map_err(|_| "端口非法".to_string())?,
        )),
        Some(_) => Err("主机为空".to_string()),
        None => Ok((authority.to_string(), 80)),
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn parse_response(bytes: &[u8], max_bytes: usize) -> Result<HttpResponse, UpstreamError> {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| UpstreamError::BadResponse("响应缺少头部终止符".to_string()))?;
    let header_text = std::str::from_utf8(&bytes[..header_end])
        .map_err(|error| UpstreamError::BadResponse(format!("响应头非 UTF-8：{error}")))?;
    let mut lines = header_text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| UpstreamError::BadResponse("响应缺少状态行".to_string()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| UpstreamError::BadResponse(format!("状态行非法：{status_line}")))?
        .parse::<u16>()
        .map_err(|_| UpstreamError::BadResponse(format!("状态码非法：{status_line}")))?;

    let mut content_length = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let body_start = header_end + 4;
    let mut body = &bytes[body_start..];
    if let Some(length) = content_length {
        if body.len() >= length {
            body = &body[..length];
        }
    }
    if body.len() > max_bytes {
        return Err(UpstreamError::TooLarge(format!(
            "响应体 {} 字节超过上限 {max_bytes}",
            body.len()
        )));
    }
    Ok(HttpResponse {
        status,
        body: String::from_utf8_lossy(body).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_response_with_content_length() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 7\r\n\r\n{\"a\":1}";
        let response = parse_response(raw, 1024).expect("parse");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{\"a\":1}");
    }

    #[test]
    fn rejects_truncated_response() {
        let error = parse_response(b"HTTP/1.1 200 OK", 1024).expect_err("应失败");
        assert!(error.code() == "UPSTREAM_ERROR");
    }

    #[test]
    fn base_url_restricts_to_http_loopback() {
        assert!(HttpClient::new("https://127.0.0.1:1", None, 10, 1024, false).is_err());
        assert!(HttpClient::new("http://10.0.0.2:8091", None, 10, 1024, false).is_err());
        assert!(HttpClient::new("http://10.0.0.2:8091", None, 10, 1024, true).is_ok());
        let client = HttpClient::new(
            "http://127.0.0.1:8091/api/",
            Some("secret".to_string()),
            5,
            1024,
            false,
        )
        .expect("loopback ok");
        assert_eq!(client.host, "127.0.0.1");
        assert_eq!(client.port, 8091);
        assert_eq!(client.base_path, "/api");
    }

    #[test]
    fn parses_ipv6_authority() {
        assert_eq!(
            split_authority("[::1]:9000").expect("ok"),
            ("[::1]".into(), 9000)
        );
        assert_eq!(
            split_authority("localhost").expect("ok"),
            ("localhost".into(), 80)
        );
    }
}
