import DA51.CborVal
open DA51.CborVal
open CborVal

/-! Minimal CBOR encoder for CborVal → ByteArray round-trip -/

namespace DA51.Encode

-- CBOR major types
def majorUint   : UInt8 := 0x00
def majorNeg    : UInt8 := 0x20
def majorBytes  : UInt8 := 0x40
def majorText   : UInt8 := 0x60
def majorArray  : UInt8 := 0x80
def majorMap    : UInt8 := 0xa0
def majorTag    : UInt8 := 0xc0
def majorSimple : UInt8 := 0xe0

def encodeHead (major : UInt8) (n : Nat) : ByteArray :=
  if n < 24 then
    ⟨#[major ||| n.toUInt8]⟩
  else if n < 256 then
    ⟨#[major ||| 24, n.toUInt8]⟩
  else if n < 65536 then
    ⟨#[major ||| 25, (n / 256).toUInt8, (n % 256).toUInt8]⟩
  else if n < 0x100000000 then
    ⟨#[major ||| 26,
      (n / 0x1000000 % 256).toUInt8,
      (n / 0x10000 % 256).toUInt8,
      (n / 0x100 % 256).toUInt8,
      (n % 256).toUInt8]⟩
  else
    ⟨#[major ||| 27,
      (n / 0x100000000000000 % 256).toUInt8,
      (n / 0x1000000000000 % 256).toUInt8,
      (n / 0x10000000000 % 256).toUInt8,
      (n / 0x100000000 % 256).toUInt8,
      (n / 0x1000000 % 256).toUInt8,
      (n / 0x10000 % 256).toUInt8,
      (n / 0x100 % 256).toUInt8,
      (n % 256).toUInt8]⟩

partial def encode : CborVal → ByteArray
  | cnat n    => encodeHead majorUint n
  | cneg n    => encodeHead majorNeg (n - 1)  -- CBOR: -1-n stored as n
  | cbytes bs =>
    let arr := ByteArray.mk (bs.toArray.map Nat.toUInt8)
    encodeHead majorBytes bs.length ++ arr
  | ctext s   =>
    let utf8 := s.toUTF8
    encodeHead majorText utf8.size ++ utf8
  | carray xs =>
    let body := xs.foldl (fun acc x => acc ++ encode x) ByteArray.empty
    encodeHead majorArray xs.length ++ body
  | cmap kvs  =>
    let body := kvs.foldl (fun acc (k, v) => acc ++ encode k ++ encode v) ByteArray.empty
    encodeHead majorMap kvs.length ++ body
  | ctag t v  => encodeHead majorTag t ++ encode v
  | cbool b   => if b then ⟨#[0xf5]⟩ else ⟨#[0xf4]⟩
  | cnull     => ⟨#[0xf6]⟩
  | cfloat _  => ⟨#[0xf6]⟩  -- lossy: we truncated float→Nat on import

end DA51.Encode
