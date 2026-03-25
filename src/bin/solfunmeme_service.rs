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
    /// Identify holders: classify as wallet/program/token-account/closed via getAccountInfo
    Identify {
        /// Proofs directory (reads tier_snapshots.json, writes holder_identities.json)
        #[arg(long, default_value = "~/.solfunmeme/proofs")]
        proofs_dir: String,
        /// Requests per second
        #[arg(long, default_value = "8")]
        rate: usize,
    },
    /// Collect votes from feeds (file inbox, AMQP, HTTP) and verify + tally
    CollectVotes {
        /// Proofs directory (reads credentials.json, writes tally.json)
        #[arg(long, default_value = "~/.solfunmeme/proofs")]
        proofs_dir: String,
        /// Vote inbox directory (agents drop signed vote JSON files here)
        #[arg(long, default_value = "~/.solfunmeme/votes/inbox")]
        inbox: String,
        /// AMQP URL (optional, for RabbitMQ feed)
        #[arg(long, default_value = "amqp://guest:guest@127.0.0.1:5672")]
        amqp_url: String,
    },
    /// Run Lean4 proof verification + mint NFT credentials + generate vote schedule
    Prove {
        /// Proofs directory
        #[arg(long, default_value = "~/.solfunmeme/proofs")]
        proofs_dir: String,
        /// Path to solfunmeme-lean binary
        #[arg(long, default_value = "")]
        lean_bin: String,
    },
    /// Analyze cached tx → holder balances → Fibonacci tiers → completeness proof
    Analyze {
        /// HF dataset directory with getTransaction JSON files
        #[arg(long, default_value = "~/.solfunmeme/hf-dataset")]
        hf_dir: String,
        /// Time block size in seconds (default 1 day = 86400)
        #[arg(long, default_value = "86400")]
        block_secs: i64,
        /// Output directory for proof artifacts
        #[arg(long, default_value = "~/.solfunmeme/proofs")]
        out_dir: String,
    },
    /// Batch-crawl: fetch missing tx details from HF dataset sigs (daily systemd timer)
    BatchCrawl {
        /// HF dataset directory with getSignaturesForAddress JSON files
        #[arg(long, default_value = "~/.solfunmeme/hf-dataset")]
        hf_dir: String,
        /// Max transactions to fetch per run (daily budget)
        #[arg(long, default_value = "95000")]
        budget: usize,
        /// Requests per second (leave headroom below 10)
        #[arg(long, default_value = "8")]
        rate: usize,
    },
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

/// Extract {owner → amount} from preTokenBalances/postTokenBalances for a given mint
#[cfg(feature = "native")]
fn token_balances(arr: Option<&serde_json::Value>, mint: &str) -> std::collections::HashMap<String, i128> {
    let mut map = std::collections::HashMap::new();
    if let Some(serde_json::Value::Array(items)) = arr {
        for item in items {
            if item["mint"].as_str() == Some(mint) {
                if let (Some(owner), Some(amount)) = (
                    item["owner"].as_str(),
                    item["uiTokenAmount"]["amount"].as_str(),
                ) {
                    if let Ok(amt) = amount.parse::<i128>() {
                        map.insert(owner.to_string(), amt);
                    }
                }
            }
        }
    }
    map
}

