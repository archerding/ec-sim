#!/usr/bin/env python3
"""B47 / M3-W8：`ec-sim-mcp` stdio 桥接端到端探针（无第三方依赖）。

覆盖：
1. `initialize` / `notifications/initialized` / `tools/list`（恰好 9 个只读工具）；
2. 只读工具实调：/health、/state、/dict、/sdo（经真实控制面 `--serve`）；
3. 错误路径：未知工具、缺必填参数、静态模式下 live-only 工具、未知方法、坏 JSON；
4. 安全边界：`--serve-write` 开启后 `tools/list` **完全不变**（无写工具）。

退出码：0 = 全部通过；1 = 失败；2 = 前置缺失（SKIP）。
"""

import argparse
import json
import queue
import socket
import subprocess
import sys
import threading
import time
import urllib.request
from pathlib import Path

EXPECTED_TOOLS = {
    "ecsim_health",
    "ecsim_state",
    "ecsim_dict",
    "ecsim_sdo_read",
    "ecsim_topology",
    "ecsim_pdo",
    "ecsim_pdo_history",
    "ecsim_metrics",
    "ecsim_sdo_audit",
}

failures = []


def check(condition, message):
    if condition:
        print(f"ok   {message}")
    else:
        print(f"FAIL {message}")
        failures.append(message)


def free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_health(port, timeout=8.0):
    deadline = time.monotonic() + timeout
    url = f"http://127.0.0.1:{port}/health"
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=0.5) as response:
                if response.status == 200:
                    return True
        except Exception:  # noqa: BLE001 - 启动期连接失败是预期路径
            time.sleep(0.05)
    return False


