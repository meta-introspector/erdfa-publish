use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::io::Write;

pub mod render;
pub mod cft;
pub mod privacy;
pub mod mixer;
pub mod ingest;
pub mod stego;
pub mod distribute;

// Re-export for WASM consumers
pub use stego::{StegoPlugin, StegoChain};
pub use distribute::{DistributionPlan, DistributionTarget, Platform, AclTier, Ecc, IpfsManifest};

/// DA51 CBOR tag (0xDA51 = 55889)
const DASL_TAG: u64 = 55889;

// ── Semantic components ─────────────────────────────────────────

/// A semantic UI component. Renderers choose presentation per a11y layer.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum Component {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    Code { language: String, source: String },
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
    Tree { label: String, children: Vec<Component> },
    List { ordered: bool, items: Vec<String> },
    Link { href: String, label: String },
    Image { alt: String, cid: String },
    KeyValue { pairs: Vec<(String, String)> },
    MapEntity { name: String, kind: String, x: f64, y: f64, meta: Vec<(String, String)> },
    Group { role: String, children: Vec<Component> },
}

// ── Shard ───────────────────────────────────────────────────────

/// One CBOR shard: a semantic component with identity.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Shard {
    pub id: String,
    pub cid: String,
    pub component: Component,
    #[serde(default)]
    pub tags: Vec<String>,
}

impl Shard {
    pub fn new(id: impl Into<String>, component: Component) -> Self {
        let id = id.into();
        let json = serde_json::to_vec(&component).unwrap_or_default();
        let cid = format!("bafk{}", &hex::encode(Sha256::digest(&json))[..32]);
        Self { id, cid, component, tags: Vec::new() }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Encode as DA51-tagged CBOR bytes.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let val = ciborium::Value::serialized(self).unwrap();
        let tagged = ciborium::Value::Tag(DASL_TAG, Box::new(val));
        ciborium::into_writer(&tagged, &mut buf).unwrap();
        buf
    }

    pub fn ipfs_url(&self) -> String { format!("https://ipfs.io/ipfs/{}", self.cid) }
    pub fn paste_url(&self, base: &str) -> String { format!("{}/raw/{}", base, self.id) }
}

// ── ShardSet (manifest) ─────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShardSet {
    pub name: String,
    pub shards: Vec<ShardRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShardRef {
    pub id: String,
    pub cid: String,
    pub tags: Vec<String>,
}

impl ShardSet {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), shards: Vec::new() }
    }

    pub fn add(&mut self, shard: &Shard) {
        self.shards.push(ShardRef {
            id: shard.id.clone(),
            cid: shard.cid.clone(),
            tags: shard.tags.clone(),
        });
    }

    /// Manifest as DA51-tagged CBOR.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let val = ciborium::Value::serialized(self).unwrap();
        let tagged = ciborium::Value::Tag(DASL_TAG, Box::new(val));
        ciborium::into_writer(&tagged, &mut buf).unwrap();
        buf
    }

    /// Write all shards + manifest as a tar archive.
    pub fn to_tar<W: Write>(&self, shards: &[Shard], mut w: W) -> std::io::Result<()> {
        for shard in shards {
            let data = shard.to_cbor();
            tar_entry(&mut w, &format!("{}.cbor", shard.id), &data)?;
        }
        let manifest = self.to_cbor();
        tar_entry(&mut w, "manifest.cbor", &manifest)?;
        // Two 512-byte zero blocks = tar EOF
        w.write_all(&[0u8; 1024])?;
        Ok(())
    }
}

fn tar_entry<W: Write>(w: &mut W, name: &str, data: &[u8]) -> std::io::Result<()> {
    let mut header = [0u8; 512];
    let n = name.as_bytes();
    header[..n.len().min(100)].copy_from_slice(&n[..n.len().min(100)]);
    // mode
    header[100..107].copy_from_slice(b"0000644");
    // size in octal
    let size_str = format!("{:011o}", data.len());
    header[124..135].copy_from_slice(size_str.as_bytes());
    // typeflag = regular file
    header[156] = b'0';
    // magic
    header[257..263].copy_from_slice(b"ustar\0");
    // checksum
    header[148..156].copy_from_slice(b"        ");
    let cksum: u32 = header.iter().map(|&b| b as u32).sum();
    let ck_str = format!("{:06o}\0 ", cksum);
    header[148..156].copy_from_slice(ck_str.as_bytes());
    w.write_all(&header)?;
    w.write_all(data)?;
    // Pad to 512-byte boundary
    let pad = (512 - data.len() % 512) % 512;
    if pad > 0 { w.write_all(&vec![0u8; pad])?; }
    Ok(())
}

// ── Triple-oriented API (wire-compatible with wasm/src/codec/cbor.rs) ──

impl ShardSet {
    pub fn from_shards(name: impl Into<String>, shards: &[Shard]) -> Self {
        let mut set = Self::new(name);
        for s in shards { set.add(s); }
        set
    }

