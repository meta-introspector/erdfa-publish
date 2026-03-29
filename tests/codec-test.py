#!/usr/bin/env python3
"""codec-test.py — Compare PSNR of transcoded images against reference.

Usage: python3 scripts/codec-test.py <ref.png> <test_dir>
Expects test_dir to contain transcoded files from the Makefile.
"""

import subprocess, os, sys

def psnr(ref, test):
    try:
        r = subprocess.run(
            ['magick', 'compare', '-metric', 'PSNR', ref, test, '/dev/null'],
            capture_output=True, text=True, timeout=30)
        return float((r.stderr or r.stdout).strip().split()[0])
    except:
        return 0.0

def main():
    ref = sys.argv[1]
    out = sys.argv[2]

    # Normalize all test files to 1024x1024 PNG for comparison
    for f in os.listdir(out):
        if f.startswith('t_') or f.startswith('v_') or f in ('t.jpg','t.webp','t.gif'):
            src = os.path.join(out, f)
            dst = os.path.join(out, f'n_{os.path.splitext(f)[0]}.png')
            subprocess.run(
                f'convert {src} -resize 1024x1024! {dst}',
                shell=True, capture_output=True, timeout=30)

    print(f"\n{'Format':<20} {'PSNR':>8} {'Size':>8} {'Verdict':>12}")
    print('-' * 50)

    survived = 0
    total = 0
    for f in sorted(os.listdir(out)):
        if not f.startswith('n_'):
            continue
        path = os.path.join(out, f)
        p = psnr(ref, path)
        sz = os.path.getsize(path) // 1024
        name = f.replace('n_', '').replace('.png', '')
        total += 1

        if p > 40:   verdict = "✅ perfect"
        elif p > 30:  verdict = "✅ good"
        elif p > 20:  verdict = "⚠ lossy"; survived += 1
        elif p > 0:   verdict = "❌ destroyed"
        else:         verdict = "— skip"
        if p > 30: survived += 1

        print(f"{name:<20} {p:>7.1f}dB {sz:>6}KB {verdict:>12}")

    print(f"\n{survived}/{total} formats preserve geometry (PSNR > 20dB)")

if __name__ == '__main__':
    main()
