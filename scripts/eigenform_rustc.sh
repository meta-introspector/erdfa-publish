#!/bin/bash
# SOP-EIGEN-001: Build the eigenform of rustc from its eigenvalue.
#
# Trace the full compilation tower through monster-hash at each stage:
#   Grade 0: source files (eigenvalue)
#   Grade 1: cargo metadata / AST (eigenvector)
#   Grade 2: build artifacts / deps (eigensurface)
#   Grade 3: final binary (eigenform)
#
# Each stage is monster-hashed and the orbifold trajectory is recorded.
# The compilation itself is traced with zkperf (time, size, hash per stage).
#
# Usage: eigenform_rustc.sh <project_dir> [output_json]

set -euo pipefail

PROC_ID="SOP-EIGEN-001"
MHASH="$HOME/03-march/27/monster-hash/target/release/monster_hash"
WITNESS="$HOME/erdfa-publish/scripts/multiscale_witness.py"
PROJECT="${1:?Usage: eigenform_rustc.sh <project_dir> [output_json]}"
OUT="${2:-$PROJECT/eigenform_trace.json}"

echo "[$PROC_ID] Building eigenform of rustc for: $PROJECT"

# ── Grade 0: Eigenvalue (source hash) ──────────────────────────
echo "[$PROC_ID] Grade 0: eigenvalue (source files)"
G0_START=$(date +%s%N)
G0_HASH=$("$MHASH" "$PROJECT/src/lib.rs" 2>&1 | grep "hash:" | awk '{print $2}')
G0_GRADE=$("$MHASH" "$PROJECT/src/lib.rs" 2>&1 | grep "grade:" | awk '{print $2}')
G0_SIZE=$(stat -c%s "$PROJECT/src/lib.rs" 2>/dev/null || echo 0)
G0_END=$(date +%s%N)
G0_MS=$(( (G0_END - G0_START) / 1000000 ))
echo "  hash=$G0_HASH grade=$G0_GRADE size=$G0_SIZE time=${G0_MS}ms"

# ── Grade 1: Eigenvector (Cargo.toml = dependency vector) ──────
echo "[$PROC_ID] Grade 1: eigenvector (Cargo.toml)"
G1_START=$(date +%s%N)
G1_HASH=$("$MHASH" "$PROJECT/Cargo.toml" 2>&1 | grep "hash:" | awk '{print $2}')
G1_GRADE=$("$MHASH" "$PROJECT/Cargo.toml" 2>&1 | grep "grade:" | awk '{print $2}')
G1_SIZE=$(stat -c%s "$PROJECT/Cargo.toml" 2>/dev/null || echo 0)
G1_END=$(date +%s%N)
G1_MS=$(( (G1_END - G1_START) / 1000000 ))
echo "  hash=$G1_HASH grade=$G1_GRADE size=$G1_SIZE time=${G1_MS}ms"

# ── Compile (the Hecke operator) ───────────────────────────────
echo "[$PROC_ID] Hecke operator: make build"
BUILD_START=$(date +%s%N)
cd "$PROJECT"
make build 2>&1 | tail -3
BUILD_END=$(date +%s%N)
BUILD_MS=$(( (BUILD_END - BUILD_START) / 1000000 ))
echo "  compile_time=${BUILD_MS}ms"

# ── Grade 2: Eigensurface (Cargo.lock = resolved dep graph) ────
echo "[$PROC_ID] Grade 2: eigensurface (Cargo.lock)"
G2_START=$(date +%s%N)
G2_HASH=$("$MHASH" "$PROJECT/Cargo.lock" 2>&1 | grep "hash:" | awk '{print $2}')
G2_GRADE=$("$MHASH" "$PROJECT/Cargo.lock" 2>&1 | grep "grade:" | awk '{print $2}')
G2_SIZE=$(stat -c%s "$PROJECT/Cargo.lock" 2>/dev/null || echo 0)
G2_END=$(date +%s%N)
G2_MS=$(( (G2_END - G2_START) / 1000000 ))
echo "  hash=$G2_HASH grade=$G2_GRADE size=$G2_SIZE time=${G2_MS}ms"

