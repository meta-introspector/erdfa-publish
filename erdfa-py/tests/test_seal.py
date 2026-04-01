#!/usr/bin/env python3
"""test_seal.py — Verify erdfa_py seal bindings (CRQ-ERLAN-006).
Run: nix develop -c bash -c 'maturin develop --release && python3 tests/test_seal.py'
"""
import json, sys, os
import erdfa_py

RESULTS = os.path.expanduser("~/git/solana.solfunmeme/erlan-f4rm/test-results")
os.makedirs(RESULTS, exist_ok=True)

def test_seal_round_trip():
    state = [float(i) for i in range(24)]
    dna = json.dumps([[17,91],[78,85],[19,51]]).encode()
    packed = erdfa_py.seal_pack(state, dna, None)
    rgb = erdfa_py.seal_encode(packed)
    recovered = erdfa_py.seal_decode(rgb)
    s, d, w = erdfa_py.seal_unpack(recovered)
    assert s == state and d == dna and w == b""

def test_seal_with_wasm():
    packed = erdfa_py.seal_pack([42.0]*24, b'[[2,3]]', b'\x00asm\x01\x00\x00\x00')
    rgb = erdfa_py.seal_encode(packed)
    s, d, w = erdfa_py.seal_unpack(erdfa_py.seal_decode(rgb))
    assert d == b'[[2,3]]' and w == b'\x00asm\x01\x00\x00\x00'

def test_capacity():
    big = b"X" * 100_000
    packed = erdfa_py.seal_pack([0.0]*24, big, None)
    rgb = erdfa_py.seal_encode(packed)
    _, d, _ = erdfa_py.seal_unpack(erdfa_py.seal_decode(rgb))
    assert d == big

tests = [test_seal_round_trip, test_seal_with_wasm, test_capacity]
results = {}
for t in tests:
    try: t(); results[t.__name__] = "PASS"; print(f"  ✅ {t.__name__}")
    except Exception as e: results[t.__name__] = f"FAIL: {e}"; print(f"  ❌ {t.__name__}: {e}")

passed = sum(1 for v in results.values() if v == "PASS")
print(f"\n  {passed}/{len(tests)} passed")
json.dump(results, open(f"{RESULTS}/seal_test_results.json", "w"), indent=2)
print(f"  Saved: {RESULTS}/seal_test_results.json")
sys.exit(0 if passed == len(tests) else 1)
