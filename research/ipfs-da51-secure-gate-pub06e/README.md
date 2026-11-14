# PUB-06E-15M — Experimental IPFS → DA51 security gate

Status: RESEARCH_ONLY
Production acceptance: NO_GO

## Contents

- Rust Semantic Witness consumer and pinned-claims checks
- CIDv0/CIDv1 individual block verification
- Linux descriptor-based flatfs reader
- Experimental DA51 authorization gate
- Adversarial symlink and concurrency regression tests
- Historical runtime evidence and independent validation

## Confirmed experimental results

- 40/40 Rust tests passed in PUB-06E-15L
- 38 baseline tests and 2 concurrency tests
- 300 shard-directory mutations, 3,000 read attempts
- 300 atomic block replacements, 3,000 read attempts
- No incorrect block bytes accepted in those tested scenarios

## Limitations

- Synthetic local flatfs, not a live IPFS node
- Linux-only openat/O_NOFOLLOW implementation
- 64 KiB experimental individual-block limit
- DAG-PB structure and UnixFS multiblock NOT validated
- Concurrent in-place inode modification NOT tested
- Hostile mount changes and hard-link threats NOT fully audited
- WASM and Cloudflare NOT tested
- No general Lean kernel theorem verification
- Semantic authorization limited to the pinned fixture/claims
- Not integrated into an upstream production path

The experimental gate accepts CIDv1/raw Semantic Witness
bytes only; the separate reader can verify individual
CIDv0/DAG-PB block hashes but does not reconstruct UnixFS.

## Review request

Review the boundaries between CID integrity, local filesystem
isolation and the separately trusted semantic policy.

No production deployment, merge or protected-branch mutation
is implied by this research handoff.