class ControlPlane:
    def __init__(self, binary, manifest, extra=()):
        self.port = free_port()
        self.proc = subprocess.Popen(
            [binary, "--serve", f"127.0.0.1:{self.port}", "--manifest", manifest, *extra],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        if not wait_health(self.port):
            self.stop()
            raise RuntimeError(f"控制面未在端口 {self.port} 就绪")

    @property
    def api(self):
        return f"http://127.0.0.1:{self.port}"

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()


class McpClient:
    def __init__(self, binary, api):
        self.proc = subprocess.Popen(
            [binary, "--api", api],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
        )
        self.lines = queue.Queue()
        reader = threading.Thread(target=self._read_lines, daemon=True)
        reader.start()

    def _read_lines(self):
        try:
            for line in self.proc.stdout:
                self.lines.put(line)
        except Exception as error:  # noqa: BLE001 - 线程内错误通过哨兵传回
            self.lines.put(RuntimeError(f"读取 ec-sim-mcp stdout 失败：{error}"))
        finally:
            self.lines.put(None)

    def send_raw(self, text):
        self.proc.stdin.write(text + "\n")
        self.proc.stdin.flush()

    def send(self, payload):
        self.send_raw(json.dumps(payload, separators=(",", ":")))

    def recv(self, timeout=10.0):
        line = self.lines.get(timeout=timeout)
        if line is None:
            raise RuntimeError("ec-sim-mcp 提前退出（stderr: %s）" % self.stderr_text())
        if isinstance(line, Exception):
            raise line
        return json.loads(line)

    def request(self, payload, timeout=10.0):
        self.send(payload)
        return self.recv(timeout)

    def stderr_text(self):
        try:
            return self.proc.stderr.read() or ""
        except Exception:  # noqa: BLE001
            return ""

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()


def tool_result(response):
    result = response.get("result") or {}
    content = result.get("content") or [{}]
    text = content[0].get("text", "") if content else ""
    return result, text


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", default="target/release/ec-sim-esc")
    parser.add_argument("--mcp", default="target/release/ec-sim-mcp")
    parser.add_argument(
        "--manifest", default="plugins/generic-cia402-cst-servo/plugin.yaml"
    )
    args = parser.parse_args()

    # Windows 下调用方可能给的是无 `.exe` 的路径；统一转绝对路径再 spawn
    # （CreateProcess 对相对可执行路径解析不可靠）。
    def resolve(path):
        candidate = Path(path)
        if not candidate.exists():
            candidate = Path(path + ".exe")
        return str(candidate.resolve())

    args.server = resolve(args.server)
    args.mcp = resolve(args.mcp)
    args.manifest = str(Path(args.manifest).resolve())
    for path in (args.server, args.mcp, args.manifest):
        if not Path(path).exists():
            print(f"SKIP: 缺少 {path}（先 cargo build --release -p esc-node -p mcp-server）")
            return 2

    server = ControlPlane(args.server, args.manifest)
    client = McpClient(args.mcp, server.api)
    try:
        # 1) initialize + tools/list
        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2025-03-26", "capabilities": {}},
            }
        )
        result = response.get("result") or {}
        check(result.get("protocolVersion") == "2025-03-26", "initialize 回显 protocolVersion")
        check(
            (result.get("serverInfo") or {}).get("name") == "ec-sim-mcp",
            "initialize serverInfo.name",
        )

        # 通知不应产生响应：紧接着的响应必须是 id=2。
        client.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        response = client.request({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        check(response.get("id") == 2, "通知不产生响应（下一条响应 id=2）")
        tools = (response.get("result") or {}).get("tools") or []
        names = {tool.get("name") for tool in tools}
        check(len(tools) == 9, "tools/list 恰好 9 个工具")
        check(names == EXPECTED_TOOLS, "tools/list 工具集合与目录一致")
        check(
            all(name and name.startswith("ecsim_") for name in names),
            "工具名均为 ecsim_*",
        )
        check(
            not any("write" in (name or "") or "post" in (name or "") for name in names),
            "不含任何写工具",
        )

        # 2) 只读工具实调
        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "ecsim_health", "arguments": {}},
            }
        )
        result, text = tool_result(response)
        check(result.get("isError") is False and text, "ecsim_health 返回非空文本")

        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {"name": "ecsim_state", "arguments": {"slave": 0}},
            }
        )
        result, text = tool_result(response)
        state = json.loads(text) if result.get("isError") is False else {}
        check(
            result.get("isError") is False and state.get("schema") == "ec-sim/state-v1",
            "ecsim_state 返回 state-v1",
        )

        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "ecsim_dict", "arguments": {}},
            }
        )
        result, text = tool_result(response)
        dictionary = json.loads(text) if result.get("isError") is False else {}
        check(
            result.get("isError") is False and dictionary.get("schema") == "ec-sim/dict-v1",
            "ecsim_dict 返回 dict-v1",
        )

        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "ecsim_sdo_read",
                    "arguments": {"index": "0x6040", "sub": 0},
                },
            }
        )
        result, text = tool_result(response)
        sdo = json.loads(text) if result.get("isError") is False else {}
        check(
            result.get("isError") is False and sdo.get("schema") == "ec-sim/sdo-v1",
            "ecsim_sdo_read 返回 sdo-v1",
        )

        # 3) 错误路径
        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {"name": "ecsim_nope", "arguments": {}},
            }
        )
        result, text = tool_result(response)
        check(result.get("isError") is True and "TOOL_NOT_FOUND" in text, "未知工具 → TOOL_NOT_FOUND")

        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {"name": "ecsim_sdo_read", "arguments": {"sub": 0}},
            }
        )
        result, text = tool_result(response)
        check(result.get("isError") is True and "BAD_ARGUMENT" in text, "缺必填参数 → BAD_ARGUMENT")

        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 9,
                "method": "tools/call",
                "params": {"name": "ecsim_topology", "arguments": {}},
            }
        )
        result, text = tool_result(response)
        check(
            result.get("isError") is True
            and ("NOT_FOUND" in text or "MODE_UNAVAILABLE" in text),
            "静态模式下 live-only 工具 → 工具错误",
        )

        response = client.request({"jsonrpc": "2.0", "id": 10, "method": "nope"})
        check(
            ((response.get("error") or {}).get("code")) == -32601,
            "未知方法 → JSON-RPC -32601",
        )

        client.send_raw("{bad json")
        response = client.recv()
        check(
            ((response.get("error") or {}).get("code")) == -32700,
            "坏 JSON → JSON-RPC -32700",
        )
    finally:
        client.stop()
        server.stop()

    # 4) --serve-write 下工具表必须完全不变
    server = ControlPlane(args.server, args.manifest, extra=("--serve-write",))
    client = McpClient(args.mcp, server.api)
    try:
        response = client.request({"jsonrpc": "2.0", "id": 20, "method": "tools/list"})
        tools = (response.get("result") or {}).get("tools") or []
        names = {tool.get("name") for tool in tools}
        check(
            len(tools) == 9 and names == EXPECTED_TOOLS,
            "--serve-write 下 tools/list 不变（无写工具）",
        )
        response = client.request(
            {
                "jsonrpc": "2.0",
                "id": 21,
                "method": "tools/call",
                "params": {"name": "ecsim_state", "arguments": {}},
            }
        )
        result, _ = tool_result(response)
        check(result.get("isError") is False, "--serve-write 下读工具仍可用")
    finally:
        client.stop()
        server.stop()

    if failures:
        print(f"FAIL: mcp-stdio 探针 {len(failures)} 项失败")
        return 1
    print("PASS: mcp-stdio 探针（9 只读工具、读写边界、--serve-write 下表不变）")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:  # noqa: BLE001 - 探针以退出码表达失败
        import traceback

        traceback.print_exc()
        print(f"FAIL: 探针异常：{error}")
        sys.exit(1)
