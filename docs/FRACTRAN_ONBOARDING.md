# fractran-vm → erdfa-publish: Experiment Pipeline Onboarding

## What This Is

fractran-vm runs Clifford algebra stability experiments on sporadic groups and error-correcting codes. erdfa-publish packages the results as DA51 CBOR shards for archival, peer review, and cross-referencing.

This guide shows how to go from raw experiment → published shards.

## Prerequisites

```bash
# fractran-vm (the experiment engine)
cd ~/.emacs.d.kiro/kiro.el-research/fractran-vm
nix develop --command cargo build

# erdfa-publish (the shard publisher)
cd ~/erdfa-publish
cargo build --release
```

## Step 1: Run an Experiment

### Sporadic group stability (all 26 groups)

```bash
cd ~/.emacs.d.kiro/kiro.el-research/fractran-vm

# Atlas mode: all 26 at default strength
nix develop --command cargo run -- multisize 1000

# Critical mode: all 26 at specific strength (finds breaking points)
nix develop --command cargo run -- multisize 1000 critical 1e-3

# Stress sweep: find each group's breaking point
nix develop --command cargo run -- multisize 1000 stress
```

### Error-correcting code scan (1049 codes from eczoo)

```bash
# Scan all codes, rank by monster-likeness/stability/size
nix develop --command cargo run -- eczoo \
  /mnt/data1/introspector/shards/monster-experiments/examples/eczoo_data/codes \
  1e-2 500
```

### Cross-reference (codes ↔ groups)

```bash
# Side-by-side stability curves + best match per group
nix develop --command cargo run -- cross 1000
```

## Step 2: Capture Output as Shards

Pipe experiment output into erdfa-cli:

```bash
# Capture multisize results
nix develop --command cargo run -- multisize 1000 critical 1e-3 2>&1 \
  | erdfa-cli import --src /dev/stdin --dir shards/multisize/ --max-depth 1

# Capture eczoo rankings
nix develop --command cargo run -- eczoo \
  /mnt/data1/introspector/shards/monster-experiments/examples/eczoo_data/codes \
  1e-2 500 2>&1 \
  | erdfa-cli import --src /dev/stdin --dir shards/eczoo/ --max-depth 1

# Capture cross-reference
nix develop --command cargo run -- cross 1000 2>&1 \
  | erdfa-cli import --src /dev/stdin --dir shards/cross/ --max-depth 1
```

## Step 3: Create Structured Result Shards

For richer metadata, create shards programmatically:

```rust
use erdfa_publish::{Component, Shard, ShardSet};

// A sporadic group result
let m24 = Shard::new("sporadic-M24", Component::KeyValue {
    pairs: vec![
        ("group".into(), "M24".into()),
        ("clifford".into(), "Cl(6)".into()),
        ("order_factored".into(), "2^10 · 3^3 · 5 · 7 · 11 · 23".into()),
        ("area_1e-3".into(), "1.000".into()),
        ("area_1e-2".into(), "0.154".into()),
        ("break_strength".into(), "5e-3".into()),
        ("best_eczoo_match".into(), "rhombic_dodecahedron_surface [14,3,3]".into()),
        ("match_distance".into(), "0.016".into()),
    ],
}).with_tags(vec!["sporadic".into(), "clifford".into(), "stability".into()]);

// A cross-reference result
let cross = Shard::new("cross-J4-golay", Component::KeyValue {
    pairs: vec![
        ("group".into(), "J4".into()),
        ("code".into(), "golay [23,12,7]".into()),
        ("curve_distance".into(), "0.006".into()),
        ("match_type".into(), "≡".into()),
        ("note".into(), "pariah tracks the Golay code".into()),
    ],
}).with_tags(vec!["cross-reference".into(), "pariah".into()]);

// Package as tar
let shards = vec![m24, cross];
let manifest = ShardSet::from_shards("clifford-stability-2026-03-24", &shards);
let mut f = std::fs::File::create("stability.tar").unwrap();
manifest.to_tar(&shards, &mut f).unwrap();
```

## Step 4: Publish

```bash
# List what you've got
erdfa-cli list shards/

# View a shard
erdfa-cli show shards/sporadic-M24.cbor

# Post for peer review
cat shards/sporadic-M24.cbor | erdfa-cli show /dev/stdin | pastebinit
```

## The Data You're Publishing

### Sporadic Group Rankings (26 groups)

Each group has:
- **Name**: M11, M12, ..., M (Monster)
- **Cl(N)**: which Clifford algebra it lives in (4 ≤ N ≤ 15)
- **SSP exponents**: factorization over the 15 supersingular primes
- **Stability curve**: area at strengths 1e-6 through 1e-1
- **Break strength**: where area drops below 0.5