#[cfg(feature = "native")]
fn chrono_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    format!("{}", secs)
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

        Cmd::Identify { proofs_dir, rate } => {
            let dir = expand_dir(&proofs_dir);
            let snap_path = dir.join("tier_snapshots.json");
            if !snap_path.exists() {
                eprintln!("No tier_snapshots.json — run 'analyze' first");
                return;
            }

            // Load last snapshot's addresses
            let data = std::fs::read_to_string(&snap_path).unwrap();
            let snaps: Vec<serde_json::Value> = serde_json::from_str(&data).unwrap();
            let last = snaps.last().unwrap();
            let mut addrs: Vec<(String, String, String)> = Vec::new(); // (addr, tier, balance)
            for tier in last["tiers"].as_array().unwrap() {
                let tier_name = tier["tier"].as_str().unwrap_or("?");
                for m in tier["members"].as_array().unwrap_or(&vec![]) {
                    addrs.push((
                        m["address"].as_str().unwrap_or("").to_string(),
                        tier_name.to_string(),
                        m["balance"].as_str().unwrap_or("0").to_string(),
                    ));
                }
            }
            eprintln!("◎ identify: {} holders to classify", addrs.len());

            // Known program owners
            let known_programs: std::collections::HashMap<&str, &str> = [
                ("11111111111111111111111111111111", "system"),
                ("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", "spl-token"),
                ("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb", "token-2022"),
                ("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL", "ata"),
                ("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo", "meteora-dlmm"),
                ("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc", "orca-whirlpool"),
                ("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8", "raydium-amm"),
                ("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK", "raydium-clmm"),
                ("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", "pump-fun"),
                ("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4", "jupiter-v6"),
                ("BPFLoaderUpgradeab1e11111111111111111111111", "bpf-upgradeable"),
                ("BPFLoader2111111111111111111111111111111111", "bpf-loader"),
            ].into_iter().collect();

            let rpc = rpc_url(&args.rpc);
            let mut results: Vec<serde_json::Value> = Vec::new();
            let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            let start = std::time::Instant::now();

            for (i, (addr, tier, balance)) in addrs.iter().enumerate() {
                let kind;
                let mut owner_label = String::new();
                let mut sol_balance = 0.0f64;

                let resp = rpc_post(&rpc, "getAccountInfo",
                    &serde_json::json!([addr, {"encoding": "jsonParsed"}]));

                match resp.and_then(|v: serde_json::Value| v["result"]["value"].as_object().cloned()) {
                    None => {
                        kind = "closed";
                    }
                    Some(val) => {
                        let val = serde_json::Value::Object(val);
                        let exe = val["executable"].as_bool().unwrap_or(false);
                        let owner = val["owner"].as_str().unwrap_or("");
                        sol_balance = val["lamports"].as_f64().unwrap_or(0.0) / 1e9;

                        if exe {
                            kind = "program";
                            owner_label = known_programs.get(owner)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| format!("program:{}", &owner[..16.min(owner.len())]));
                        } else if owner == "11111111111111111111111111111111" {
                            kind = "wallet";
                        } else if let Some(label) = known_programs.get(owner) {
                            kind = "contract_account";
                            owner_label = label.to_string();
                        } else {
                            kind = "contract_account";
                            owner_label = format!("unknown:{}", &owner[..16.min(owner.len())]);
                        }
                    }
                }

                *counts.entry(kind.to_string()).or_insert(0) += 1;

                results.push(serde_json::json!({
                    "address": addr,
                    "tier": tier,
                    "token_balance": balance,
                    "kind": kind,
                    "owner_label": owner_label,
                    "sol_balance": sol_balance,
                }));

                if (i + 1) % rate == 0 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                if (i + 1) % 200 == 0 {
                    let elapsed = start.elapsed().as_secs_f64();
                    eprintln!("  [{}/{}] {:.1}/s {:?}", i + 1, addrs.len(), (i+1) as f64 / elapsed, counts);
                }
            }

            let elapsed = start.elapsed().as_secs_f64();

            let output = serde_json::json!({
                "generated_at": chrono_timestamp(),
                "total_holders": results.len(),
                "classification": counts,
                "holders": results,
            });

            let out_path = dir.join("holder_identities.json");
            std::fs::write(&out_path, serde_json::to_string_pretty(&output).unwrap()).unwrap();

            eprintln!("\n◎ Holder Classification Complete ({:.1}s)", elapsed);
            for (kind, count) in &counts {
                eprintln!("  {:20} {}", kind, count);
            }
            eprintln!("◎ Wrote {}", out_path.display());
        }

        Cmd::CollectVotes { proofs_dir, inbox, amqp_url } => {
            let dir = expand_dir(&proofs_dir);
            let inbox_dir = expand_dir(&inbox);
            std::fs::create_dir_all(&inbox_dir).ok();

            // 1. Load credentials
            let cred_path = dir.join("credentials.json");
            if !cred_path.exists() {
                eprintln!("No credentials.json — run 'prove' first");
                return;
            }
            let cred_data: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&cred_path).unwrap()).unwrap();
            let valid_holders: std::collections::HashSet<String> = cred_data["credential_list"]
                .as_array().unwrap_or(&vec![])
                .iter()
                .filter_map(|c| c["holder"].as_str().map(|s| s.to_string()))
                .collect();
            eprintln!("◎ collect-votes: {} credentialed holders", valid_holders.len());

            // 2. Collect from file inbox
            let mut votes: Vec<serde_json::Value> = Vec::new();
            let mut invalid = 0usize;

            if inbox_dir.exists() {
                for entry in std::fs::read_dir(&inbox_dir).unwrap().flatten() {
                    if !entry.file_name().to_string_lossy().ends_with(".json") { continue; }
                    let Ok(data) = std::fs::read_to_string(entry.path()) else { continue };
                    let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(&data) else { invalid += 1; continue };

                    // Validate: holder must have credential
                    let holder = v["holder"].as_str().unwrap_or("");
                    if !valid_holders.contains(holder) { invalid += 1; continue; }

                    // Validate: choice must be yea/nay/abstain
                    let choice = v["choice"].as_str().unwrap_or("");
                    if !["yea", "nay", "abstain"].contains(&choice) { invalid += 1; continue; }

                    votes.push(v);
                }
            }
            eprintln!("  File inbox: {} valid, {} invalid", votes.len(), invalid);

            // 3. Try AMQP (RabbitMQ) — poll via management HTTP API
            let amqp_votes = {
                // Try management API to get messages from solfunmeme-votes queue
                let mgmt_url = "http://127.0.0.1:15672/api/queues/%2f/solfunmeme-votes/get";
                let body = serde_json::json!({"count": 1000, "ackmode": "ack_requeue_false", "encoding": "auto"});
                match ureq::post(mgmt_url)
                    .set("Authorization", "Basic Z3Vlc3Q6Z3Vlc3Q=") // guest:guest
                    .set("Content-Type", "application/json")
                    .send_string(&body.to_string()) {
                    Ok(resp) => {
                        let text = resp.into_string().unwrap_or_default();
                        let msgs: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
                        let mut amqp_v = Vec::new();
                        for msg in &msgs {
                            let payload = msg["payload"].as_str().unwrap_or("{}");
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
                                let holder = v["holder"].as_str().unwrap_or("");
                                if valid_holders.contains(holder) {
                                    amqp_v.push(v);
                                }
                            }
                        }
                        eprintln!("  AMQP: {} votes from solfunmeme-votes queue", amqp_v.len());
                        amqp_v
                    }
                    Err(_) => {
                        eprintln!("  AMQP: not available (management API not responding)");
                        vec![]
                    }
                }
            };
            votes.extend(amqp_votes);

            // 4. Deduplicate by holder (last vote wins)
            let mut by_holder: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
            for v in &votes {
                if let Some(h) = v["holder"].as_str() {
                    by_holder.insert(h.to_string(), v.clone());
                }
            }
            eprintln!("  Total: {} unique votes (from {} raw)", by_holder.len(), votes.len());

            // 5. Tally per chamber
            let mut tally: std::collections::HashMap<String, (usize, usize, usize)> = std::collections::HashMap::new();
            for (holder, vote) in &by_holder {
                // Find chamber from credentials
                let empty_vec = vec![];
                let chamber = cred_data["credential_list"].as_array().unwrap_or(&empty_vec)
                    .iter()
                    .find(|c| c["holder"].as_str() == Some(holder))
                    .and_then(|c| c["chamber"].as_str())
                    .unwrap_or("unknown");
                let choice = vote["choice"].as_str().unwrap_or("abstain");
                let entry = tally.entry(chamber.to_string()).or_insert((0, 0, 0));
                match choice {
                    "yea" => entry.0 += 1,
                    "nay" => entry.1 += 1,
                    _ => entry.2 += 1,
                }
            }

            // 6. Resolve per chamber
            let mut results: Vec<serde_json::Value> = Vec::new();
            for (chamber, (yea, nay, abstain)) in &tally {
                let (size, majority) = match chamber.as_str() {
                    "senate" => (100, 51),
                    "house" => (500, 251),
                    "lobby" => (1000, 0), // advisory
                    _ => (0, 0),
                };
                let quorum_met = (yea + nay + abstain) * 2 > size;
                let passed = quorum_met && *yea >= majority && yea > nay;
                results.push(serde_json::json!({
                    "chamber": chamber,
                    "yea": yea, "nay": nay, "abstain": abstain,
                    "quorum_met": quorum_met,
                    "passed": passed,
                    "advisory": chamber == "lobby",
                }));
            }

            // Bill passes if senate AND house pass
            let senate_pass = results.iter().any(|r| r["chamber"] == "senate" && r["passed"] == true);
            let house_pass = results.iter().any(|r| r["chamber"] == "house" && r["passed"] == true);
            let bill_enacted = senate_pass && house_pass;

            let output = serde_json::json!({
                "generated_at": chrono_timestamp(),
                "total_votes": by_holder.len(),
                "chambers": results,
                "bill_enacted": bill_enacted,
                "senate_passed": senate_pass,
                "house_passed": house_pass,
            });

            let out_path = dir.join("tally.json");
            std::fs::write(&out_path, serde_json::to_string_pretty(&output).unwrap()).unwrap();

            eprintln!("\n◎ Vote Tally");
            for r in &results {
                eprintln!("  {:8} yea={} nay={} abstain={} quorum={} {}",
                    r["chamber"].as_str().unwrap_or("?"),
                    r["yea"], r["nay"], r["abstain"],
                    r["quorum_met"],
                    if r["advisory"] == true { "(advisory)" }
                    else if r["passed"] == true { "PASSED" }
                    else { "FAILED" });
            }
            eprintln!("  Bill: {}", if bill_enacted { "ENACTED" } else { "NOT ENACTED" });
            eprintln!("◎ Wrote {}", out_path.display());
        }

        Cmd::Prove { proofs_dir, lean_bin } => {
            let dir = expand_dir(&proofs_dir);
            std::fs::create_dir_all(&dir).ok();

            // 1. Find lean binary
            let lean = if lean_bin.is_empty() {
                // Search common locations
                let candidates = [
                    expand_dir("~/.solfunmeme/solfunmeme-lean"),
                    std::path::PathBuf::from("/mnt/data1/meta-introspector/submodules/solfunmeme-introspector/.lake/build/bin/solfunmeme-lean"),
                ];
                candidates.into_iter().find(|p| p.exists())
                    .unwrap_or_else(|| { eprintln!("solfunmeme-lean not found; run 'lake build' in solfunmeme-introspector"); std::process::exit(1); })
            } else {
                expand_dir(&lean_bin)
            };

            // 2. Run Lean4 proofs
            eprintln!("◎ prove: running Lean4 verification...");
            let output = std::process::Command::new(&lean).output();
            match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    eprint!("{}", stdout);
                    if !o.status.success() {
                        eprintln!("✗ Lean4 verification FAILED");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("✗ Failed to run {}: {}", lean.display(), e);
                    std::process::exit(1);
                }
            }

            // 3. Mint NFT credentials from holder_identities.json
            let id_path = dir.join("holder_identities.json");
            if !id_path.exists() {
                eprintln!("No holder_identities.json — run 'identify' first");
                return;
            }

            let data = std::fs::read_to_string(&id_path).unwrap();
            let ids: serde_json::Value = serde_json::from_str(&data).unwrap();

            let mut credentials: Vec<serde_json::Value> = Vec::new();
            let day = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() / 86400;

            for h in ids["holders"].as_array().unwrap_or(&vec![]) {
                if h["kind"].as_str() != Some("wallet") { continue; }
                let addr = h["address"].as_str().unwrap_or("");
                let tier = h["tier"].as_str().unwrap_or("community");
                let balance = h["token_balance"].as_str().unwrap_or("0");
                let chamber = match tier {
                    "diamond" => "senate",
                    "gold" => "house",
                    "silver" | "fib-3" | "fib-4" | "fib-5" => "lobby",
                    _ => continue, // community doesn't get credentials
                };
                let min_balance: u64 = match chamber {
                    "senate" => 1_000_000,
                    "house" => 100_000,
                    _ => 10_000,
                };
                let bal: u64 = balance.parse().unwrap_or(0);
                if bal < min_balance { continue; }

                credentials.push(serde_json::json!({
                    "holder": addr,
                    "chamber": chamber,
                    "tier": tier,
                    "balance": balance,
                    "snapshot_day": day,
                    "min_balance": min_balance,
                    "credential_hash": format!("{:x}", {
                        use std::collections::hash_map::DefaultHasher;
                        use std::hash::{Hash, Hasher};
                        let mut h = DefaultHasher::new();
                        addr.hash(&mut h); chamber.hash(&mut h); day.hash(&mut h);
                        h.finish()
                    }),
                }));
            }

            // 4. Generate vote schedule (next 7 days)
            let mut schedule: Vec<serde_json::Value> = Vec::new();
            for d in 0..7 {
                schedule.push(serde_json::json!({
                    "day": day + d,
                    "bill_deadline_slot": (day + d) * 216000 + 216000, // ~1 day in slots
                    "status": if d == 0 { "open" } else { "pending" },
                }));
            }

            let output = serde_json::json!({
                "generated_at": chrono_timestamp(),
                "lean4_verified": true,
                "snapshot_day": day,
                "credentials": {
                    "total": credentials.len(),
                    "senate": credentials.iter().filter(|c| c["chamber"] == "senate").count(),
                    "house": credentials.iter().filter(|c| c["chamber"] == "house").count(),
                    "lobby": credentials.iter().filter(|c| c["chamber"] == "lobby").count(),
                },
                "credential_list": credentials,
                "vote_schedule": schedule,
            });

            let out_path = dir.join("credentials.json");
            std::fs::write(&out_path, serde_json::to_string_pretty(&output).unwrap()).unwrap();

            eprintln!("\n◎ Proof + Credentials Complete");
            eprintln!("  Lean4 verified: ✓");
            eprintln!("  Credentials minted: {} (senate={}, house={}, lobby={})",
                credentials.len(),
                credentials.iter().filter(|c| c["chamber"] == "senate").count(),
                credentials.iter().filter(|c| c["chamber"] == "house").count(),
                credentials.iter().filter(|c| c["chamber"] == "lobby").count());
            eprintln!("  Vote schedule: 7 days");
            eprintln!("◎ Wrote {}", out_path.display());
        }

        Cmd::Analyze { hf_dir, block_secs, out_dir } => {
            let hf = expand_dir(&hf_dir);
            let out = expand_dir(&out_dir);
            std::fs::create_dir_all(&out).ok();

            // 1. Parse all tx files → extract token balance changes
            eprintln!("◎ analyze: scanning tx files in {}", hf.display());
            let mint = TOKEN_CA;

            // BalanceChange: (slot, blockTime, owner, delta)
            let mut changes: Vec<(u64, i64, String, i128)> = Vec::new();
            let mut tx_count = 0usize;
            let mut relevant = 0usize;
            let mut errors = 0usize;
            let mut all_sigs: Vec<String> = Vec::new();

            for entry in std::fs::read_dir(&hf).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("method_getTransaction_") { continue; }
                tx_count += 1;

                // Extract sig from filename
                if let Some(rest) = name.strip_prefix("method_getTransaction_signature_") {
                    if let Some(sig) = rest.rsplit_once('_').map(|(s,_)| s) {
                        all_sigs.push(sig.to_string());
                    }
                }

                let Ok(data) = std::fs::read(entry.path()) else { errors += 1; continue };
                let Ok(v): Result<serde_json::Value, _> = serde_json::from_slice(&data) else { errors += 1; continue };
                let Some(result) = v.get("result") else { errors += 1; continue };
                let slot = result["slot"].as_u64().unwrap_or(0);
                let block_time = result["blockTime"].as_i64().unwrap_or(0);
                let meta = &result["meta"];

                // Extract pre/post token balances for our mint
                let pre = token_balances(meta.get("preTokenBalances"), mint);
                let post = token_balances(meta.get("postTokenBalances"), mint);

                let all_owners: std::collections::HashSet<&str> =
                    pre.keys().chain(post.keys()).map(|s| s.as_str()).collect();

                let mut has_change = false;
                for owner in all_owners {
                    let pre_amt = pre.get(owner).copied().unwrap_or(0);
                    let post_amt = post.get(owner).copied().unwrap_or(0);
                    let delta = post_amt - pre_amt;
                    if delta != 0 {
                        changes.push((slot, block_time, owner.to_string(), delta));
                        has_change = true;
                    }
                }
                if has_change { relevant += 1; }

                if tx_count % 5000 == 0 {
                    eprintln!("  [{}/...] relevant={} changes={}", tx_count, relevant, changes.len());
                }
            }

            eprintln!("  {} tx files, {} relevant, {} balance changes, {} errors",
                tx_count, relevant, changes.len(), errors);

            // 2. Sort by slot (time-ordered)
            changes.sort_by_key(|c| (c.0, c.2.clone()));

            // 3. Compute running balances and tier snapshots per time block
            let mut balances: std::collections::HashMap<String, i128> = std::collections::HashMap::new();
            let mut total_inflow: i128 = 0;
            let mut total_outflow: i128 = 0;
            let mut actors: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut block_snapshots: Vec<serde_json::Value> = Vec::new();

            let tiers = fibonacci_tiers();
            let mut current_block_time: i64 = 0;

            for (slot, bt, owner, delta) in &changes {
                actors.insert(owner.clone());
                *balances.entry(owner.clone()).or_insert(0) += delta;
                if *delta > 0 { total_inflow += delta; }
                else { total_outflow += delta.abs(); }

                // Snapshot at block boundaries
                if current_block_time == 0 { current_block_time = *bt; }
                if *bt >= current_block_time + block_secs || *slot == changes.last().map(|c| c.0).unwrap_or(0) {
                    // Sort holders by balance descending
                    let mut ranked: Vec<(&String, &i128)> = balances.iter()
                        .filter(|(_, b)| **b > 0)
                        .collect();
                    ranked.sort_by(|a, b| b.1.cmp(a.1));

                    // Build tier assignments
                    let mut tier_groups: Vec<serde_json::Value> = Vec::new();
                    for (tier_name, boundary) in &tiers {
                        let start = if tier_groups.is_empty() { 0 } else {
                            tiers.iter()
                                .take_while(|(n,_)| n != tier_name)
                                .last()
                                .map(|(_,b)| *b)
                                .unwrap_or(0)
                        };
                        let members: Vec<serde_json::Value> = ranked.iter()
                            .skip(start).take(boundary - start)
                            .map(|(addr, bal)| serde_json::json!({
                                "address": addr, "balance": bal.to_string()
                            }))
                            .collect();
                        tier_groups.push(serde_json::json!({
                            "tier": tier_name,
                            "boundary": boundary,
                            "count": members.len(),
                            "members": members,
                        }));
                    }
                    // Community tier (everyone else)
                    let last_boundary = tiers.last().map(|(_,b)| *b).unwrap_or(0);
                    let community: Vec<serde_json::Value> = ranked.iter()
                        .skip(last_boundary)
                        .map(|(addr, bal)| serde_json::json!({
                            "address": addr, "balance": bal.to_string()
                        }))
                        .collect();
                    tier_groups.push(serde_json::json!({
                        "tier": "community",
                        "boundary": "∞",
                        "count": community.len(),
                        "members": community,
                    }));

                    block_snapshots.push(serde_json::json!({
                        "block_time": current_block_time,
                        "slot": slot,
                        "total_holders": ranked.len(),
                        "total_actors": actors.len(),
                        "tiers": tier_groups,
                    }));

                    current_block_time = *bt;
                }
            }

            // 4. Completeness proof
            // Collect all sigs from getSignaturesForAddress files
            let mut dataset_sigs: std::collections::HashSet<String> = std::collections::HashSet::new();
            for entry in std::fs::read_dir(&hf).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("method_getSignaturesForAddress_") { continue; }
                let Ok(data) = std::fs::read(entry.path()) else { continue };
                let Ok(v): Result<serde_json::Value, _> = serde_json::from_slice(&data) else { continue };
                if let Some(arr) = v["result"].as_array() {
                    for item in arr {
                        if let Some(sig) = item["signature"].as_str() {
                            dataset_sigs.insert(sig.to_string());
                        }
                    }
                }
            }

            let analyzed_sigs: std::collections::HashSet<String> = all_sigs.into_iter().collect();
            let missing: Vec<&String> = dataset_sigs.iter()
                .filter(|s| !analyzed_sigs.contains(s.as_str()))
                .take(100)
                .collect();

            let net_balance: i128 = balances.values().sum();

            let proof = serde_json::json!({
                "proof_type": "federal_model_completeness",
                "token_mint": mint,
                "generated_at": chrono_timestamp(),
                "coverage": {
                    "total_sigs_in_dataset": dataset_sigs.len(),
                    "tx_files_analyzed": tx_count,
                    "tx_with_token_changes": relevant,
                    "coverage_pct": if dataset_sigs.is_empty() { 0.0 }
                        else { (tx_count as f64 / dataset_sigs.len() as f64) * 100.0 },
                    "missing_sample": missing,
                },
                "conservation": {
                    "total_inflow": total_inflow.to_string(),
                    "total_outflow": total_outflow.to_string(),
                    "net_balance": net_balance.to_string(),
                    "inflow_equals_outflow": total_inflow == total_outflow,
                },
                "actors": {
                    "total_unique_actors": actors.len(),
                    "holders_with_positive_balance": balances.values().filter(|b| **b > 0).count(),
                    "holders_with_zero_balance": balances.values().filter(|b| **b == 0).count(),
                },
                "tiers": fibonacci_tiers().iter().map(|(n,b)| serde_json::json!({"name": n, "boundary": b})).collect::<Vec<_>>(),
                "block_snapshots": block_snapshots.len(),
            });

            // 5. Write outputs
            let proof_path = out.join("completeness_proof.json");
            std::fs::write(&proof_path, serde_json::to_string_pretty(&proof).unwrap()).unwrap();
            eprintln!("◎ Wrote {}", proof_path.display());

            let snapshots_path = out.join("tier_snapshots.json");
            std::fs::write(&snapshots_path, serde_json::to_string_pretty(&block_snapshots).unwrap()).unwrap();
            eprintln!("◎ Wrote {} ({} blocks)", snapshots_path.display(), block_snapshots.len());

            // Summary to stderr
            eprintln!("\n◎ Federal Model Analysis Complete");
            eprintln!("  TX analyzed:    {}/{} ({:.1}% coverage)",
                tx_count, dataset_sigs.len(),
                if dataset_sigs.is_empty() { 0.0 } else { (tx_count as f64 / dataset_sigs.len() as f64) * 100.0 });
            eprintln!("  Token changes:  {} txns, {} balance changes", relevant, changes.len());
            eprintln!("  Actors:         {} unique addresses", actors.len());
            eprintln!("  Holders:        {} with positive balance", balances.values().filter(|b| **b > 0).count());
            eprintln!("  Conservation:   inflow={} outflow={} net={}",
                total_inflow, total_outflow, net_balance);
            eprintln!("  Time blocks:    {} snapshots", block_snapshots.len());

            // Print top 10 holders
            let mut top: Vec<_> = balances.iter().filter(|(_,b)| **b > 0).collect();
            top.sort_by(|a,b| b.1.cmp(a.1));
            eprintln!("\n  Top 10 holders:");
            for (i, (addr, bal)) in top.iter().take(10).enumerate() {
                let tier = tiers.iter()
                    .find(|(_, boundary)| i < *boundary)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("community");
                eprintln!("    #{:<3} {} {:>20} [{}]", i+1, &addr[..16], bal, tier);
            }
        }

        Cmd::BatchCrawl { hf_dir, budget, rate } => {
            let hf = expand_dir(&hf_dir);
            if !hf.exists() {
                eprintln!("HF dataset dir not found: {}", hf.display());
                eprintln!("Clone it: git clone https://huggingface.co/datasets/introspector/solfunmeme {}", hf.display());
                return;
            }

            // 1. Collect existing tx sigs
            let mut have: std::collections::HashSet<String> = std::collections::HashSet::new();
            for entry in std::fs::read_dir(&hf).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("method_getTransaction_signature_") {
                    // Extract sig from filename: method_getTransaction_signature_{SIG}_{HASH}.json
                    if let Some(rest) = name.strip_prefix("method_getTransaction_signature_") {
                        if let Some(sig) = rest.rsplit_once('_').map(|(s,_)| s) {
                            have.insert(sig.to_string());
                        }
                    }
                }
            }
            eprintln!("◎ batch-crawl: {} existing tx details", have.len());

            // 2. Extract all sigs from getSignaturesForAddress files
            let mut all_sigs: Vec<String> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for entry in std::fs::read_dir(&hf).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("method_getSignaturesForAddress_") { continue; }
                let Ok(data) = std::fs::read(entry.path()) else { continue };
                let Ok(v): Result<serde_json::Value, _> = serde_json::from_slice(&data) else { continue };
                if let Some(arr) = v["result"].as_array() {
                    for item in arr {
                        if let Some(sig) = item["signature"].as_str() {
                            if seen.insert(sig.to_string()) {
                                all_sigs.push(sig.to_string());
                            }
                        }
                    }
                }
            }
            eprintln!("  {} unique sigs in dataset", all_sigs.len());

            // 3. Filter to missing
            let missing: Vec<&str> = all_sigs.iter()
                .filter(|s| !have.contains(s.as_str()))
                .map(|s| s.as_str())
                .take(budget)
                .collect();
            eprintln!("  {} missing, fetching {} (budget)", all_sigs.len() - have.len(), missing.len());

            if missing.is_empty() {
                eprintln!("◎ All caught up!");
                return;
            }

            // 4. Fetch at rate limit
            let mut fetched = 0usize;
            let mut errors = 0usize;
            let start = std::time::Instant::now();
            for (i, sig) in missing.iter().enumerate() {
                if cache_tx(&hf, sig, &args.rpc) {
                    fetched += 1;
                } else {
                    errors += 1;
                }
                if (i + 1) % rate == 0 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                if (i + 1) % 1000 == 0 {
                    let elapsed = start.elapsed().as_secs_f64();
                    let rps = (i + 1) as f64 / elapsed;
                    let eta = (missing.len() - i - 1) as f64 / rps / 60.0;
                    eprintln!("  [{}/{}] fetched={} errors={} {:.1}/s eta={:.1}min",
                        i + 1, missing.len(), fetched, errors, rps, eta);
                }
            }
            let elapsed = start.elapsed().as_secs_f64();
            eprintln!("◎ Done: {} fetched, {} errors in {:.1}min",
                fetched, errors, elapsed / 60.0);
            eprintln!("  Total tx details: {}", have.len() + fetched);
        }
    }
}

#[cfg(not(feature = "native"))]
fn main() { eprintln!("requires native feature"); }
