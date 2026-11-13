#!/usr/bin/env bash
# task-66 C4 v2：systemd unit 结束后的外部归档/清理核验（VM 上、unit 之外运行，补 C4 证据可复核性）。
# 用法：bash /root/c4v2-postcheck.sh
set -u
OUT="${OUT:-/tmp/hmi-c4-v2}"
UNIT="${UNIT:-ecsim-c4v2}"
TAP="${TAP:-escsim0}"
mkdir -p "$OUT"
{
  echo "postcheck_at=$(date -Is)"
  echo "=== unit ==="
  systemctl is-active "$UNIT" 2>&1 || true
  systemctl show "$UNIT" -p ActiveState -p Result -p ExecMainStatus 2>&1 || true
  echo "=== journal (unit, full) ==="
} >"$OUT/unit.log" 2>&1

{
  echo "postcheck_at=$(date -Is)"
  echo "=== tap ==="
  ip link show "$TAP" 2>&1 || echo "tap_absent"
  echo "=== processes ==="
  pgrep -af "ec-sim-esc --tap $TAP" || echo "sim_absent"
  pgrep -af "simple_ng $TAP" || echo "master_absent"
  echo "=== ports ==="
  ss -ltn 2>/dev/null | grep 18210 || echo "port_free"
} >"$OUT/cleanup-proof-after-unit.txt" 2>&1

# 保留 master.log 里 8 站 / OPERATIONAL 原始行（判 8 站 OP 用）
grep -aE "Slave [0-9]+ is in|OPERATIONAL|SAFE_OP|no slaves found" "$OUT/master.log" 2>/dev/null | tail -400 >"$OUT/master-op-lines.log" || true
echo "POSTCHECK_DONE out=$OUT"
ls -l "$OUT" 2>/dev/null | tail -20
