#!/usr/bin/env bash
# 构建单次连续 SOEM 主站长跑程序（EC-Sim VM，链接 /opt/ecsim/fixtures/soem 的 libsoem.a）。
set -euo pipefail
SRC="${SRC:-$(dirname "$0")/ecsim-master-long.c}"
SOEM_ROOT="${SOEM_ROOT:-/opt/ecsim/fixtures/soem}"
OUT="${OUT:-/root/ecsim-master-long}"
CFLAGS="${CFLAGS:--O2}"
LIBS=""
if [ -f "$SOEM_ROOT/build/libsoem.a" ]; then LIBS="$SOEM_ROOT/build/libsoem.a -lpthread -lrt"; fi
if [ -z "$LIBS" ] && [ -f "$SOEM_ROOT/build/install/lib/libsoem.a" ]; then LIBS="$SOEM_ROOT/build/install/lib/libsoem.a -lpthread -lrt"; fi
if [ -z "$LIBS" ]; then LIBS="$(find "$SOEM_ROOT" -name 'libsoem.a' 2>/dev/null | head -1) -lpthread -lrt"; fi
if [ -z "$LIBS" ] || [ ! -f "$(echo "$LIBS" | awk '{print $1}')" ]; then
  echo "FAIL: libsoem.a not found under $SOEM_ROOT"; exit 2
fi
cc $CFLAGS -I"$SOEM_ROOT/build/include" -I"$SOEM_ROOT/include" -I"$SOEM_ROOT/osal" -I"$SOEM_ROOT/osal/linux" \
  -I"$SOEM_ROOT/oshw" -I"$SOEM_ROOT/oshw/linux" -o "$OUT" "$SRC" $LIBS
echo "BUILT $OUT"
sha256sum "$OUT"
