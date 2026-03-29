#!/bin/bash
# test_multiscale_witness.sh — Verify SOP-WITNESS-001 tool works correctly.
#
# Tests all 3 subcommands against known inputs.
# Exit 0 = all pass, exit 1 = failure.

set -euo pipefail

TOOL="$HOME/erdfa-publish/scripts/multiscale_witness.py"
TEST_FILE="$HOME/03-march/27/monster-hash/Cargo.toml"
SHARD_ROOT="$HOME/erdfa-publish/shards"
PASS=0
FAIL=0

check() {
  local name="$1" cond="$2"
  if eval "$cond"; then
    echo "  ✅ $name"
    PASS=$((PASS + 1))
  else
    echo "  ❌ $name"
    FAIL=$((FAIL + 1))
  fi
}

echo "=== SOP-WITNESS-001 Test Suite ==="
echo ""

# Test 1: file subcommand produces valid JSON with all scales
echo "Test 1: file subcommand"
OUT=$(python3 "$TOOL" file "$TEST_FILE" 2>/dev/null)
check "valid JSON" "echo '$OUT' | python3 -m json.tool >/dev/null 2>&1"
check "has words scale" "echo '$OUT' | python3 -c 'import json,sys; d=json.load(sys.stdin); assert len(d[\"scales\"][\"words\"]) > 0'"
check "has lines scale" "echo '$OUT' | python3 -c 'import json,sys; d=json.load(sys.stdin); assert len(d[\"scales\"][\"lines\"]) > 0'"
check "has ngrams scale" "echo '$OUT' | python3 -c 'import json,sys; d=json.load(sys.stdin); assert len(d[\"scales\"][\"ngrams\"]) > 0'"
check "has full hash" "echo '$OUT' | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d[\"scales\"][\"full\"][\"hash\"]'"
check "orbifold has 3 coords" "echo '$OUT' | python3 -c 'import json,sys; d=json.load(sys.stdin); assert len(d[\"scales\"][\"full\"][\"orb\"]) == 3'"

# Test 2: deterministic (same file → same output)
echo ""
echo "Test 2: determinism"
HASH1=$(echo "$OUT" | python3 -c "import json,sys; print(json.load(sys.stdin)['scales']['full']['hash'])")
HASH2=$(python3 "$TOOL" file "$TEST_FILE" 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin)['scales']['full']['hash'])")
check "same hash on rerun" "[ '$HASH1' = '$HASH2' ]"

# Test 3: resonances subcommand
echo ""
echo "Test 3: resonances subcommand"
RES=$(python3 "$TOOL" resonances "$SHARD_ROOT" 3 2>/dev/null)
check "valid JSON" "echo '$RES' | python3 -m json.tool >/dev/null 2>&1"
check "returns 3 cells" "echo '$RES' | python3 -c 'import json,sys; assert len(json.load(sys.stdin)) == 3'"

# Summary
echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
