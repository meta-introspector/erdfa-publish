# ZOS / DA51 — Semantic Witness integration handoff

Status: EXPERIMENTAL
Production decision: NO_GO

## Verified experimental result

A single 1103-byte DA51-tagged CBOR object, with tag 55889,
was constructed with Python, reproduced byte-for-byte using
the original Lean DA51 encoder, and decoded and re-encoded
byte-for-byte in an isolated Rust ciborium harness.

Witness SHA-256:

df436530f2716cf5f9c92261374638352d4c5e0687e498d1c04b5bd088fb69ec

Four fixed semantic statements were proven in Lean.
Four exact type bindings were compiled.
An independent replay reconstructed their proofs and
reproduced the kernel trust-query output byte-for-byte.

The evidence replay checked five evidence hashes and
four claim records, rejecting seven canonical-CBOR
semantic mutations.

An isolated Rust authorization harness accepts this
exact pinned witness and rejects eight mutations.

## Required integration boundary

The existing Rust Shard decoder is NOT a general
Semantic Witness verifier.

Introduce a separate experimental witness consumer.
Do not silently reinterpret a Semantic Witness as Shard.

Recommended authorization sequence:

1. Bound input size and resource consumption.
2. Decode one complete DA51 CBOR item.
3. Enforce a canonical, restricted witness schema.
4. Verify object identity against an independently
   trusted expected digest or authenticated publisher.
5. Reconstruct or independently verify evidence hashes.
6. Check theorem identity, proposition and trust policy.
7. Accept only an explicitly authorized claim scope.
8. Fail closed on missing, ambiguous or stale evidence.

A CBOR tag, a successful deserialization, or the
presence of SHA-256 strings does not prove semantics.

## Known limitations

- Only one pinned witness was authorized in Rust.
- The Rust authorization harness does not independently
  reconstruct Lean proofs.
- The Python strict CBOR validator is experimental,
  not formally verified.
- Original Rust consumers remain unchanged.
- General Semantic Witness verification is not implemented.
- No authenticated publisher or cryptographic signature.
- No end-to-end IPFS retrieval confirmation for this witness.
- No proof of a general semantic/geometric embedding.
- No Leech, Monster or no-ghost theorem established here.
- Original DA51 Mathlib-dependent kernel audit remains open.
- No production readiness claim.

## Recommended next milestone

Implement a separate, opt-in Rust Semantic Witness
consumer in an isolated branch, with explicit fail-closed
tests and a documented compatibility contract.

Do not merge or deploy solely on the strength of this
handoff. Request maintainer review of authority boundaries
and protocol compatibility first.
