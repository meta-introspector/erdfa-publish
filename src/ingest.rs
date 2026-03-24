//! Solana transaction ingestion, holder graph, Fibonacci tier NFT series.
//!
//! Crawls token CA + author + all interacting addresses for the full year.
//! Builds holder rankings, assigns Fibonacci tiers, generates layered
//! stego NFT tiles per tier. Claim requires wallet signature.

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Known solfunmeme addresses.
pub const TOKEN_CA: &str = "BwUTq7fS6sfUmHDwAiCQZ3asSiPEapW5zDrsbwtapump";
pub const AUTHOR: &str = "HMEKzpgzJEfyYyqoob5uGHR9P3LF6248zbm8tWgaApim";
pub const MAINNET_RPC: &str = "https://api.mainnet-beta.solana.com";

// ── Fibonacci tiers ─────────────────────────────────────────────

/// Generate Fibonacci tier boundaries starting from seed values.
/// Returns: [(tier_name, cumulative_count), ...]
pub fn fibonacci_tiers() -> Vec<(String, usize)> {
    let mut tiers = vec![
        ("diamond".into(), 100),
        ("gold".into(), 500),
        ("silver".into(), 1000),
    ];
    // Continue with Fibonacci from 1000
    let (mut a, mut b) = (500usize, 1000);
    let mut tier_num = 3;
    loop {
        let next = a + b;
        if next > 100_000 { break; }
        tiers.push((format!("fib-{}", tier_num), next));
        a = b;
        b = next;
        tier_num += 1;
    }
    tiers
}

