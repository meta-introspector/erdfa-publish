/// solfunmeme-cli: command-line interface for the SOLFUNMEME DAO
///
/// Sign up on the web → get API key → use CLI to vote, submit data, check status.
///
/// Usage:
///   solfunmeme-cli login              — authenticate with API key from web signup
///   solfunmeme-cli status             — show DAO status + your tier
///   solfunmeme-cli vote yea|nay       — vote on today's bill
///   solfunmeme-cli submit <file>      — submit tx data for bounty
///   solfunmeme-cli tiers              — show Fibonacci tier boundaries
///   solfunmeme-cli prove              — run Lean4 proof verification
///   solfunmeme-cli shards <file>      — split file into 71 Gandalf shards

#[cfg(feature = "native")]
use clap::{Parser, Subcommand};

#[cfg(feature = "native")]
#[derive(Parser)]
#[command(name = "solfunmeme-cli", about = "SOLFUNMEME DAO command-line interface")]
struct Cli {
    /// API endpoint
    #[arg(long, default_value = "https://solana.solfunmeme.com/solfunmeme")]
    endpoint: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[cfg(feature = "native")]
#[derive(Subcommand)]
enum Cmd {
    /// Authenticate with API key from web signup
    Login {
        /// API key (from https://solana.solfunmeme.com/dioxus/accounts)
        #[arg(long)]
        key: Option<String>,
    },
    /// Show DAO status and your tier
    Status,
    /// Vote on today's bill
    Vote {
        /// yea, nay, or abstain
        choice: String,
    },
    /// Submit transaction data for bounty
    Submit {
        /// JSON file with getTransaction result
        file: String,
    },
    /// Show Fibonacci tier boundaries
    Tiers,
    /// Run Lean4 proof verification
    Prove,
    /// Split file into 71 Gandalf shards with PQC signatures
    Shards {
        /// File to shard
        file: String,
        /// Output directory
        #[arg(long, default_value = "./shards")]
        out: String,
    },
    /// Generate erdfa URL for an action (opens in browser or prints)
    Url {
        /// Action: vote, submit, status, tiers, dao, paste, p2p
        action: String,
        /// Extra key=value params
        params: Vec<String>,
    },
    /// Open the dioxus app at a specific route
    Open {
        /// Route: /, /dao, /paste, /p2p, /plugins
        #[arg(default_value = "/")]
        route: String,
    },
}

#[cfg(feature = "native")]
fn config_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(home).join(".solfunmeme").join("cli.json")
}

#[cfg(feature = "native")]
fn load_key() -> Option<String> {
    let data = std::fs::read_to_string(config_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    v["api_key"].as_str().map(|s| s.to_string())
}

#[cfg(feature = "native")]
fn save_key(key: &str) {
    let dir = config_path().parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&dir).ok();
    let v = serde_json::json!({"api_key": key, "endpoint": "https://solana.solfunmeme.com/solfunmeme"});
    std::fs::write(config_path(), serde_json::to_string_pretty(&v).unwrap()).ok();
}

#[cfg(feature = "native")]
fn api_get(endpoint: &str, path: &str) -> Option<serde_json::Value> {
    let url = format!("{}{}", endpoint, path);
    let resp = ureq::get(&url).call().ok()?;
    serde_json::from_reader(resp.into_reader()).ok()
}

#[cfg(feature = "native")]
fn api_post(endpoint: &str, path: &str, body: &str) -> Option<serde_json::Value> {
    let url = format!("{}{}", endpoint, path);
    let resp = ureq::post(&url)
        .set("Content-Type", "text/plain")
        .send_string(body).ok()?;
    serde_json::from_reader(resp.into_reader()).ok()
}

