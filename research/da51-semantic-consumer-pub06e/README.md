# Experimental DA51 Semantic Witness Rust consumer

Status: research only.
Production status: NO_GO.

This is an isolated Rust prototype. It does not modify or
replace the existing `Shard` and `decode_shard` APIs.

## Reproduce

With Rust and the pinned Cargo dependencies available:

```bash
CARGO_NET_OFFLINE=true cargo test --offline --locked
```

Tested locally with Rust 1.99.1 and ciborium 0.2.2.

## Evidence

The tests cover:

- 11 baseline validation checks.
- 3 reproductions of a semantic authorization limitation.
- 4 remediation tests.

The original policy can accept modified theorem statements
or axiom metadata if the caller deliberately supplies the
modified object's digest as trusted.

The additional `authorize_pinned_claims` policy rejects the
tested modifications by comparing claims with a fixed,
SHA256-verified reference witness.

## Trust and security limitations

- The authorization policy is specific to one audited fixture.
- The fixture reference is compiled into the prototype.
- The trusted digest must come from independent configuration.
- The consumer does not execute Lean or verify arbitrary proofs.
- The prototype does not authenticate publishers.
- The implementation is not integrated with production paths.
- It does not implement complete independent evidence retrieval.
- Tested CBOR restrictions do not constitute formal proof of
  general CBOR canonicality.
- No full Rust/WASM deployment validation has been performed.
- The 3 gap-reproduction tests intentionally assert acceptance
  by the OLD policy; they are regression evidence, not a
  recommended security behavior.

This research package is independent of Draft PR #4.

Recommended follow-up: maintainer review of the trust model,
a separate opt-in integration API, and independent tests
against untrusted IPFS/Cloudflare transports.
