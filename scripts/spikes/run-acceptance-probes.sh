#!/usr/bin/env bash
# PV-18 / Gate B-2：**行业协议验收探针聚合脚本**（EC-SIM ACCEPTANCE §3.2.2 的一键门）。
#
# 与 `run-all-probes.sh` 的分工：
#   - `run-all-probes.sh` = **B-1 观测面/控制面**探针（任何平台可跑的 any 类 + Linux root 的 TAP 项，
#     注册表由脚本自算；用于"控制面不做假实时/只读边界"这类回归）；
#   - 本脚本 = **B-2 行业协议验收**探针组（§3.2.2 表：发现/身份、SII、OD/PDO、SDO、AL/OP、WKC/多站、
#     DC、Watchdog、控制面交叉、CiA402 Tier0），按 ACC-01..18/ACC-26/ACC-27 组织；
#     两者可串联（Gate B = `run-all-probes.sh` + 本脚本，均 0 FAIL）。
#
# 输出格式（供证据/CI 归档，stderr 不进入数据行）：
#   `<name>|<kind>|<EXIT>|<PASS|FAIL|SKIP>|<秒>[|<skip_reason>]`
#   - kind ∈ any | linux | root（平台/权限要求；skip 规则见下）
#   - SKIP 行**必须**带第 6 字段原因（"平台不适用/缺 root/缺夹具/可选依赖"之一，禁止用 SKIP 顶替 FAIL）
#   - 末尾：`== 汇总：N PASS / M FAIL / K SKIP（共 T）==` 与 `RESULT: PASS|FAIL`
#   - **T = 注册表项数，由脚本从清单自算打印**（不硬编码，避免"21 vs 20"类清单漂移）
#
# 判定：0 FAIL → RESULT PASS（允许有理由的 SKIP）；任一 FAIL → exit 1。
# 局限：脚本只覆盖"能自动化/本机可跑"的探针；真机/HIL 项（物理安全链/真实伺服）不在此列。
#
# 前置（Linux root 验证机；与 EC-SIM ACCEPTANCE §3.2 一致）：
#   cargo build --release -p esc-node -p mcp-server
#   验证机本地 SOEM TAP 构建、IgH（含 ecrt 夹具）按各自探针注释准备（缺夹具时探针 exit 2 → SKIP+原因）
#
# 用法：
#   ./run-acceptance-probes.sh              # 自动按平台/root 选择
#   ./run-acceptance-probes.sh --list       # 只列计划（含 skip 原因）
#   RUN_ROOT=0 ./run-acceptance-probes.sh   # 即使 root 也不跑 root 类
#   TIMEOUT_SECONDS=240 LOG_DIR=/tmp/acc ./run-acceptance-probes.sh
set -uo pipefail

cd "$(dirname "$0")/../.." || exit 1

BIN="${BIN:-target/release/ec-sim-esc}"
PY="${PY:-$(command -v python3 || command -v python || echo python3)}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-180}"
RUN_ROOT="${RUN_ROOT:-auto}"      # auto | 0 | 1
LOG_DIR="${LOG_DIR:-/tmp/ec-sim-acceptance}"
mkdir -p "$LOG_DIR"

os="$(uname -s 2>/dev/null || echo unknown)"
case "$os" in
  Linux*) host_kind="linux" ;;
  *) host_kind="other" ;;
esac
is_root="no"
if [ "$(id -u 2>/dev/null || echo 1)" = "0" ]; then is_root="yes"; fi
has_timeout="no"
if timeout --version >/dev/null 2>&1; then has_timeout="yes"; fi

# 注册表：name|kind|acc（acc 用于证据归档/追溯；不改探针行为）。
# kind 只决定"本机是否能跑"：any = 任意平台；linux = 需要 Linux（UDS/合成主站）；
# root = 需要 root + TAP/内核模块/IgH（缺 root 时 SKIP，禁止在 CI 用 SKIP 顶替 B-2，见 ACCEPTANCE §1.3）。
PROBES="
minimal-esc-responder|root|ACC-01/02
soem-multi-slave-probe|linux|ACC-01/02
uds-multi-slave-probe|linux|ACC-01/02
tap-multi-slave-probe|root|ACC-01/02,ACC-14
tap-cia402-multi-slave-probe|root|ACC-07,ACC-22(EC-Sim 侧)
esi-sii-scan-probe|root|ACC-03
soem-mailbox-probe|root|ACC-03/04
sv660n-capability-probe|root|ACC-05/06
manifest-slave-probe|root|ACC-07
esi-manifest-op-probe|root|ACC-07
beckhoff-el2008-probe|root|ACC-07
generic-cia402-probe|root|ACC-07
cia402-modes-probe|linux|ACC-07,ACC-26
sdo-upload-probe|root|ACC-08
sdo-segmented-upload-probe|root|ACC-09
sdo-info-probe|root|ACC-10
sdo-ca-probe|linux|ACC-10
cia402-sdo-download-probe|linux|ACC-09
soem-process-data|root|ACC-11..14
igh-op-probe|root|ACC-11/12
igh-cia402-op-probe|root|ACC-11/12
igh-cia402-csv-op-probe|root|ACC-11/12
igh-el2008-op-probe|root|ACC-11/12
igh-sv660n-op-probe|root|ACC-11/12
igh-cia402-cst-op-probe|root|ACC-11/12
uds-loopback|linux|ACC-12/16
soem-dc-probe|root|ACC-15
igh-dc-op-probe|root|ACC-15
dc-clock-probe|linux|ACC-15
mcp-stdio-probe|any|ACC-26/27 交叉
serve-token-probe|any|ACC-27
sdo-write-probe|linux|ACC-27
sdo-read-probe|linux|ACC-27
topology-probe|linux|ACC-27 交叉
invalid-slave-probe|any|ACC-27②
al-error-probe|linux|ACC-11..13 / AC-E-203 交叉（AL 注入）
"