#[cfg(feature = "native")]
fn main() {
    use erdfa_publish::ingest::*;
    use erdfa_publish::distribute::gandalf_shard;

    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Login { key } => {
            let key = key.unwrap_or_else(|| {
                eprint!("API key (from web signup): ");
                let mut buf = String::new();
                std::io::stdin().read_line(&mut buf).unwrap();
                buf.trim().to_string()
            });
            save_key(&key);
            eprintln!("✓ Saved to {}", config_path().display());
            eprintln!("  Test: solfunmeme-cli status");
        }

        Cmd::Status => {
            match api_get(&cli.endpoint, "/status") {
                Some(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
                None => eprintln!("✗ Could not reach {}/status", cli.endpoint),
            }
        }

        Cmd::Vote { choice } => {
            if !["yea", "nay", "abstain"].contains(&choice.as_str()) {
                eprintln!("✗ Choice must be: yea, nay, or abstain");
                return;
            }
            let key = load_key().unwrap_or_default();
            let vote = serde_json::json!({
                "holder": key,
                "choice": choice,
                "channel": "cli",
            });
            match api_post(&cli.endpoint, "/paste", &vote.to_string()) {
                Some(v) => {
                    eprintln!("✓ Vote submitted: {}", choice);
                    eprintln!("  ID: {}", v["id"].as_str().unwrap_or("?"));
                }
                None => eprintln!("✗ Failed to submit vote"),
            }
        }

        Cmd::Submit { file } => {
            let data = std::fs::read_to_string(&file).unwrap_or_else(|e| {
                eprintln!("✗ Cannot read {}: {}", file, e);
                std::process::exit(1);
            });
            match api_post(&cli.endpoint, "/paste", &data) {
                Some(v) => {
                    eprintln!("✓ Submitted {} ({} bytes)", file, data.len());
                    eprintln!("  ID: {}", v["id"].as_str().unwrap_or("?"));
                    eprintln!("  Hash: {}", v["content_hash"].as_str().unwrap_or("?"));
                }
                None => eprintln!("✗ Failed to submit"),
            }
        }

        Cmd::Tiers => {
            for (name, boundary) in fibonacci_tiers() {
                println!("  {:12} {}", name, boundary);
            }
        }

        Cmd::Prove => {
            let lean = std::path::Path::new("/mnt/data1/meta-introspector/submodules/solfunmeme-introspector/.lake/build/bin/solfunmeme-lean");
            if lean.exists() {
                let out = std::process::Command::new(lean).output();
                match out {
                    Ok(o) => print!("{}", String::from_utf8_lossy(&o.stdout)),
                    Err(e) => eprintln!("✗ {}", e),
                }
            } else {
                eprintln!("✗ solfunmeme-lean not found. Run: cd solfunmeme-introspector && lake build");
            }
        }

        Cmd::Shards { file, out } => {
            let data = std::fs::read(&file).unwrap_or_else(|e| {
                eprintln!("✗ Cannot read {}: {}", file, e);
                std::process::exit(1);
            });
            let (shards, root) = gandalf_shard(&data);
            std::fs::create_dir_all(&out).ok();
            for (i, shard) in shards.iter().enumerate() {
                let path = format!("{}/shard_{:03}.bin", out, i);
                std::fs::write(&path, shard).unwrap();
            }
            eprintln!("✓ {} → {} shards in {}/", file, shards.len(), out);
            eprintln!("  Merkle root: {}", root);
        }

        Cmd::Url { action, params } => {
            let base = &cli.endpoint;
            let kv: Vec<String> = params.iter()
                .map(|p| p.replace('=', "="))
                .collect();
            let query = if kv.is_empty() { String::new() } else { format!("?{}", kv.join("&")) };

            let url = match action.as_str() {
                "vote" => format!("{}/paste{}", base, query),
                "submit" => format!("{}/paste{}", base, query),
                "status" => format!("{}/status", base),
                "tiers" => format!("{}/tiers", base),
                "dao" | "paste" | "p2p" | "plugins" => {
                    // Dioxus frontend routes
                    let web = base.replace("/solfunmeme", "/dioxus");
                    format!("{}/{}{}", web, action, query)
                }
                _ => format!("{}/{}{}", base, action, query),
            };
            println!("{}", url);
        }

        Cmd::Open { route } => {
            let web = cli.endpoint.replace("/solfunmeme", "/dioxus");
            let url = format!("{}{}", web, route);
            eprintln!("Opening {}", url);
            #[cfg(target_os = "linux")]
            { let _ = std::process::Command::new("xdg-open").arg(&url).spawn(); }
            #[cfg(target_os = "macos")]
            { let _ = std::process::Command::new("open").arg(&url).spawn(); }
            println!("{}", url);
        }
    }
}

#[cfg(not(feature = "native"))]
fn main() { eprintln!("requires native feature"); }

// Note: URL encoding support via erdfa-clean
// Pass state as URL params: ?vote=yea&holder=PUBKEY&endpoint=http://...
// Or as erdfa URL: erdfa convert "https://solana.solfunmeme.com/solfunmeme/paste?vote=yea"
