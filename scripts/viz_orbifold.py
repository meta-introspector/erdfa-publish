#!/usr/bin/env python3
"""SOP-VIZ-001: Visualize ctuning corpus orbifold distribution.

ITIL: Service Asset — visual dashboard for ingested shards
ISO 9001: Measurement & Analysis — orbifold coordinate distribution
Six Sigma: Measure phase — identify clustering, gaps, anomalies
C4/PlantUML: Context level — corpus → monster-hash → orbifold → SVG

Inputs:  ~/erdfa-publish/shards/ctuning/*.json
Outputs: /var/www/solana.solfunmeme.com/retro-sync/scratch/ctuning_orbifold.svg

DASL Type 6 · Shard (41,31,37) · Earth · Bott 1(C) · T_5
"""

import json, os, glob, math, sys

def load_shards(shard_dir):
    shards = []
    for f in glob.glob(os.path.join(shard_dir, "*.json")):
        with open(f) as fh:
            try:
                shards.append(json.load(fh))
            except json.JSONDecodeError:
                continue
    return shards

def build_svg(shards, title, out_path):
    W, H = 900, 750
    mx, my = 50, 50
    sx = (W - 2*mx) / 71.0
    sy = (H - 2*my) / 59.0

    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}">',
        f'<rect width="100%" height="100%" fill="#0a0a0a"/>',
        f'<text x="450" y="30" text-anchor="middle" fill="#ffd700" font-size="14" font-family="monospace">{title} · {len(shards)} shards · orbifold (mod 71, 59, 47)</text>',
        f'<text x="450" y="740" text-anchor="middle" fill="#666" font-size="10" font-family="monospace">o71 →</text>',
        f'<text x="12" y="380" text-anchor="middle" fill="#666" font-size="10" font-family="monospace" transform="rotate(-90,12,380)">o59 →</text>',
    ]

    # Grid
    for i in range(0, 72, 10):
        x = mx + i * sx
        lines.append(f'<line x1="{x:.0f}" y1="{my}" x2="{x:.0f}" y2="{H-my}" stroke="#222"/>')
        lines.append(f'<text x="{x:.0f}" y="{H-my+14}" text-anchor="middle" fill="#444" font-size="8" font-family="monospace">{i}</text>')
    for i in range(0, 60, 10):
        y = H - my - i * sy
        lines.append(f'<line x1="{mx}" y1="{y:.0f}" x2="{W-mx}" y2="{y:.0f}" stroke="#222"/>')
        lines.append(f'<text x="{mx-4}" y="{y+3:.0f}" text-anchor="end" fill="#444" font-size="8" font-family="monospace">{i}</text>')

    # Points
    for s in shards:
        o = s.get("orbifold", [0,0,0])
        sz = s.get("size", 1)
        x = mx + o[0] * sx
        y = H - my - o[1] * sy
        r = max(1.2, min(7, math.log2(max(sz, 1)) / 3))
        hue = int(o[2] * 360 / 47)
        path = s.get("path", "")
        lines.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r:.1f}" fill="hsl({hue},75%,50%)" opacity="0.6">'
                     f'<title>{path} ({sz}B) orb=({o[0]},{o[1]},{o[2]})</title></circle>')

    # Legend
    lines.append(f'<text x="{W-140}" y="70" fill="#888" font-size="10" font-family="monospace">color = o47</text>')
    for v in range(0, 47, 8):
        hue = int(v * 360 / 47)
        yy = 85 + (v // 8) * 16
        lines.append(f'<circle cx="{W-135}" cy="{yy}" r="5" fill="hsl({hue},75%,50%)"/>')
        lines.append(f'<text x="{W-125}" y="{yy+4}" fill="#888" font-size="9" font-family="monospace">{v}</text>')

    # Stats
    sizes = [s.get("size", 0) for s in shards]
    total_mb = sum(sizes) / 1048576
    lines.append(f'<text x="{W-140}" y="{H-80}" fill="#888" font-size="9" font-family="monospace">{len(shards)} files</text>')
    lines.append(f'<text x="{W-140}" y="{H-65}" fill="#888" font-size="9" font-family="monospace">{total_mb:.0f} MB</text>')

    lines.append('</svg>')

    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w") as f:
        f.write("\n".join(lines))

def main():
    shard_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/erdfa-publish/shards/ctuning")
    out_path = sys.argv[2] if len(sys.argv) > 2 else "/var/www/solana.solfunmeme.com/retro-sync/scratch/ctuning_orbifold.svg"

    shards = load_shards(shard_dir)
    print(f"[SOP-VIZ-001] Loaded {len(shards)} shards from {shard_dir}")

    build_svg(shards, "ctuning corpus · monster-hash", out_path)
    print(f"[SOP-VIZ-001] ✅ {out_path}")
    print(f"→ https://solana.solfunmeme.com/retro-sync/scratch/ctuning_orbifold.svg")

if __name__ == "__main__":
    main()