# ── Grade 3: Eigenform (compiled binary) ───────────────────────
echo "[$PROC_ID] Grade 3: eigenform (binary)"
# Find the binary
BIN=$(ls -t "$PROJECT/target/release/"*monster_hash* 2>/dev/null | grep -v '\.d$\|\.rlib$' | head -1)
if [ -z "$BIN" ]; then
  BIN=$(ls -t "$PROJECT/target/release/"*.so 2>/dev/null | head -1)
fi
if [ -z "$BIN" ]; then
  BIN=$(ls -t "$PROJECT"/target/release/deps/*.rlib 2>/dev/null | head -1)
fi

if [ -n "$BIN" ]; then
  G3_START=$(date +%s%N)
  G3_HASH=$("$MHASH" "$BIN" 2>&1 | grep "hash:" | awk '{print $2}')
  G3_GRADE=$("$MHASH" "$BIN" 2>&1 | grep "grade:" | awk '{print $2}')
  G3_SIZE=$(stat -c%s "$BIN" 2>/dev/null || echo 0)
  G3_END=$(date +%s%N)
  G3_MS=$(( (G3_END - G3_START) / 1000000 ))
  echo "  binary=$BIN"
  echo "  hash=$G3_HASH grade=$G3_GRADE size=$G3_SIZE time=${G3_MS}ms"
else
  echo "  ⚠ No binary found"
  G3_HASH="none"
  G3_GRADE=0
  G3_SIZE=0
  G3_MS=0
fi

# ── Orbifold coordinates per grade ─────────────────────────────
orb() {
  local h="${1#0x}"
  python3 -c "h=int('$h',16); print(f'[{h%71},{h%59},{h%47}]')"
}

G0_ORB=$(orb "$G0_HASH")
G1_ORB=$(orb "$G1_HASH")
G2_ORB=$(orb "$G2_HASH")
G3_ORB=$(orb "$G3_HASH" 2>/dev/null || echo "[0,0,0]")

echo ""
echo "[$PROC_ID] === EIGENFORM TRACE ==="
echo "  Grade 0 (source):  $G0_HASH → orb=$G0_ORB  ${G0_MS}ms"
echo "  Grade 1 (deps):    $G1_HASH → orb=$G1_ORB  ${G1_MS}ms"
echo "  Hecke (compile):   ${BUILD_MS}ms"
echo "  Grade 2 (lock):    $G2_HASH → orb=$G2_ORB  ${G2_MS}ms"
echo "  Grade 3 (binary):  $G3_HASH → orb=$G3_ORB  ${G3_MS}ms"

# ── Write JSON trace ───────────────────────────────────────────
cat > "$OUT" << JSONEOF
{
  "project": "$PROJECT",
  "grades": [
    {"grade": 0, "name": "eigenvalue",   "file": "src/lib.rs",   "hash": "$G0_HASH", "orbifold": $G0_ORB, "grade_cl": $G0_GRADE, "size": $G0_SIZE, "time_ms": $G0_MS},
    {"grade": 1, "name": "eigenvector",  "file": "Cargo.toml",   "hash": "$G1_HASH", "orbifold": $G1_ORB, "grade_cl": $G1_GRADE, "size": $G1_SIZE, "time_ms": $G1_MS},
    {"grade": 2, "name": "eigensurface", "file": "Cargo.lock",   "hash": "$G2_HASH", "orbifold": $G2_ORB, "grade_cl": $G2_GRADE, "size": $G2_SIZE, "time_ms": $G2_MS},
    {"grade": 3, "name": "eigenform",    "file": "$BIN",         "hash": "$G3_HASH", "orbifold": $G3_ORB, "grade_cl": $G3_GRADE, "size": $G3_SIZE, "time_ms": $G3_MS}
  ],
  "hecke": {"operator": "cargo build --release", "time_ms": $BUILD_MS},
  "total_ms": $((G0_MS + G1_MS + BUILD_MS + G2_MS + G3_MS))
}
JSONEOF

echo "[$PROC_ID] ✅ $OUT"
