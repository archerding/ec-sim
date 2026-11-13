#!/usr/bin/env python3
"""task-78：生成 T-PERF-06 用合成 dict-v1（固定 seed、可复现；明确标注非真实字典）。

用法：python3 scripts/spikes/make-synthetic-dict.py <out.json> [object_count=2000]
输出：out.json + 同目录 out.sha256.txt（含 seed/count/sha256/说明）。
"""
import hashlib
import json
import random
import sys
from pathlib import Path

out = Path(sys.argv[1] if len(sys.argv) > 1 else "synthetic-dict-v1.json")
count = int(sys.argv[2]) if len(sys.argv) > 2 else 2000
SEED = 42
random.seed(SEED)

objects = []
entries_total = 0
for index in range(count):
    base = 0x2000 + index
    objects.append({
        "index": f"0x{base:04X}",
        "name": f"Synthetic Object {index}",
        "code": "VAR",
        "entries": [{
            "sub": 0,
            "name": f"Synthetic Entry {index}",
            "type": "UNSIGNED32",
            "bits": 32,
            "access": "rw" if index % 3 == 0 else "ro",
            "value_hex": f"{(random.getrandbits(32)):08x}",
        }],
    })
    entries_total += 1

data = {
    "schema": "ec-sim/dict-v1",
    "object_count": objects.__len__(),
    "entry_count": entries_total,
    "objects": objects,
    "note": "SYNTHETIC/desensitized for T-PERF-06 search benchmark only; NOT the real 8-slave dictionary (real ~18 objects).",
}
out.parent.mkdir(parents=True, exist_ok=True)
text = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
out.write_text(text + "\n", encoding="utf-8")
digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
(out.with_suffix(".sha256.txt")).write_text(
    f"file={out.name}\nseed={SEED}\nobject_count={len(objects)}\nentry_count={entries_total}\nsha256={digest}\n"
    f"note=SYNTHETIC/desensitized; not real 8-slave dictionary (real ~18 objects)\n",
    encoding="utf-8",
)
print(f"OK {out} objects={len(objects)} entries={entries_total} seed={SEED} sha256={digest}")
