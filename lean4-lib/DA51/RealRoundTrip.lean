import DA51.CborVal
import DA51.Encode
open DA51.CborVal
open CborVal
open DA51.Encode

-- Real shard data: 3 decls from clifford.rs
def realShard : CborVal :=
  ctag 55889 (cmap [
    ((ctext "file"), (ctext "clifford.rs")),
    ((ctext "decl_count"), (cnat 126)),
    ((ctext "decl_blade"), (ctext "0x7b5b")),
    ((ctext "decl_grade"), (cnat 11)),
    ((ctext "decls"), (carray [
      (cmap [
        ((ctext "subject"), (ctext "serde::{..}")),
        ((ctext "predicate"), (ctext "use")),
        ((ctext "object"), (ctext "clifford.rs")),
        ((ctext "prime"), (cnat 23)),
        ((ctext "blade"), (ctext "0x0100"))
      ]),
      (cmap [
        ((ctext "subject"), (ctext "LEECH_KISSING_NUMBER")),
        ((ctext "predicate"), (ctext "const")),
        ((ctext "object"), (ctext "clifford.rs")),
        ((ctext "prime"), (cnat 3)),
        ((ctext "blade"), (ctext "0x0002"))
      ]),
      (cmap [
        ((ctext "subject"), (ctext "DIM")),
        ((ctext "predicate"), (ctext "const")),
        ((ctext "object"), (ctext "clifford.rs")),
        ((ctext "prime"), (cnat 3)),
        ((ctext "blade"), (ctext "0x0002"))
      ])
    ]))
  ])

-- Prove it's DA51 tagged
theorem realShard_is_da51 : ∃ v, realShard = ctag 55889 v := ⟨_, rfl⟩

def main : IO Unit := do
  let bytes := encode realShard
  IO.println s!"Encoded real shard: {bytes.size} bytes"
  IO.FS.writeBinFile "real_roundtrip.cbor" bytes
  IO.println "Wrote real_roundtrip.cbor"
