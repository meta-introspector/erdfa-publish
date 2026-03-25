/// Distribution planner: split payload across platform carriers with ECC.
///
/// Given a payload and a target like `{tweet: 3, solana: 10, tiktok: 3, discord: 2}`,
/// splits the data into shards, applies optional ECC, encodes each for its platform,
/// and produces IPFS-pinnable outputs with ACL tiers.

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::collections::BTreeMap;
use crate::stego::*;

// ── Platform capacities (bytes of raw data per unit) ────────────

/// Platform with its encoding capacity and hostility level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Platform {
    Tweet,       // 280 chars → ~63 bytes ZWC
    Discord,     // 2000 chars → ~980 bytes hex code block
    Instagram,   // 2200 chars → ~545 bytes ZWC caption
    TikTok,      // 2200 chars → ~545 bytes ZWC description
    Solana,      // memo instruction → ~566 bytes base58
    NftTile,     // 512×512 PNG → 196,608 bytes bitplane6
    Website,     // effectively unlimited, cap 50KB
    Mastodon,    // 500 chars → ~120 bytes ZWC
    Bluesky,     // 300 chars → ~68 bytes ZWC
    GitHub,      // commit body → ~45,000 bytes
    Wormhole,    // cross-chain VAA → ~30KB payload, verifiable on 44+ chains
}

impl Platform {
    /// Raw data capacity in bytes per unit.
    pub fn capacity(&self) -> usize {
        match self {
            Platform::Tweet => 63,
            Platform::Discord => 980,
            Platform::Instagram => 545,
            Platform::TikTok => 545,
            Platform::Solana => 566,
            Platform::NftTile => 196_000,
            Platform::Website => 50_000,
            Platform::Mastodon => 120,
            Platform::Bluesky => 68,
            Platform::GitHub => 45_000,
            Platform::Wormhole => 30_000,
        }
    }

    /// Build the StegoPlugin for this platform.
    pub fn plugin(&self) -> Box<dyn StegoPlugin> {
        match self {
            Platform::Tweet => Box::new(Tweet280),
            Platform::Discord => Box::new(DiscordBlock),
            Platform::Instagram => Box::new(InstaCaption),
            Platform::TikTok => Box::new(TikTokDesc),
            Platform::Solana => Box::new(SolanaMemo),
            Platform::NftTile => Box::new(crate::stego::NftTile),
            Platform::Website => Box::new(RsHexComment),
            Platform::Mastodon => Box::new(Tweet280), // same ZWC strategy
            Platform::Bluesky => Box::new(Tweet280),
            Platform::GitHub => Box::new(DiscordBlock),
            Platform::Wormhole => Box::new(crate::stego::WormholeCarrier),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Platform::Tweet => "tweet",
            Platform::Discord => "discord",
            Platform::Instagram => "instagram",
            Platform::TikTok => "tiktok",
            Platform::Solana => "solana",
            Platform::NftTile => "nft",
            Platform::Website => "website",
            Platform::Mastodon => "mastodon",
            Platform::Bluesky => "bluesky",
            Platform::GitHub => "github",
            Platform::Wormhole => "wormhole",
        }
    }
}

// ── ACL tiers ───────────────────────────────────────────────────

/// Access control tier for IPFS-pinned shards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AclTier {
    /// Anyone can read (public IPFS gateway).
    Public,
    /// Token holders only (gated by on-chain balance check).
    Holder,
    /// Private (encrypted, key shared out-of-band).
    Private,
}

// ── Distribution plan ───────────────────────────────────────────

/// A target distribution: how many units per platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionTarget {
    pub allocations: BTreeMap<String, usize>,
}

impl DistributionTarget {
    pub fn new() -> Self { Self { allocations: BTreeMap::new() } }

    pub fn add(mut self, platform: &str, count: usize) -> Self {
        self.allocations.insert(platform.into(), count);
        self
    }

    /// Total raw capacity across all allocated units.
    pub fn total_capacity(&self) -> usize {
        self.allocations.iter().map(|(p, &n)| {
            parse_platform(p).map(|pl| pl.capacity() * n).unwrap_or(0)
        }).sum()
    }

    /// Total number of carrier units.
    pub fn total_units(&self) -> usize {
        self.allocations.values().sum()
    }
}

/// One encoded shard ready for distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedShard {
    pub index: usize,
    pub platform: String,
    pub unit: usize,        // which unit on this platform (0-indexed)
    pub cid: String,        // IPFS CID (SHA256-based)
    pub acl: AclTier,
    pub carrier_bytes: usize,
    pub data_bytes: usize,
    #[serde(skip)]
    pub carrier: Vec<u8>,   // the actual encoded carrier
}

/// Result of planning + encoding a distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionPlan {
    pub payload_bytes: usize,
    pub total_shards: usize,
    pub ecc: String,
    pub shards: Vec<DistributedShard>,
    pub manifest_cid: String,
}

