#!/usr/bin/env python3
"""multiscale_witness.py — Reusable multi-scale monster-hash witness tool.

Splits any file into words, lines, paragraphs, and character n-grams (1..71),
runs monster_hash on each fragment, collects orbifold trajectories at every scale.

The multi-scale orbifold trajectory IS the content fingerprint.

Usage:
    # Witness a single file
    multiscale_witness.py file <path>

    # Witness all files at a resonance cell
    multiscale_witness.py cell <shard_root> <o71> <o59> <o47>

    # Witness files from a resonance analysis
    multiscale_witness.py resonances <shard_root> [--top N]

Output: JSON to stdout (pipe to file or jq as needed)

Requires: ~/03-march/27/monster-hash/target/release/monster_hash

See also: SOP-WITNESS-001 in ~/DOCS/services/orbifold-viz/
"""

import json, os, glob, subprocess, sys
from collections import defaultdict

MHASH = os.path.expanduser("~/03-march/27/monster-hash/target/release/monster_hash")

def hash_fragment(data_bytes):
    """Run monster_hash on raw bytes, return 16-char hex hash or None."""
    try:
        r = subprocess.run([MHASH, "/dev/stdin"], input=data_bytes,
                          capture_output=True, timeout=5)
        for line in r.stdout.decode(errors="replace").splitlines():
            line = line.strip()
            # Parse "hash:  0xabcdef1234567890"
            if line.startswith("hash:"):
                h = line.split("0x")[-1].strip()[:16]
                if len(h) == 16:
                    return h
    except Exception:
        pass
    return None

def orb(h):
    """Convert 16-char hex hash to orbifold coordinates."""
    if not h:
        return None
    n = int(h, 16)
    return [n % 71, n % 59, n % 47]

def witness_file(content):
    """Multi-scale witness of text content. Returns dict of scale → hash list."""
    scales = {}

    # Words (whitespace-split, up to 71)
    scales["words"] = []
    for w in content.split()[:71]:
        h = hash_fragment(w.encode())
        if h:
            scales["words"].append({"text": w[:20], "hash": h, "orb": orb(h)})

    # Lines (up to 71 non-empty)
    scales["lines"] = []
    for ln in content.splitlines()[:71]:
        if not ln.strip():
            continue
        h = hash_fragment(ln.encode())
        if h:
            scales["lines"].append({"text": ln[:40], "hash": h, "orb": orb(h)})

    # Paragraphs (double-newline split, up to 71)
    scales["paragraphs"] = []
    for p in [p.strip() for p in content.split("\n\n") if p.strip()][:71]:
        h = hash_fragment(p.encode())
        if h:
            scales["paragraphs"].append({"text": p[:60], "hash": h, "orb": orb(h)})

    # Character n-grams 1..71 (prefix of increasing length)
    scales["ngrams"] = []
    for n in range(1, min(72, len(content) + 1)):
        h = hash_fragment(content[:n].encode())
        if h:
            scales["ngrams"].append({"n": n, "hash": h, "orb": orb(h)})

    # Full file
    h = hash_fragment(content.encode())
    if h:
        scales["full"] = {"hash": h, "orb": orb(h)}

    return scales

def load_shards(shard_root):
    """Load all shard JSONs, grouped by orbifold cell."""
    by_cell = defaultdict(list)
    for d in sorted(os.listdir(shard_root)):
        dp = os.path.join(shard_root, d)
        if not os.path.isdir(dp):
            continue
        for f in glob.glob(os.path.join(dp, "*.json")):
            try:
                s = json.load(open(f))
                s["_corpus"] = d
                by_cell[tuple(s.get("orbifold", [0, 0, 0]))].append(s)
            except Exception:
                continue
    return by_cell

def cmd_file(args):
    """Witness a single file."""
    path = args[0]
    content = open(path, "r", errors="replace").read()
    result = {"path": path, "size": len(content), "scales": witness_file(content)}
    json.dump(result, sys.stdout, indent=2, ensure_ascii=False)

def cmd_cell(args):
    """Witness all files at a specific orbifold cell."""
    shard_root, o71, o59, o47 = args[0], int(args[1]), int(args[2]), int(args[3])
    by_cell = load_shards(shard_root)
    cell = (o71, o59, o47)
    shards = by_cell.get(cell, [])
    if not shards:
        print(f"No shards at cell {list(cell)}", file=sys.stderr)
        sys.exit(1)
    # TODO: resolve paths and witness each file
    json.dump({"cell": list(cell), "shards": len(shards)}, sys.stdout, indent=2)

def cmd_resonances(args):
    """Witness top N resonance cells."""
    shard_root = args[0]
    top_n = int(args[1]) if len(args) > 1 else 3
    by_cell = load_shards(shard_root)
    resonances = {c: ss for c, ss in by_cell.items() if len(set(s["_corpus"] for s in ss)) > 1}
    print(f"{len(resonances)} resonance cells", file=sys.stderr)
    # Output summary
    results = []
    for cell in sorted(resonances, key=lambda c: -len(resonances[c]))[:top_n]:
        shards = resonances[cell]
        results.append({
            "cell": list(cell),
            "n_shards": len(shards),
            "corpora": sorted(set(s["_corpus"] for s in shards)),
        })
    json.dump(results, sys.stdout, indent=2, ensure_ascii=False)

def usage():
    print(__doc__)
    sys.exit(1)

if __name__ == "__main__":
    if len(sys.argv) < 2:
        usage()
    cmd = sys.argv[1]
    if cmd == "file":
        cmd_file(sys.argv[2:])
    elif cmd == "cell":
        cmd_cell(sys.argv[2:])
    elif cmd == "resonances":
        cmd_resonances(sys.argv[2:])
    else:
        usage()
