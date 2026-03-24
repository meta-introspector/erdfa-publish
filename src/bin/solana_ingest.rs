/// solana-ingest: Fetch all transactions for an address, extract erdfa memos,
/// reassemble shards, decode via StegoPlugin pipeline.
///
/// Usage:
///   solana-ingest --rpc <URL> --address <PUBKEY> [--out <DIR>]
///
/// Scans transaction history for `erdfa:` prefixed memo instructions,
/// decodes each via SolanaMemo plugin, writes recovered shards as .cbor files.

use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "native")]
use clap::Parser;

#[cfg(feature = "native")]
#[derive(Parser)]
#[command(name = "solana-ingest")]
struct Args {
    /// Solana RPC URL
    #[arg(long, default_value = "http://127.0.0.1:8899")]
    rpc: String,
    /// Address to scan
    #[arg(long)]
    address: Option<String>,
    /// Output directory for recovered shards
    #[arg(long, default_value = "ingested")]
    out: PathBuf,
    /// Max transactions to scan
    #[arg(long, default_value = "1000")]
    limit: usize,
}

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

fn get_signatures(rpc: &str, address: &str, limit: usize) -> Vec<String> {
    let output = Command::new("solana")
        .args(["--url", rpc, "transaction-history", address,
               "--limit", &limit.to_string()])
        .output()
        .expect("solana CLI");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("transactions found"))
        .map(|l| l.trim().to_string())
        .collect()
}

/// Fetch transaction and extract memo data
fn extract_memo(rpc: &str, sig: &str) -> Option<String> {
    let output = Command::new("solana")
        .args(["--url", rpc, "confirm", "-v", sig])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // Look for erdfa: memo in the output
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Data: \"erdfa:") {
            // Extract the memo string between quotes
            let start = trimmed.find("\"erdfa:")? + 1;
            let end = trimmed.rfind('"')?;
            return Some(trimmed[start..end].to_string());
        }
        // Also check log messages
        if trimmed.contains("Memo (len") && trimmed.contains("erdfa:") {
            let start = trimmed.find("\"erdfa:")? + 1;
            let end = trimmed.rfind('"')?;
            return Some(trimmed[start..end].to_string());
        }
    }
    None
}

#[cfg(feature = "native")]
fn main() {
    use erdfa_publish::stego::{StegoPlugin, SolanaMemo};

    let args = Args::parse();
    std::fs::create_dir_all(&args.out).expect("create output dir");

    let address = args.address.unwrap_or_else(|| get_address(&args.rpc));
    eprintln!("◎ Scanning {} for erdfa memos", address);
    eprintln!("  RPC: {}", args.rpc);

    let sigs = get_signatures(&args.rpc, &address, args.limit);
    eprintln!("  Found {} transactions", sigs.len());

    let memo_plugin = SolanaMemo;
    let mut found = 0;

    for (i, sig) in sigs.iter().enumerate() {
        eprint!("\r  Scanning {}/{}", i + 1, sigs.len());

        if let Some(memo) = extract_memo(&args.rpc, sig) {
            // Decode via SolanaMemo plugin
            match memo_plugin.decode(memo.as_bytes()) {
                Some(shard_data) => {
                    let hash = {
                        use sha2::{Sha256, Digest};
                        let h = Sha256::digest(&shard_data);
                        hex::encode(&h[..8])
                    };
                    let path = args.out.join(format!("shard-{}-{}.cbor", found, hash));
                    std::fs::write(&path, &shard_data).expect("write shard");
                    found += 1;
                    eprintln!("\r  ✓ {} → {} ({} bytes)    ",
                        &sig[..16], path.display(), shard_data.len());
                }
                None => {
                    eprintln!("\r  ✗ {} memo decode failed    ", &sig[..16]);
                }
            }
        }
    }

    eprintln!();
    eprintln!("◎ Ingested {} shards from {} transactions into {}",
        found, sigs.len(), args.out.display());

    // Try to decode recovered shards
    if found > 0 {
        eprintln!();
        eprintln!("=== Recovered shards ===");
        for entry in std::fs::read_dir(&args.out).expect("read dir").flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "cbor").unwrap_or(false) {
                let data = std::fs::read(&path).unwrap();
                // Try to decode as DA51 CBOR shard
                match erdfa_publish::render::decode_shard(&data) {
                    Some(shard) => {
                        let text = erdfa_publish::render::render_text(&shard);
                        eprintln!("  {} ({} bytes):", path.file_name().unwrap().to_string_lossy(), data.len());
                        for line in text.lines().take(5) {
                            eprintln!("    {}", line);
                        }
                    }
                    None => {
                        eprintln!("  {} ({} bytes): raw data (not DA51 CBOR)",
                            path.file_name().unwrap().to_string_lossy(), data.len());
                    }
                }
            }
        }
    }
}

#[cfg(not(feature = "native"))]
fn main() { eprintln!("requires native feature"); }
