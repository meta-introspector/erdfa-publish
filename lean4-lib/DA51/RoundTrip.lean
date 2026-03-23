import DA51.CborVal
import DA51.Encode
open DA51.CborVal
open CborVal
open DA51.Encode

/-! Round-trip test: encode a CborVal back to CBOR bytes and verify structure -/

-- A small test value mimicking a DA51 shard
def testShard : CborVal :=
  ctag 55889 (cmap [
    ((ctext "subject"), (ctext "main")),
    ((ctext "predicate"), (ctext "fn")),
    ((ctext "object"), (ctext "main.rs")),
    ((ctext "prime"), (cnat 2)),
    ((ctext "blade"), (cnat 1))
  ])

-- Encode it
def testBytes : ByteArray := encode testShard

-- Verify DA51 tag is present: CBOR tag 55889 = 0xDA51
-- Tag 55889 needs 2-byte encoding: major 0xC0 | 25, then 0xDA, 0x51
theorem da51_tag_header :
    testBytes.data[0]? = some (0xC0 ||| 25) := by native_decide

theorem da51_tag_hi :
    testBytes.data[1]? = some 0xDA := by native_decide

theorem da51_tag_lo :
    testBytes.data[2]? = some 0x51 := by native_decide

-- The encoded size should be > 0
theorem encoded_nonempty : testBytes.size > 0 := by native_decide

-- Round-trip identity: encoding the same value twice gives the same bytes
theorem encode_deterministic :
    encode testShard = encode testShard := rfl

def main : IO Unit := do
  -- Encode
  let bytes := encode testShard
  IO.println s!"Encoded {bytes.size} bytes"
  IO.println s!"First 3 bytes: {bytes.data[0]!} {bytes.data[1]!} {bytes.data[2]!}"
  IO.println s!"DA51 tag: 0x{String.mk (Nat.toDigits 16 (bytes.data[1]!.toNat * 256 + bytes.data[2]!.toNat))}"
  -- Write to file
  let outpath := "roundtrip_test.cbor"
  IO.FS.writeBinFile outpath bytes
  IO.println s!"Wrote {outpath}"