Key result: J2 in Cl(4) is weakest (breaks at 1e-3). Monster in Cl(15) holds to 1e-2.

### eczoo Code Rankings (66 parsed from 1049)

Each code has:
- **code_id**: from the Error Correction Zoo
- **[n,k,d]**: code parameters
- **Prime signature**: factorization of n,k,d over SSP
- **Monster cosine**: similarity to Monster's prime signature
- **Stability**: area under rotor perturbation

Key result: quantum_dodecahedron [16,4,3] is the most monster-like survivor (cos=0.914, area=1.0 at str=1e-2).

### Cross-Reference (26 groups × 66 codes)

For each sporadic group, the eczoo code with the closest stability curve (L2 distance over 4 test strengths).

Key result: 14/26 groups have a near-identical match (dist < 0.01). The pariah J4 tracks the Golay code (dist=0.006).

## Shard Schema

### Sporadic group shard

```json
{
  "id": "sporadic-<name>",
  "component": {
    "type": "KeyValue",
    "pairs": [
      ["group", "<name>"],
      ["clifford", "Cl(<N>)"],
      ["ssp_exponents", "<comma-separated>"],
      ["area_1e-3", "<float>"],
      ["area_1e-2", "<float>"],
      ["break_strength", "<float>"],
      ["best_eczoo_match", "<code_id> [n,k,d]"],
      ["match_distance", "<float>"]
    ]
  },
  "tags": ["sporadic", "clifford", "stability"]
}
```

### eczoo code shard

```json
{
  "id": "eczoo-<code_id>",
  "component": {
    "type": "KeyValue",
    "pairs": [
      ["code_id", "<id>"],
      ["params", "[n,k,d]"],
      ["monster_cosine", "<float>"],
      ["area", "<float>"],
      ["clifford", "Cl(<N>)"]
    ]
  },
  "tags": ["eczoo", "error-correction", "stability"]
}
```

### Cross-reference shard

```json
{
  "id": "cross-<group>-<code_id>",
  "component": {
    "type": "KeyValue",
    "pairs": [
      ["group", "<name>"],
      ["code", "<code_id> [n,k,d]"],
      ["curve_distance", "<float>"],
      ["match_type", "≡|≈|~"],
      ["signature_cosine", "<float>"]
    ]
  },
  "tags": ["cross-reference", "clifford"]
}
```

## Ranking Table as a Shard

```rust
let ranking = Shard::new("monster-ranking-top10", Component::Table {
    headers: vec![
        "Group".into(), "Cl(N)".into(), "area@1e-3".into(),
        "Best eczoo".into(), "dist".into(),
    ],
    rows: vec![
        vec!["Th".into(), "Cl(7)".into(), "1.000".into(), "stab_11_1_5".into(), "0.001".into()],
        vec!["Co3".into(), "Cl(6)".into(), "1.000".into(), "gross [144,12,12]".into(), "0.001".into()],
        vec!["Suz".into(), "Cl(6)".into(), "1.000".into(), "gross [144,12,12]".into(), "0.002".into()],
        vec!["J4⚡".into(), "Cl(10)".into(), "1.000".into(), "golay [23,12,7]".into(), "0.006".into()],
        vec!["M".into(), "Cl(15)".into(), "1.000".into(), "bravyi_bacon_shor_6".into(), "0.015".into()],
    ],
}).with_tags(vec!["ranking".into(), "cross-reference".into()]);
```

## The Loop

```
fractran-vm experiment
        │
        ▼
   stdout (rankings, curves, cross-refs)
        │
        ▼
   erdfa-cli import → CBOR shards
        │
        ▼
   erdfa-cli list/show → inspect
        │
        ▼
   pastebinit → peer review
        │
        ▼
   refine parameters → run again
```

## Quick Reference

| What | Command |
|------|---------|
| All 26 groups, default | `fractran-vm multisize 1000` |
| Find breaking points | `fractran-vm multisize 1000 stress` |
| All at one strength | `fractran-vm multisize 1000 critical 1e-3` |
| Scan eczoo codes | `fractran-vm eczoo <path> <strength> <steps>` |
| Cross-reference | `fractran-vm cross 1000` |
| Import results | `erdfa-cli import --src /dev/stdin --dir shards/` |
| View shard | `erdfa-cli show shards/foo.cbor` |
| List shards | `erdfa-cli list shards/` |

## The 15 Supersingular Primes

```
Index:  0   1   2   3   4   5   6   7   8   9  10  11  12  13  14
Prime:  2   3   5   7  11  13  17  19  23  29  31  41  47  59  71
```

These are the primes dividing |M| (the Monster group's order) and also the primes p where the modular curve X₀(p)⁺ has genus 0. The Clifford algebra Cl(15,0,0) has one basis vector per prime. This is the bridge between group theory, error correction, and the meme farm.
