#!/usr/bin/env bash
(
  set -euo pipefail

  ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
  cd "$ROOT"

  echo "=== VERIFY PACKAGE SHA256 ==="
  sha256sum -c SHA256SUMS

  echo "=== VERIFY FIXED WITNESS ==="
  printf '%s\n' \
    'df436530f2716cf5f9c92261374638352d4c5e0687e498d1c04b5bd088fb69ec  witness/LEAN_WITNESS_DA51.cbor' \
    | sha256sum -c -

  echo "=== VERIFY LEAN VERSION ==="
  lean --version

  LAB="$(mktemp -d)"
  trap 'rm -rf -- "$LAB"' EXIT

  cp proofs/*.lean "$LAB/"
  cd "$LAB"
  export LEAN_PATH="$LAB"

  echo "=== REBUILD LEAN PROOFS ==="
  timeout 120s lean \
    -o SemanticWitnesses.olean SemanticWitnesses.lean
  timeout 120s lean \
    -o ExactTypes.olean ExactTypes.lean

  timeout 60s lean TrustQuery.lean > trust.stdout

  cmp trust.stdout \
    "$ROOT/evidence/trust.stdout"

  echo "LEAN_REPLAY=PASS"

  echo "=== VERIFY STRICT CBOR ==="
  python3 -B - "$ROOT" <<'PY'
from pathlib import Path
import importlib.util
import sys

root = Path(sys.argv[1])
p = root / "validator/strict_da51.py"

spec = importlib.util.spec_from_file_location(
    "strict_da51_handoff", p
)
if spec is None or spec.loader is None:
    raise SystemExit("FAIL: VALIDATOR_LOAD")

codec = importlib.util.module_from_spec(spec)
spec.loader.exec_module(codec)

raw = (
    root / "witness/LEAN_WITNESS_DA51.cbor"
).read_bytes()

obj = codec.validate(raw)

if len(raw) != 1103 or len(obj.get("claims", [])) != 4:
    raise SystemExit("FAIL: WITNESS_CONTRACT")

print("STRICT_CBOR=PASS")
print("CLAIMS=4")
PY

  echo "PACKAGE_OFFLINE_REPLAY=PASS"
)