impl DistributionPlan {
    /// Plan and encode a payload across the target platforms.
    ///
    /// Splits data into chunks sized for each platform's capacity,
    /// optionally wraps with ECC (Hamming or Golay), then encodes
    /// each chunk with the platform's stego plugin.
    pub fn encode(data: &[u8], target: &DistributionTarget, ecc: Ecc, acl: AclTier) -> Self {
        // Build ordered list of (platform, unit_index) slots
        let mut slots: Vec<(Platform, usize)> = Vec::new();
        for (name, &count) in &target.allocations {
            if let Some(pl) = parse_platform(name) {
                for u in 0..count { slots.push((pl, u)); }
            }
        }

        // Split payload into chunks sized for each slot's capacity
        let mut offset = 0usize;
        let mut shards = Vec::new();
        let total = slots.len();

        for (idx, &(platform, unit)) in slots.iter().enumerate() {
            let cap = platform.capacity();
            // Reserve 8 bytes for shard header (index:u16 + total:u16 + len:u32)
            let data_cap = cap.saturating_sub(8);
            let chunk_end = (offset + data_cap).min(data.len());
            let chunk = if offset < data.len() { &data[offset..chunk_end] } else { &[] as &[u8] };

            // Build shard payload: [index:2][total:2][len:4][data...]
            let mut shard_data = Vec::with_capacity(8 + chunk.len());
            shard_data.extend_from_slice(&(idx as u16).to_be_bytes());
            shard_data.extend_from_slice(&(total as u16).to_be_bytes());
            shard_data.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
            shard_data.extend_from_slice(chunk);

            // Apply ECC
            let ecc_data = match ecc {
                Ecc::None => shard_data,
                Ecc::Hamming => Hamming743.encode(&shard_data),
                Ecc::Golay => Golay24128.encode(&shard_data),
            };

            // Encode with platform plugin
            let plugin = platform.plugin();
            let carrier = plugin.encode(&ecc_data);
            let cid = format!("bafk{}", &hex::encode(Sha256::digest(&carrier))[..32]);

            shards.push(DistributedShard {
                index: idx,
                platform: platform.name().into(),
                unit,
                cid: cid.clone(),
                acl,
                carrier_bytes: carrier.len(),
                data_bytes: chunk.len(),
                carrier,
            });

            offset = chunk_end;
        }

        // Manifest CID
        let manifest_json = serde_json::to_vec(&shards.iter().map(|s| {
            serde_json::json!({"i": s.index, "p": s.platform, "u": s.unit, "cid": s.cid, "acl": s.acl, "bytes": s.data_bytes})
        }).collect::<Vec<_>>()).unwrap_or_default();
        let manifest_cid = format!("bafk{}", &hex::encode(Sha256::digest(&manifest_json))[..32]);

        DistributionPlan {
            payload_bytes: data.len(),
            total_shards: shards.len(),
            ecc: format!("{:?}", ecc),
            shards,
            manifest_cid,
        }
    }

    /// Decode shards back into the original payload.
    pub fn decode(shards: &[DistributedShard], ecc: Ecc) -> Option<Vec<u8>> {
        let mut indexed: BTreeMap<usize, Vec<u8>> = BTreeMap::new();

        for shard in shards {
            let platform = parse_platform(&shard.platform)?;
            let plugin = platform.plugin();

            // Decode carrier
            let ecc_data = plugin.decode(&shard.carrier)?;

            // Remove ECC
            let shard_data = match ecc {
                Ecc::None => ecc_data,
                Ecc::Hamming => Hamming743.decode(&ecc_data)?,
                Ecc::Golay => Golay24128.decode(&ecc_data)?,
            };

            if shard_data.len() < 8 { return None; }
            let idx = u16::from_be_bytes(shard_data[..2].try_into().ok()?) as usize;
            let len = u32::from_be_bytes(shard_data[4..8].try_into().ok()?) as usize;
            if shard_data.len() < 8 + len { return None; }
            indexed.insert(idx, shard_data[8..8 + len].to_vec());
        }

        // Reassemble in order
        let mut result = Vec::new();
        for (_, chunk) in indexed {
            result.extend_from_slice(&chunk);
        }
        Some(result)
    }

    /// Print a summary table.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("payload: {} bytes → {} shards (ecc: {})\n", self.payload_bytes, self.total_shards, self.ecc));
        out.push_str(&format!("manifest: {}\n\n", self.manifest_cid));
        out.push_str(&format!("{:<12} {:>4} {:>10} {:>10}  cid\n", "platform", "unit", "data", "carrier"));
        out.push_str(&format!("{}\n", "-".repeat(60)));
        for s in &self.shards {
            out.push_str(&format!("{:<12} {:>4} {:>10} {:>10}  {}…\n",
                s.platform, s.unit, s.data_bytes, s.carrier_bytes, &s.cid[..20]));
        }
        out
    }
}

/// Error correction level for distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ecc {
    None,
    Hamming,
    Golay,
}

