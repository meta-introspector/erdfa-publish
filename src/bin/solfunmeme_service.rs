/// solfunmeme-service: systemd-ready service for txn crawl, holder ranking,
/// NFT series generation, and pastebin/claim web endpoints.
///
/// Commands:
///   crawl  — Fetch full year of transactions for token + author + interactors
///   rank   — Build holder rankings and assign Fibonacci tiers
///   encode — Generate layered stego NFT tile series per tier
///   serve  — HTTP server: pastebin for txn submission + claim verification
///   status — Show current state

use std::path::PathBuf;
use std::io::{Read, Write, BufRead, BufReader};
use std::net::TcpListener;

#[cfg(feature = "native")]
use clap::{Parser, Subcommand};

#[cfg(feature = "native")]
#[derive(Parser)]
#[command(name = "solfunmeme-service")]
struct Args {
    /// Data directory for state, tiles, pastebin
    #[arg(long, default_value = "~/.solfunmeme")]
    data_dir: String,
    /// Solana RPC URL
    #[arg(long, default_value = "https://api.mainnet-beta.solana.com")]
    rpc: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[cfg(feature = "native")]
#[derive(Subcommand)]
enum Cmd {
    /// Crawl all transactions (full year)
    Crawl {
        /// Crawl depth (0=seeds only, 1=+interactors, 2=+their interactors)
        #[arg(long, default_value = "1")]
        depth: usize,
    },
    /// Build holder rankings from crawled data
    Rank,
    /// Generate NFT tile series per Fibonacci tier
    Encode,
    /// Run HTTP server for pastebin + claim
    Serve {
        #[arg(long, default_value = "0.0.0.0:7780")]
        bind: String,
    },
    /// Show current state
    Status,
}

#[cfg(feature = "native")]
fn expand_dir(s: &str) -> PathBuf {
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(&s[2..])
    } else {
        PathBuf::from(s)
    }
}