    /// Pack all shard CBOR as NFT7 segments, split across N tiles.
    /// Returns tile chunks ready for `stego::lsb_embed`.
    pub fn to_nft7_tiles(&self, shards: &[Shard], n_tiles: usize) -> Vec<Vec<u8>> {
        let segments: Vec<(&str, Vec<u8>)> = shards.iter()
            .map(|s| (s.id.as_str(), s.to_cbor()))
            .collect();
        let seg_refs: Vec<(&str, &[u8])> = segments.iter()
            .map(|(id, data)| (*id, data.as_slice()))
            .collect();
        let payload = stego::nft7_encode(&seg_refs);
        stego::split_payload(&payload, n_tiles)
    }
}

impl Shard {
    /// Encode this shard's CBOR into a steganographic carrier.
    pub fn to_carrier(&self, ct: stego::CarrierType) -> Vec<u8> {
        stego::encode(&self.to_cbor(), ct)
    }

    /// Decode a shard from a steganographic carrier.
    pub fn from_carrier(carrier: &[u8], ct: stego::CarrierType) -> Option<Self> {
        let cbor = stego::decode(carrier, ct)?;
        // Strip DA51 tag and deserialize
        let val: ciborium::Value = ciborium::from_reader(&cbor[..]).ok()?;
        let inner = match val {
            ciborium::Value::Tag(55889, boxed) => *boxed,
            other => other,
        };
        ciborium::Value::deserialized(&inner).ok()
    }
}

/// Encode RDFa triples as minimal CBOR (array of [s,p,o] arrays).
/// Wire-compatible with the eRDFa WASM decoder.
pub fn encode_triples(triples: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    cbor_uint(4, triples.len() as u64, &mut out);
    for (s, p, o) in triples {
        cbor_uint(4, 3, &mut out);
        cbor_str(s, &mut out);
        cbor_str(p, &mut out);
        cbor_str(o, &mut out);
    }
    out
}

/// Content-addressed ID from bytes (SHA-256, baf-prefixed).
pub fn content_cid(data: &[u8]) -> String {
    format!("baf{}", &hex::encode(Sha256::digest(data))[..32])
}

fn cbor_uint(major: u8, val: u64, out: &mut Vec<u8>) {
    let mt = major << 5;
    if val < 24 { out.push(mt | val as u8); }
    else if val <= 0xFF { out.push(mt | 24); out.push(val as u8); }
    else if val <= 0xFFFF { out.push(mt | 25); out.extend(&(val as u16).to_be_bytes()); }
    else if val <= 0xFFFF_FFFF { out.push(mt | 26); out.extend(&(val as u32).to_be_bytes()); }
    else { out.push(mt | 27); out.extend(&val.to_be_bytes()); }
}

fn cbor_str(s: &str, out: &mut Vec<u8>) {
    cbor_uint(3, s.len() as u64, out);
    out.extend(s.as_bytes());
}

/// Quick shard from RDFa triples (wire-compatible CBOR, no DA51 tag).
pub fn triple_shard(_id: &str, triples: &[(&str, &str, &str)]) -> (String, Vec<u8>) {
    let cbor = encode_triples(triples);
    let cid = content_cid(&cbor);
    (cid, cbor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_cid_deterministic() {
        let s1 = Shard::new("a", Component::Paragraph { text: "hello".into() });
        let s2 = Shard::new("b", Component::Paragraph { text: "hello".into() });
        assert_eq!(s1.cid, s2.cid); // same content → same CID
    }

    #[test]
    fn shard_cbor_roundtrip() {
        let s = Shard::new("test", Component::Heading { level: 1, text: "Hi".into() });
        let cbor = s.to_cbor();
        assert!(cbor.len() > 10);
        // DA51 tag = 0xD9DA51 in CBOR
        assert_eq!(cbor[0], 0xD9); // tag marker (2-byte)
        assert_eq!(cbor[1], 0xDA);
        assert_eq!(cbor[2], 0x51);
    }

    #[test]
    fn manifest_tracks_shards() {
        let s1 = Shard::new("a", Component::Paragraph { text: "one".into() });
        let s2 = Shard::new("b", Component::Paragraph { text: "two".into() });
        let set = ShardSet::from_shards("test", &[s1, s2]);
        assert_eq!(set.shards.len(), 2);
        assert_eq!(set.name, "test");
    }

    #[test]
    fn tar_has_manifest() {
        let s = Shard::new("x", Component::Code { language: "rust".into(), source: "1+1".into() });
        let set = ShardSet::from_shards("t", &[s.clone()]);
        let mut buf = Vec::new();
        set.to_tar(&[s], &mut buf).unwrap();
        // Should have at least 2 tar entries + EOF
        assert!(buf.len() > 2048);
        // Check first entry name
        let name = String::from_utf8_lossy(&buf[..6]);
        assert_eq!(&name[..2], "x.");
    }

    #[test]
    fn triple_cbor_wire_compat() {
        let cbor = encode_triples(&[("s", "p", "o")]);
        assert_eq!(cbor[0], 0x81); // array(1)
        assert_eq!(cbor[1], 0x83); // array(3)
        assert_eq!(cbor[2], 0x61); // text(1)
        assert_eq!(cbor[3], b's');
    }

    #[test]
    fn triple_shard_cid() {
        let (cid, cbor) = triple_shard("test", &[("_:a", "rdf:type", "erdfa:Name")]);
        assert!(cid.starts_with("baf"));
        assert!(cbor.len() > 10);
    }
}