// ── Transaction data ────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxRecord {
    pub signature: String,
    pub slot: u64,
    pub timestamp: i64,
    pub accounts: Vec<String>,
    pub memo: Option<String>,
    pub raw: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HolderInfo {
    pub address: String,
    pub tx_count: usize,
    pub first_seen: i64,
    pub last_seen: i64,
    pub tier: String,
    pub rank: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IngestState {
    pub rpc: String,
    pub seed_addresses: Vec<String>,
    pub transactions: Vec<TxRecord>,
    pub holders: Vec<HolderInfo>,
    pub tiers: Vec<(String, usize)>,
    pub crawled_addresses: Vec<String>,
}

impl IngestState {
    pub fn new(rpc: &str) -> Self {
        Self {
            rpc: rpc.into(),
            seed_addresses: vec![TOKEN_CA.into(), AUTHOR.into()],
            transactions: Vec::new(),
            holders: Vec::new(),
            tiers: fibonacci_tiers(),
            crawled_addresses: Vec::new(),
        }
    }

    pub fn save(&self, path: &Path) {
        let json = serde_json::to_string_pretty(self).expect("serialize state");
        std::fs::write(path, json).expect("write state");
    }

    pub fn load(path: &Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        serde_json::from_slice(&data).ok()
    }
}

// ── Solana RPC crawler ──────────────────────────────────────────

/// Fetch all transaction signatures for an address (up to limit).
pub fn fetch_signatures(rpc: &str, address: &str, limit: usize) -> Vec<String> {
    let output = Command::new("solana")
        .args(["--url", rpc, "transaction-history", address,
               "--limit", &limit.to_string()])
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty() && !l.contains("transactions found"))
            .map(|l| l.trim().to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Fetch transaction detail and extract accounts + memo.
pub fn fetch_tx_detail(rpc: &str, sig: &str) -> Option<TxRecord> {
    let output = Command::new("solana")
        .args(["--url", rpc, "confirm", "-v", sig])
        .output().ok()?;
    let raw = String::from_utf8_lossy(&output.stdout).to_string();

    // Extract slot
    let slot = raw.lines()
        .find(|l| l.contains("Slot:"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // Extract timestamp
    let timestamp = raw.lines()
        .find(|l| l.contains("Timestamp:"))
        .and_then(|l| l.split("Timestamp:").nth(1))
        .map(|s| s.trim().to_string())
        .and_then(|s| parse_timestamp(&s))
        .unwrap_or(0);

    // Extract all base58 addresses (32-44 chars)
    let accounts: Vec<String> = raw.lines()
        .flat_map(|l| l.split_whitespace())
        .filter(|w| w.len() >= 32 && w.len() <= 44 && is_base58(w))
        .map(|s| s.to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter().collect();

    // Extract memo
    let memo = raw.lines()
        .find(|l| l.contains("Memo") || l.contains("erdfa:"))
        .map(|l| l.trim().to_string());

    Some(TxRecord { signature: sig.into(), slot, timestamp, accounts, memo, raw })
}

fn is_base58(s: &str) -> bool {
    s.chars().all(|c| matches!(c,
        '1'..='9' | 'A'..='H' | 'J'..='N' | 'P'..='Z' | 'a'..='k' | 'm'..='z'))
}

fn parse_timestamp(s: &str) -> Option<i64> {
    // Try to parse "2026-03-24T..." or unix timestamp
    s.parse().ok()
}

/// Full crawl: fetch sigs for all seed addresses, then expand to interacting addresses.
pub fn crawl(state: &mut IngestState, depth: usize) {
    let mut to_crawl: Vec<String> = state.seed_addresses.clone();
    let mut seen_sigs: std::collections::HashSet<String> = 
        state.transactions.iter().map(|t| t.signature.clone()).collect();

    for d in 0..=depth {
        let mut next_addresses = Vec::new();
        eprintln!("◎ Crawl depth {} — {} addresses to scan", d, to_crawl.len());

        for addr in &to_crawl {
            if state.crawled_addresses.contains(addr) { continue; }
            eprintln!("  Fetching {}", addr);

            let sigs = fetch_signatures(&state.rpc, addr, 1000);
            eprintln!("    {} signatures", sigs.len());

            for sig in &sigs {
                if seen_sigs.contains(sig) { continue; }
                seen_sigs.insert(sig.clone());

                if let Some(tx) = fetch_tx_detail(&state.rpc, sig) {
                    // Collect new addresses for next depth
                    for acct in &tx.accounts {
                        if !state.crawled_addresses.contains(acct) && !to_crawl.contains(acct) {
                            next_addresses.push(acct.clone());
                        }
                    }
                    state.transactions.push(tx);
                }
            }
            state.crawled_addresses.push(addr.clone());
        }

        to_crawl = next_addresses;
        if to_crawl.is_empty() { break; }
        // Limit expansion at depth > 0
        if d > 0 { to_crawl.truncate(50); }
    }
    eprintln!("◎ Crawl complete: {} txns, {} addresses",
        state.transactions.len(), state.crawled_addresses.len());
}

// ── Holder ranking ──────────────────────────────────────────────

/// Build holder rankings from transaction data.
pub fn rank_holders(state: &mut IngestState) {
    let mut counts: HashMap<String, (usize, i64, i64)> = HashMap::new();

    for tx in &state.transactions {
        for addr in &tx.accounts {
            let entry = counts.entry(addr.clone()).or_insert((0, i64::MAX, 0));
            entry.0 += 1;
            entry.1 = entry.1.min(tx.timestamp);
            entry.2 = entry.2.max(tx.timestamp);
        }
    }

    // Sort by tx_count descending
    let mut ranked: Vec<_> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0));

    // Assign Fibonacci tiers
    let tiers = &state.tiers;
    state.holders = ranked.into_iter().enumerate().map(|(i, (addr, (count, first, last)))| {
        let tier = tiers.iter()
            .find(|(_, boundary)| i < *boundary)
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| "community".into());
        HolderInfo {
            address: addr,
            tx_count: count,
            first_seen: first,
            last_seen: last,
            tier,
            rank: i + 1,
        }
    }).collect();

    eprintln!("◎ Ranked {} holders across {} tiers",
        state.holders.len(), state.tiers.len());
}

// ── NFT series generation ───────────────────────────────────────

/// Claim metadata: holder must sign this to activate their NFT.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaimMetadata {
    pub tier: String,
    pub tile_index: usize,
    pub holder_address: String,
    pub challenge: String,  // random nonce to sign
    pub merkle_root: String,
}

/// Generate NFT tile series for each tier with layered stego encoding.
/// Higher tiers get more data layers encoded in their tiles.
pub fn generate_nft_series(state: &IngestState, out_dir: &Path) {
    use crate::stego::{StegoPlugin, BitPlane6};

    std::fs::create_dir_all(out_dir).expect("create output dir");
    let bp6 = BitPlane6;

    // Serialize all transaction data
    let all_data = serde_json::to_vec(&state.transactions).expect("serialize txns");
    eprintln!("◎ Total transaction data: {} bytes", all_data.len());

    // Group holders by tier
    let mut tier_holders: HashMap<String, Vec<&HolderInfo>> = HashMap::new();
    for h in &state.holders {
        tier_holders.entry(h.tier.clone()).or_default().push(h);
    }

    let tile_cap = 196_608usize; // BitPlane6 512×512 capacity

    for (tier_name, boundary) in &state.tiers {
        let tier_dir = out_dir.join(tier_name);
        std::fs::create_dir_all(&tier_dir).expect("create tier dir");

        // Layer encoding: higher tiers get more data
        // Diamond gets everything, gold gets 80%, silver 60%, etc.
        let data_fraction = match tier_name.as_str() {
            "diamond" => 1.0,
            "gold" => 0.8,
            "silver" => 0.6,
            _ => 0.4,
        };
        let data_len = ((all_data.len() as f64) * data_fraction) as usize;
        let tier_data = &all_data[..data_len.min(all_data.len())];

        // Split into tiles
        let n_tiles = (tier_data.len() + tile_cap - 1) / tile_cap;
        let n_tiles = n_tiles.max(1);

        eprintln!("  {} tier: {} holders, {} bytes → {} tiles",
            tier_name, tier_holders.get(tier_name).map(|v| v.len()).unwrap_or(0),
            tier_data.len(), n_tiles);

        for i in 0..n_tiles {
            let start = i * tile_cap;
            let end = (start + tile_cap).min(tier_data.len());
            let chunk = if start < tier_data.len() { &tier_data[start..end] } else { &[] };
            let tile = bp6.encode(chunk);
            let path = tier_dir.join(format!("tile-{:04}.png", i));
            std::fs::write(&path, &tile).expect("write tile");
        }

        // Write tier manifest with claim challenges
        let holders = tier_holders.get(tier_name).cloned().unwrap_or_default();
        let claims: Vec<ClaimMetadata> = holders.iter().enumerate().map(|(i, h)| {
            let mut nonce = [0u8; 16];
            let hash = Sha256::digest(format!("{}:{}:{}", tier_name, h.address, i).as_bytes());
            nonce.copy_from_slice(&hash[..16]);
            ClaimMetadata {
                tier: tier_name.clone(),
                tile_index: i % n_tiles,
                holder_address: h.address.clone(),
                challenge: hex::encode(nonce),
                merkle_root: hex::encode(&Sha256::digest(tier_data)[..]),
            }
        }).collect();

        let manifest = serde_json::json!({
            "tier": tier_name,
            "boundary": boundary,
            "tiles": n_tiles,
            "data_bytes": tier_data.len(),
            "holders": claims.len(),
            "claims": claims,
        });
        std::fs::write(
            tier_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest).unwrap()
        ).expect("write manifest");
    }

    // Write master manifest
    let master = serde_json::json!({
        "type": "solfunmeme-nft-series",
        "token_ca": TOKEN_CA,
        "author": AUTHOR,
        "total_transactions": state.transactions.len(),
        "total_holders": state.holders.len(),
        "tiers": state.tiers,
        "data_bytes": all_data.len(),
    });
    std::fs::write(
        out_dir.join("series-manifest.json"),
        serde_json::to_string_pretty(&master).unwrap()
    ).expect("write master manifest");

    eprintln!("◎ NFT series generated in {}", out_dir.display());
}

