/// ledger-to-nft: Read sidechain transaction history, encode into BitPlane6 NFT tiles.
///
/// Usage:
///   ledger-to-nft --rpc <URL> --address <PUBKEY> --out <DIR> [--limit N]
///
/// Fetches all transactions for an address, serializes them as CBOR,
/// encodes into 512×512 BitPlane6 PNG tiles (196KB each).

use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "native")]
use clap::Parser;

#[cfg(feature = "native")]
#[derive(Parser)]
#[command(name = "ledger-to-nft")]
struct Args {
    /// Solana RPC URL
    #[arg(long, default_value = "http://127.0.0.1:8899")]
    rpc: String,
    /// Address to fetch history for
    #[arg(long)]
    address: Option<String>,
    /// Output directory for NFT tiles
    #[arg(long, default_value = "nft-tiles")]
    out: PathBuf,
    /// Max transactions to fetch
    #[arg(long, default_value = "1000")]
    limit: usize,
}

/// Fetch transaction signatures for an address
fn get_signatures(rpc: &str, address: &str, limit: usize) -> Vec<String> {
    let output = Command::new("solana")
        .args(["--url", rpc, "transaction-history", address,
               "--limit", &limit.to_string()])
        .output()
        .expect("solana CLI");
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter(|l| !l.is_empty() && !l.contains("transactions found"))
        .map(|l| l.trim().to_string())
        .collect()
}

/// Fetch full transaction JSON
fn get_transaction(rpc: &str, sig: &str) -> Option<serde_json::Value> {
    let output = Command::new("solana")
        .args(["--url", rpc, "confirm", "-v", "--output", "json", sig])
        .output()
        .ok()?;
    serde_json::from_slice(&output.stdout).ok()
}

/// Get own address from solana CLI
fn get_address(rpc: &str) -> String {
    Command::new("solana")
        .args(["--url", rpc, "address"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Encode data into BitPlane6 512×512 PNG tiles
fn encode_tiles(data: &[u8]) -> Vec<Vec<u8>> {
    use erdfa_publish::stego::{StegoPlugin, BitPlane6};
    let bp6 = BitPlane6;
    let tile_cap = 196_608; // 512×512×6bits / 8
    let n_tiles = (data.len() + tile_cap - 1) / tile_cap;

    (0..n_tiles.max(1)).map(|i| {
        let start = i * tile_cap;
        let end = (start + tile_cap).min(data.len());
        let chunk = if start < data.len() { &data[start..end] } else { &[] };
        bp6.encode(chunk)
    }).collect()
}

#[cfg(feature = "native")]
fn main() {
    let args = Args::parse();
    std::fs::create_dir_all(&args.out).expect("create output dir");

    let address = args.address.unwrap_or_else(|| get_address(&args.rpc));
    eprintln!("◎ Fetching transactions for {} from {}", address, args.rpc);

    // Fetch signatures
    let sigs = get_signatures(&args.rpc, &address, args.limit);
    eprintln!("  Found {} transactions", sigs.len());

    if sigs.is_empty() {
        eprintln!("No transactions found");
        return;
    }

    // Fetch full transactions and collect as JSON array
    let mut txns = Vec::new();
    for (i, sig) in sigs.iter().enumerate() {
        eprint!("\r  Fetching {}/{}", i + 1, sigs.len());
        if let Some(tx) = get_transaction(&args.rpc, sig) {
            txns.push(serde_json::json!({
                "signature": sig,
                "transaction": tx,
            }));
        }
    }
    eprintln!();

    // Serialize as CBOR
    let ledger = serde_json::json!({
        "type": "erdfa-sidechain-ledger",
        "address": address,
        "rpc": args.rpc,
        "transaction_count": txns.len(),
        "transactions": txns,
    });
    let json_bytes = serde_json::to_vec(&ledger).expect("serialize");
    eprintln!("  Ledger: {} bytes ({} txns)", json_bytes.len(), txns.len());

    // Encode into BitPlane6 tiles
    let tiles = encode_tiles(&json_bytes);
    eprintln!("  Encoding into {} NFT tile(s)", tiles.len());

    for (i, tile) in tiles.iter().enumerate() {
        let path = args.out.join(format!("sidechain-tile-{:03}.png", i));
        std::fs::write(&path, tile).expect("write tile");
        eprintln!("  ✓ {} ({} bytes)", path.display(), tile.len());
    }

    // Write manifest
    let manifest = serde_json::json!({
        "type": "erdfa-nft-manifest",
        "source": "sidechain-ledger",
        "address": address,
        "tiles": tiles.len(),
        "total_bytes": json_bytes.len(),
        "transactions": txns.len(),
        "tile_capacity": 196_608,
    });
    let manifest_path = args.out.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap())
        .expect("write manifest");
    eprintln!("  ✓ {}", manifest_path.display());
    eprintln!("◎ Done: {} tiles in {}", tiles.len(), args.out.display());
}

#[cfg(not(feature = "native"))]
fn main() { eprintln!("requires native feature"); }