skip_reason() {
  local kind="$1"
  case "$kind" in
    any) echo "" ;;
    linux)
      [ "$host_kind" = "linux" ] && echo "" || echo "平台不适用（需要 Linux）"
      ;;
    root)
      if [ "$host_kind" != "linux" ]; then echo "平台不适用（需要 Linux TAP）"; return; fi
      if [ "$RUN_ROOT" = "0" ]; then echo "RUN_ROOT=0（本机不跑 root 类）"; return; fi
      if [ "$is_root" != "yes" ]; then echo "缺 root（TAP/内核模块/IgH 需要）"; return; fi
      echo ""
      ;;
    *) echo "未知 kind（注册表错误）" ;;
  esac
}

if [ "${1:-}" = "--list" ]; then
  total=0
  echo "host=$host_kind root=$is_root RUN_ROOT=$RUN_ROOT BIN=$BIN PY=$PY"
  while IFS='|' read -r name kind acc; do
    [ -z "$name" ] && continue
    total=$((total + 1))
    reason="$(skip_reason "$kind")"
    if [ -n "$reason" ]; then
      echo "SKIP $name (kind=$kind acc=$acc) — $reason"
    else
      echo "READY $name (kind=$kind acc=$acc)"
    fi
  done <<EOF
$PROBES
EOF
  echo "== 注册表项数：$total（由脚本自算）=="
  exit 0
fi

command -v "$PY" >/dev/null 2>&1 || {
  echo "FAIL: 找不到 Python：$PY"
  exit 1
}

total=0
pass=0
fail=0
skip=0
failed_names=""
echo "# EC-Sim B-2 验收探针（host=$host_kind root=$is_root BIN=$BIN）"
echo "# name|kind|EXIT|status|sec[|skip_reason]"

while IFS='|' read -r name kind acc; do
  [ -z "$name" ] && continue
  total=$((total + 1))
  script="scripts/spikes/${name}.sh"
  log="$LOG_DIR/${name}.log"
  reason="$(skip_reason "$kind")"
  if [ ! -f "$script" ]; then
    fail=$((fail + 1))
    failed_names="$failed_names $name"
    echo "${name}|${kind}|-|FAIL|0|注册表登记的探针不存在：$script"
    continue
  fi
  if [ -n "$reason" ]; then
    skip=$((skip + 1))
    echo "${name}|${kind}|-|SKIP|0|${reason}"
    continue
  fi
  started="$(date +%s)"
  if [ "$has_timeout" = "yes" ]; then
    BIN="$BIN" PY="$PY" timeout "$TIMEOUT_SECONDS" bash "$script" >"$log" 2>&1
    code=$?
  else
    BIN="$BIN" PY="$PY" bash "$script" >"$log" 2>&1
    code=$?
  fi
  elapsed=$(( $(date +%s) - started ))
  if [ "$code" -eq 0 ]; then
    pass=$((pass + 1))
    echo "${name}|${kind}|0|PASS|${elapsed}"
  elif [ "$code" -eq 2 ]; then
    # 探针约定：exit 2 = 缺夹具/可选依赖（例如验证机本地 SOEM/IgH 构建缺失）→ SKIP 且必须写原因。
    skip=$((skip + 1))
    last="$(tail -n 1 "$log" 2>/dev/null | tr -d '\r' | cut -c1-120)"
    echo "${name}|${kind}|2|SKIP|${elapsed}|探针 exit 2（缺夹具/可选依赖）：${last}"
  elif [ "$code" -eq 124 ]; then
    fail=$((fail + 1))
    failed_names="$failed_names $name"
    echo "${name}|${kind}|124|FAIL|${elapsed}|超时（>${TIMEOUT_SECONDS}s，日志 $log）"
  else
    fail=$((fail + 1))
    failed_names="$failed_names $name"
    last="$(tail -n 1 "$log" 2>/dev/null | tr -d '\r' | cut -c1-120)"
    echo "${name}|${kind}|${code}|FAIL|${elapsed}|exit=${code}（日志 $log）：${last}"
  fi
done <<EOF
$PROBES
EOF

echo "== 汇总：$pass PASS / $fail FAIL / $skip SKIP（共 $total）=="
if [ "$fail" -gt 0 ]; then
  echo "FAILED:$failed_names"
  echo "RESULT: FAIL"
  exit 1
fi
echo "RESULT: PASS"