// ── Pastebin submission ─────────────────────────────────────────

/// A pastebin entry: someone submits transaction data for team review.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PastebinEntry {
    pub id: String,
    pub submitted_at: String,
    pub submitter: Option<String>,
    pub content: String,
    pub content_hash: String,
    pub status: PasteStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PasteStatus {
    Pending,
    Reviewed,
    Accepted,
    Rejected,
}

impl PastebinEntry {
    pub fn new(content: String, submitter: Option<String>) -> Self {
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        let id = hash[..16].to_string();
        Self {
            id,
            submitted_at: chrono_now(),
            submitter,
            content,
            content_hash: hash,
            status: PasteStatus::Pending,
        }
    }
}

fn chrono_now() -> String {
    // Use system time as ISO string
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap();
    format!("{}", d.as_secs())
}

/// Pastebin store backed by a directory.
pub struct PastebinStore {
    pub dir: PathBuf,
}

impl PastebinStore {
    pub fn new(dir: PathBuf) -> Self {
        std::fs::create_dir_all(&dir).expect("create pastebin dir");
        Self { dir }
    }

    pub fn submit(&self, content: String, submitter: Option<String>) -> PastebinEntry {
        let entry = PastebinEntry::new(content, submitter);
        let path = self.dir.join(format!("{}.json", entry.id));
        std::fs::write(&path, serde_json::to_string_pretty(&entry).unwrap())
            .expect("write paste");
        entry
    }

