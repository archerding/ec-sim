#!/usr/bin/env bash
# task-66 C4 v2：持续主站流量（SOEM simple_ng 循环）——让 --serve 统计 frames_in>0、8 站进 OP。
# 仅 VM 使用；不重启、不修改系统配置。
set -u
TAP="${TAP:-escsim0}"
SOEM="${SOEM:-/opt/ecsim/fixtures/soem/build/samples/simple_ng/simple_ng}"
LOG="${LOG:-/tmp/hmi-c4-v2-master.log}"
echo "MASTER_LOOP start tap=$TAP bin=$SOEM $(date +%Is)" >>"$LOG"
while :; do
  "$SOEM" "$TAP" >>"$LOG" 2>&1 || true
  echo "MASTER_LOOP restart $(date +%Is)" >>"$LOG"
  sleep 1
done
