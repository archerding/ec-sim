//! 最小 JSON 解析/序列化（零第三方依赖）。
//!
//! 只覆盖 MCP/JSON-RPC 需要的能力：对象、数组、字符串（含 `\uXXXX`）、数字、布尔、null。
//! 数字统一用 `f64` 保存，序列化时整数值输出为整数（JSON-RPC 的 `id` 回显需要）。

use std::fmt::Write as _;

/// JSON 值。
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// 构造对象（保持插入顺序）。
    pub fn object(entries: Vec<(&str, Json)>) -> Self {
        Self::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    /// 按 key 取对象字段。
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Self::Object(entries) => entries
                .iter()
                .find_map(|(name, value)| (name == key).then_some(value)),
            _ => None,
        }
    }

    /// 字符串值。
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// 整数值（`f64` 且无小数部分）。
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value)
                if value.is_finite() && value.fract() == 0.0 && value.abs() <= 9.0e15 =>
            {
                Some(*value as i64)
            }
            _ => None,
        }
    }

    /// 对象字段切片。
    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Self::Object(entries) => Some(entries),
            _ => None,
        }
    }

    /// 数组元素切片。
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// 序列化为紧凑 JSON（不含换行）。
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Number(value) => {
                if value.is_finite() && value.fract() == 0.0 && value.abs() <= 9.0e15 {
                    let _ = write!(out, "{}", *value as i64);
                } else if value.is_finite() {
                    let _ = write!(out, "{value}");
                } else {
                    out.push_str("null");
                }
            }
            Self::String(text) => write_string(out, text),
            Self::Array(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            Self::Object(entries) => {
                out.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_string(out, key);
                    out.push(':');
                    value.write_json(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            control if (control as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// 解析 JSON 文本；尾部只允许空白。
pub fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        position: 0,
    };
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.position != parser.bytes.len() {
        return Err(format!("尾随字符（偏移 {}）", parser.position));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.bytes.get(self.position) {
            if matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
                self.position += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(Json::String(self.parse_string()?)),
            Some(b't') => self.expect_literal("true", Json::Bool(true)),
            Some(b'f') => self.expect_literal("false", Json::Bool(false)),
            Some(b'n') => self.expect_literal("null", Json::Null),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.parse_number(),
            Some(byte) => Err(format!(
                "非法 JSON 起始字符 {:?}（偏移 {}）",
                byte as char, self.position
            )),
            None => Err("JSON 为空".to_string()),
        }
    }

    fn expect_literal(&mut self, literal: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.position..].starts_with(literal.as_bytes()) {
            self.position += literal.len();
            Ok(value)
        } else {
            Err(format!("非法字面量（偏移 {}）", self.position))
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.position += 1; // '{'
        let mut entries = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(format!("对象 key 必须是字符串（偏移 {}）", self.position));
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(format!("对象缺少冒号（偏移 {}）", self.position));
            }
            self.position += 1;
            let value = self.parse_value()?;
            entries.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    return Ok(Json::Object(entries));
                }
                _ => return Err(format!("对象缺少逗号/右括号（偏移 {}）", self.position)),
            }
        }
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.position += 1; // '['
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(format!("数组缺少逗号/右括号（偏移 {}）", self.position)),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.position += 1; // '"'
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err("字符串未闭合".to_string()),
                Some(b'"') => {
                    self.position += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.position += 1;
                    let escape = self.peek().ok_or("转义序列未闭合")?;
                    self.position += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.parse_unicode_escape()?),
                        other => return Err(format!("非法转义 \\{}", other as char)),
                    }
                }
                Some(byte) if byte < 0x20 => {
                    return Err(format!("字符串含未转义控制字符（偏移 {}）", self.position))
                }
                Some(byte) => {
                    // 按 UTF-8 继续读取（调用方输入已是 &str，直接取字符更简单）。
                    let rest = std::str::from_utf8(&self.bytes[self.position..])
                        .map_err(|error| format!("非法 UTF-8：{error}"))?;
                    let character = rest.chars().next().ok_or("字符串未闭合")?;
                    out.push(character);
                    self.position += character.len_utf8();
                    let _ = byte;
                }
            }
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let first = self.parse_hex_u16()?;
        if (0xD800..=0xDBFF).contains(&first) {
            // 代理对：需要紧随 `\uXXXX` 的低代理。
            if self.peek() == Some(b'\\') && self.bytes.get(self.position + 1) == Some(&b'u') {
                self.position += 2;
                let second = self.parse_hex_u16()?;
                if (0xDC00..=0xDFFF).contains(&second) {
                    let code = 0x1_0000u32
                        + (u32::from(first - 0xD800) << 10)
                        + u32::from(second - 0xDC00);
                    return char::from_u32(code).ok_or_else(|| "非法代理对".to_string());
                }
            }
            return Err("孤立的高代理".to_string());
        }
        char::from_u32(u32::from(first)).ok_or_else(|| "非法的 \\u 转义".to_string())
    }

    fn parse_hex_u16(&mut self) -> Result<u16, String> {
        let end = self.position + 4;
        let digits = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| "\\u 转义不足 4 位".to_string())?;
        let text = std::str::from_utf8(digits).map_err(|_| "\\u 转义非 ASCII".to_string())?;
        let value = u16::from_str_radix(text, 16).map_err(|_| "\\u 转义非十六进制".to_string())?;
        self.position = end;
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.position += 1;
        }
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|_| "非法数字".to_string())?;
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| format!("非法数字：{text}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_serializes_nested_values() {
        let text = r#"{"a":1,"b":[true,null,"x\n\u4e2d"],"c":{"d":-2.5}}"#;
        let value = parse(text).expect("parse");
        assert_eq!(value.get("a").and_then(Json::as_i64), Some(1));
        assert_eq!(
            value.get("b").and_then(Json::as_array).map(<[Json]>::len),
            Some(3)
        );
        assert_eq!(
            value
                .get("c")
                .and_then(|c| c.get("d"))
                .and_then(Json::as_i64),
            None,
            "小数不是整数"
        );
        let roundtrip = parse(&value.to_json()).expect("reparse");
        assert_eq!(roundtrip, value);
    }

    #[test]
    fn rejects_trailing_and_malformed_input() {
        assert!(parse("{} x").is_err());
        assert!(parse("{").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("\"\\uD800\"").is_err(), "孤立高代理应报错");
        assert_eq!(
            parse("\"\\u00e4\"").expect("escape"),
            Json::String("ä".into())
        );
    }
}