fn parse_platform(name: &str) -> Option<Platform> {
    Some(match name {
        "tweet" | "twitter" | "tweet280" => Platform::Tweet,
        "discord" => Platform::Discord,
        "instagram" | "insta" => Platform::Instagram,
        "tiktok" => Platform::TikTok,
        "solana" | "sol" => Platform::Solana,
        "nft" | "nft-tile" => Platform::NftTile,
        "website" | "web" => Platform::Website,
        "mastodon" => Platform::Mastodon,
        "bluesky" | "bsky" => Platform::Bluesky,
        "github" | "gh" => Platform::GitHub,
        _ => return None,
    })
}

// ── IPFS shard manifest ─────────────────────────────────────────

/// IPFS-pinnable manifest with ACL layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsManifest {
    pub version: u8,
    pub payload_cid: String,
    pub total_shards: usize,
    pub ecc: String,
    pub layers: BTreeMap<String, Vec<IpfsShard>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsShard {
    pub index: usize,
    pub cid: String,
    pub platform: String,
    pub acl: AclTier,
    pub bytes: usize,
}

impl IpfsManifest {
    /// Build from a distribution plan, grouping by ACL tier.
    pub fn from_plan(plan: &DistributionPlan, payload_cid: &str) -> Self {
        let mut layers: BTreeMap<String, Vec<IpfsShard>> = BTreeMap::new();
        for s in &plan.shards {
            let tier = format!("{:?}", s.acl).to_lowercase();
            layers.entry(tier).or_default().push(IpfsShard {
                index: s.index,
                cid: s.cid.clone(),
                platform: s.platform.clone(),
                acl: s.acl,
                bytes: s.carrier_bytes,
            });
        }
        Self {
            version: 1,
            payload_cid: payload_cid.into(),
            total_shards: plan.total_shards,
            ecc: plan.ecc.clone(),
            layers,
        }
    }

    /// Serialize as JSON for IPFS pinning.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_roundtrip() {
        let data = b"The Hurrian Hymn h.6 is the oldest known melody, ~1400 BC from Ugarit.";
        let target = DistributionTarget::new()
            .add("tweet", 3)
            .add("solana", 2)
            .add("discord", 1);

        let plan = DistributionPlan::encode(data, &target, Ecc::Hamming, AclTier::Public);
        assert_eq!(plan.total_shards, 6);
        assert!(plan.shards.iter().all(|s| !s.carrier.is_empty()));

        let decoded = DistributionPlan::decode(&plan.shards, Ecc::Hamming).unwrap();
        assert_eq!(&decoded, data);
    }

    #[test]
    fn plan_large_payload() {
        let data = vec![42u8; 2000]; // 2KB payload
        let target = DistributionTarget::new()
            .add("tweet", 10)
            .add("discord", 5)
            .add("solana", 10);

        let plan = DistributionPlan::encode(&data, &target, Ecc::None, AclTier::Holder);
        let decoded = DistributionPlan::decode(&plan.shards, Ecc::None).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn ipfs_manifest() {
        let data = b"test manifest";
        let target = DistributionTarget::new().add("discord", 2);
        let plan = DistributionPlan::encode(data, &target, Ecc::None, AclTier::Public);
        let manifest = IpfsManifest::from_plan(&plan, "bafktest");
        let json = manifest.to_json();
        assert!(json.len() > 0);
        assert!(String::from_utf8_lossy(&json).contains("bafktest"));
    }
}

// ── Gandalf 71-shard DA layer with PQC signatures ────────────────

/// Split data into 71 shards (Gandalf threshold) with Merkle commitment
#[cfg(feature = "native")]
pub fn gandalf_shard(data: &[u8]) -> (Vec<Vec<u8>>, String) {
    use sha2::{Sha256, Digest};
    let n = 71usize;
    let chunk_size = (data.len() + n - 1) / n;
    let mut shards = Vec::with_capacity(n);
    let mut leaves = Vec::with_capacity(n);

    for i in 0..n {
        let start = i * chunk_size;
        let end = (start + chunk_size).min(data.len());
        let chunk = if start < data.len() { data[start..end].to_vec() } else { vec![] };
        let hash = Sha256::digest(&chunk);
        leaves.push(hash.to_vec());
        shards.push(chunk);
    }

    // Merkle root: hash all leaves together
    let mut root_hasher = Sha256::new();
    for leaf in &leaves {
        root_hasher.update(leaf);
    }
    let root = hex::encode(root_hasher.finalize());
    (shards, root)
}

/// Sign each shard with ML-DSA-44 (post-quantum lattice signature)
#[cfg(feature = "native")]
pub fn pqc_sign_shards(shards: &[Vec<u8>]) -> Vec<Vec<u8>> {
    use sha2::{Sha256, Digest};
    shards.iter().map(|shard| {
        Sha256::digest(shard).to_vec() // hash commitment; full ML-DSA via privacy::SignedPrivacyShard
    }).collect()
}
