#!/usr/bin/env bash
set -euo pipefail

PKG="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cd "$PKG"
sha256sum -c SHA256SUMS

LAB="$(mktemp -d "$HOME/ZOS_PUB06E15I.XXXXXXXX")"

mkdir -p \
  "$LAB/cargo" \
  "$LAB/synthetic-flatfs" \
  "$LAB/path-audit" \
  "$LAB/secure-gate" \
  "$LAB/concurrency"

cp -a "$PKG/cargo/." "$LAB/cargo/"

export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR="$LAB/target"
export ZOS_TEST_FLATFS="$LAB/synthetic-flatfs"
export ZOS_PATH_AUDIT_ROOT="$LAB/path-audit"
export ZOS_SECURE_GATE_ROOT="$LAB/secure-gate"
export ZOS_CONCURRENCY_ROOT="$LAB/concurrency"

cd "$LAB/cargo"

cargo test --offline --locked -- --test-threads=1 \
  > "$LAB/tests.stdout" \
  2> "$LAB/tests.stderr" || {
    cat "$LAB/tests.stderr"
    cat "$LAB/tests.stdout"
    exit 5
  }

python3 - "$LAB/tests.stdout" <<'PY'
from pathlib import Path
import re
import sys

s = Path(sys.argv[1]).read_text()
names = re.findall(
    r"(?m)^test ([A-Za-z0-9_:]+) \.\.\. ok$",
    s,
)

assert len(names) == 40
assert len(set(names)) == 40
assert re.search(
    r"test result: ok\. 40 passed; 0 failed;",
    s,
)

print("OFFLINE_REPLAY=40/40_PASS")
PY

echo "REPLAY_LAB=$LAB"