#[cfg(feature = "native")]
fn main() {
    use erdfa_publish::ingest::*;

    let args = Args::parse();
    let data_dir = expand_dir(&args.data_dir);
    std::fs::create_dir_all(&data_dir).expect("create data dir");

    let state_path = data_dir.join("state.json");
    let tiles_dir = data_dir.join("nft-series");
    let paste_dir = data_dir.join("pastebin");

    match args.cmd {
        Cmd::Crawl { depth } => {
            let mut state = IngestState::load(&state_path)
                .unwrap_or_else(|| IngestState::new(&args.rpc));
            state.rpc = args.rpc;

            eprintln!("◎ solfunmeme crawl (depth={})", depth);
            eprintln!("  Token CA: {}", TOKEN_CA);
            eprintln!("  Author:   {}", AUTHOR);
            eprintln!("  RPC:      {}", state.rpc);

            crawl(&mut state, depth);
            state.save(&state_path);
            eprintln!("◎ State saved: {} txns, {} addresses",
                state.transactions.len(), state.crawled_addresses.len());
        }

        Cmd::Rank => {
            let mut state = IngestState::load(&state_path)
                .expect("run 'crawl' first");
            rank_holders(&mut state);
            state.save(&state_path);

            // Print top 20
            eprintln!("◎ Top 20 holders:");
            for h in state.holders.iter().take(20) {
                eprintln!("  #{:>4} {} — {} txns [{}]",
                    h.rank, &h.address[..16], h.tx_count, h.tier);
            }
            eprintln!("  ... {} total holders", state.holders.len());
        }

        Cmd::Encode => {
            let state = IngestState::load(&state_path)
                .expect("run 'crawl' then 'rank' first");
            if state.holders.is_empty() {
                eprintln!("No holders ranked yet — run 'rank' first");
                return;
            }
            generate_nft_series(&state, &tiles_dir);
        }

        Cmd::Serve { bind } => {
            let pastebin = PastebinStore::new(paste_dir);
            let state = IngestState::load(&state_path);

            eprintln!("◎ solfunmeme-service listening on {}", bind);
            eprintln!("  POST /paste         — submit txn data for review");
            eprintln!("  GET  /paste         — list all submissions");
            eprintln!("  GET  /paste/<id>    — get submission");
            eprintln!("  POST /claim         — verify wallet signature");
            eprintln!("  GET  /status        — service status");
            eprintln!("  GET  /tiers         — Fibonacci tier info");

            let listener = TcpListener::bind(&bind).expect("bind");
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(&stream);
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() { continue; }

                let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
                if parts.len() < 2 { continue; }
                let (method, path) = (parts[0], parts[1]);

                // Read headers to get content-length
                let mut content_length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() { break; }
                    if line.trim().is_empty() { break; }
                    if line.to_lowercase().starts_with("content-length:") {
                        content_length = line.split(':').nth(1)
                            .and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                    }
                }

                // Read body
                let mut body = vec![0u8; content_length];
                if content_length > 0 { let _ = reader.read_exact(&mut body); }
                let body_str = String::from_utf8_lossy(&body).to_string();

                let (status, response) = match (method, path) {
                    ("GET", "/status") => {
                        let s = state.as_ref().map(|s| serde_json::json!({
                            "transactions": s.transactions.len(),
                            "holders": s.holders.len(),
                            "crawled_addresses": s.crawled_addresses.len(),
                            "tiers": s.tiers,
                            "token_ca": TOKEN_CA,
                            "author": AUTHOR,
                        })).unwrap_or(serde_json::json!({"status": "no data yet"}));
                        ("200 OK", serde_json::to_string_pretty(&s).unwrap())
                    }

                    ("GET", "/tiers") => {
                        let tiers = fibonacci_tiers();
                        ("200 OK", serde_json::to_string_pretty(&tiers).unwrap())
                    }

                    ("POST", "/paste") => {
                        let entry = pastebin.submit(body_str, None);
                        eprintln!("  ← paste {} ({} bytes)", entry.id, entry.content.len());
                        ("201 Created", serde_json::to_string_pretty(&entry).unwrap())
                    }

                    ("GET", "/paste") => {
                        let list = pastebin.list();
                        ("200 OK", serde_json::to_string_pretty(&list).unwrap())
                    }

                    ("GET", p) if p.starts_with("/paste/") => {
                        let id = &p[7..];
                        match pastebin.get(id) {
                            Some(e) => ("200 OK", serde_json::to_string_pretty(&e).unwrap()),
                            None => ("404 Not Found", r#"{"error":"not found"}"#.into()),
                        }
                    }

                    ("POST", "/claim") => {
                        // Body: {"address":"...", "challenge":"...", "signature":"..."}
                        let claim: Result<serde_json::Value, _> = serde_json::from_str(&body_str);
                        match claim {
                            Ok(v) => {
                                let addr = v["address"].as_str().unwrap_or("");
                                let challenge = v["challenge"].as_str().unwrap_or("");
                                let sig = v["signature"].as_str().unwrap_or("");
                                let meta = ClaimMetadata {
                                    tier: String::new(),
                                    tile_index: 0,
                                    holder_address: addr.into(),
                                    challenge: challenge.into(),
                                    merkle_root: String::new(),
                                };
                                let valid = verify_claim(&meta, sig);
                                let resp = serde_json::json!({
                                    "address": addr,
                                    "verified": valid,
                                });
                                if valid { ("200 OK", serde_json::to_string_pretty(&resp).unwrap()) }
                                else { ("403 Forbidden", serde_json::to_string_pretty(&resp).unwrap()) }
                            }
                            Err(_) => ("400 Bad Request", r#"{"error":"invalid json"}"#.into()),
                        }
                    }

                    _ => ("404 Not Found", r#"{"error":"not found"}"#.into()),
                };

                let http = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                    status, response.len(), response
                );
                let _ = (&stream).write_all(http.as_bytes());
            }
        }

        Cmd::Status => {
            match IngestState::load(&state_path) {
                Some(state) => {
                    eprintln!("◎ solfunmeme-service status");
                    eprintln!("  Transactions:  {}", state.transactions.len());
                    eprintln!("  Holders:       {}", state.holders.len());
                    eprintln!("  Addresses:     {}", state.crawled_addresses.len());
                    eprintln!("  Tiers:         {}", state.tiers.len());
                    if !state.holders.is_empty() {
                        eprintln!("  Top holder:    {} ({} txns)",
                            state.holders[0].address, state.holders[0].tx_count);
                    }
                }
                None => eprintln!("No state yet — run 'crawl' first"),
            }
        }
    }
}

#[cfg(not(feature = "native"))]
fn main() { eprintln!("requires native feature"); }