    pub fn list(&self) -> Vec<PastebinEntry> {
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                if e.path().extension().map(|x| x == "json").unwrap_or(false) {
                    if let Ok(data) = std::fs::read(e.path()) {
                        if let Ok(entry) = serde_json::from_slice::<PastebinEntry>(&data) {
                            entries.push(entry);
                        }
                    }
                }
            }
        }
        entries.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
        entries
    }

    pub fn get(&self, id: &str) -> Option<PastebinEntry> {
        let path = self.dir.join(format!("{}.json", id));
        let data = std::fs::read(path).ok()?;
        serde_json::from_slice(&data).ok()
    }

    pub fn update_status(&self, id: &str, status: PasteStatus) -> Option<PastebinEntry> {
        let path = self.dir.join(format!("{}.json", id));
        let data = std::fs::read(&path).ok()?;
        let mut entry: PastebinEntry = serde_json::from_slice(&data).ok()?;
        entry.status = status;
        std::fs::write(&path, serde_json::to_string_pretty(&entry).unwrap()).ok()?;
        Some(entry)
    }
}

// ── Wallet signature verification ───────────────────────────────

/// Verify a claim: holder signs the challenge with their wallet.
/// For Solana, this means verifying an ed25519 signature over the challenge bytes.
pub fn verify_claim(claim: &ClaimMetadata, signature_b58: &str) -> bool {
    // Use solana CLI to verify: `solana verify-offchain-signature`
    // For now, verify format and log — full ed25519 verify needs solana-sdk
    let output = Command::new("solana")
        .args(["verify-offchain-signature",
               "--signer", &claim.holder_address,
               "--message", &claim.challenge,
               "--signature", signature_b58])
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fibonacci_tiers() {
        let tiers = fibonacci_tiers();
        assert_eq!(tiers[0], ("diamond".into(), 100));
        assert_eq!(tiers[1], ("gold".into(), 500));
        assert_eq!(tiers[2], ("silver".into(), 1000));
        // Next should be 1500 (500+1000)
        assert_eq!(tiers[3].1, 1500);
        assert_eq!(tiers[4].1, 2500); // 1000+1500
        assert!(tiers.len() >= 6);
        // All boundaries increasing
        for w in tiers.windows(2) {
            assert!(w[1].1 > w[0].1);
        }
    }

    #[test]
    fn test_pastebin_roundtrip() {
        let dir = std::env::temp_dir().join("erdfa-paste-test");
        let _ = std::fs::remove_dir_all(&dir);
        let store = PastebinStore::new(dir.clone());

        let entry = store.submit("tx sig 123abc".into(), Some("tester".into()));
        assert_eq!(entry.content, "tx sig 123abc");

        let loaded = store.get(&entry.id).unwrap();
        assert_eq!(loaded.content_hash, entry.content_hash);

        let updated = store.update_status(&entry.id, PasteStatus::Accepted).unwrap();
        assert!(matches!(updated.status, PasteStatus::Accepted));

        let list = store.list();
        assert_eq!(list.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_holder_ranking() {
        let mut state = IngestState::new("http://localhost:8899");
        // Fake some transactions
        for i in 0..10 {
            state.transactions.push(TxRecord {
                signature: format!("sig{}", i),
                slot: i as u64,
                timestamp: 1000 + i as i64,
                accounts: vec!["whale".into(), format!("addr{}", i)],
                memo: None,
                raw: String::new(),
            });
        }
        rank_holders(&mut state);
        // "whale" appears in all 10 txns, should be rank 1
        assert_eq!(state.holders[0].address, "whale");
        assert_eq!(state.holders[0].tx_count, 10);
        assert_eq!(state.holders[0].tier, "diamond"); // rank 1 < 100
    }
}
