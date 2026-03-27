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
        #[arg(long)]
        key: Option<String>,
    },
    /// Show DAO status and your tier
    Status,
    /// Vote on today's bill
    Vote { choice: String },
    /// Submit transaction data for bounty
    Submit { file: String },
    /// Show Fibonacci tier boundaries
    Tiers,
    /// Run Lean4 proof verification
    Prove,
    /// Split file into 71 Gandalf shards with PQC signatures
    Shards {
        file: String,
        #[arg(long, default_value = "./shards")]
        out: String,
    },
    /// Generate erdfa URL for an action
    Url { action: String, params: Vec<String> },
    /// Open the dioxus app at a specific route
    Open {
        #[arg(default_value = "/")]
        route: String,
    },
    /// Check all 9 deployment platforms + services
    Platforms,
    /// Deploy to one or all platforms (build shards first)
    Deploy {
        /// Platform name or "all" (cf, netlify, vercel, gh, hf, oci, all)
        #[arg(default_value = "all")]
        target: String,
    },
    /// HTTP probe all deployments, post to telemetry
    Test,
    /// Run zkperf witnesses on all deployments
    Zkperf,
    /// Mesh subcommands
    Mesh {
        #[command(subcommand)]
        sub: MeshCmd,
    },
}

#[cfg(feature = "native")]
#[derive(Subcommand)]
enum MeshCmd {
    /// Show mesh logs
    Logs,
    /// Show/register mesh peers
    Peers,
    /// Post a message to the mesh
    Ping { msg: String },
    /// Sync logs with WireGuard peers
    Sync,
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

        Cmd::Platforms => {
            let platforms = [
                ("github-pages", "https://meta-introspector.github.io/solfunmeme-dioxus/"),
                ("cloudflare", "https://solfunmeme-dioxus.pages.dev/"),
                ("vercel", "https://solfunmeme-dioxus.vercel.app/"),
                ("huggingface", "https://introspector-solfunmeme-dioxus.hf.space/"),
                ("oracle-oci", "https://objectstorage.us-ashburn-1.oraclecloud.com/n/id1iqr236pdp/b/solfunmeme-dioxus/o/index.html"),
                ("netlify", "https://solfunmeme.netlify.app/"),
                ("render", "https://solfunmeme-static.onrender.com/"),
                ("self-hosted", "http://192.168.68.62/dioxus/"),
                ("supabase", "https://aesruozmcbvtutpoyaze.supabase.co/rest/v1/mesh_logs?limit=0"),
            ];
            eprintln!("SOLFUNMEME Deployment Platforms");
            let supa_key = std::fs::read_to_string(
                format!("{}/.solfunmeme/supabase-anon-key", std::env::var("HOME").unwrap_or_default())
            ).unwrap_or_default().trim().to_string();
            for (name, url) in &platforms {
                let mut req = ureq::get(url).timeout(std::time::Duration::from_secs(10));
                if *name == "supabase" && !supa_key.is_empty() {
                    req = req.set("apikey", &supa_key);
                }
                let code = req.call().map(|r| r.status()).unwrap_or(0);
                let s = if code == 200 { "✅" } else { "❌" };
                if *name == "supabase" {
                    // Also show row count
                    let count = ureq::get(&format!("https://aesruozmcbvtutpoyaze.supabase.co/rest/v1/mesh_logs?select=id"))
                        .set("apikey", &supa_key).set("Prefer", "count=exact")
                        .timeout(std::time::Duration::from_secs(5))
                        .call().ok().and_then(|r| r.header("content-range").map(|h| h.to_string()))
                        .unwrap_or_default();
                    eprintln!("  {} {:20} {:3} {} ({})", s, name, code, url, count);
                } else {
                    eprintln!("  {} {:20} {:3} {}", s, name, code, url);
                }
            }
            eprintln!("\nServices:");
            for svc in ["solfunmeme-service", "solfunmeme-dioxus", "prometheus", "jaeger"] {
                let ok = std::process::Command::new("systemctl")
                    .args(["--user", "is-active", svc]).output()
                    .map(|o| o.status.success()).unwrap_or(false);
                eprintln!("  {} {}", if ok { "✅" } else { "❌" }, svc);
            }
        }

        Cmd::Deploy { target } => {
            let targets: Vec<&str> = if target == "all" {
                vec!["gh", "cf", "vercel", "netlify", "oci", "hf"]
            } else { vec![target.as_str()] };
            for t in targets {
                eprint!("  {} → ", t);
                let svc = match t {
                    "gh" => "kagenti-solfunmeme-github-pages",
                    "cf" => "kagenti-solfunmeme-cloudflare",
                    "vercel" => "kagenti-solfunmeme-vercel",
                    "netlify" => "kagenti-solfunmeme-netlify",
                    "oci" => "kagenti-solfunmeme-oracle-oci",
                    "hf" => "kagenti-solfunmeme-huggingface",
                    _ => { eprintln!("unknown"); continue; }
                };
                let ok = std::process::Command::new("systemctl")
                    .args(["--user", "start", svc]).status().map(|s| s.success()).unwrap_or(false);
                eprintln!("{}", if ok { "✅ triggered" } else { "❌" });
            }
        }

        Cmd::Test => {
            eprintln!("Running probe...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "start", "kagenti-solfunmeme-test"]).status();
            std::thread::sleep(std::time::Duration::from_secs(2));
            let _ = std::process::Command::new("journalctl")
                .args(["--user", "-u", "kagenti-solfunmeme-test", "--no-pager", "-n", "12"]).status();
        }

        Cmd::Zkperf => {
            eprintln!("Running zkperf...");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "start", "kagenti-solfunmeme-zkperf"]).status();
            std::thread::sleep(std::time::Duration::from_secs(20));
            let _ = std::process::Command::new("journalctl")
                .args(["--user", "-u", "kagenti-solfunmeme-zkperf", "--no-pager", "-n", "15"]).status();
        }

        Cmd::Mesh { sub } => match sub {
            MeshCmd::Logs => {
                let resp: serde_json::Value = ureq::get(&format!("{}/mesh/logs", cli.endpoint))
                    .call().unwrap().into_json().unwrap();
                eprintln!("Mesh logs: {}", resp["count"]);
                if let Some(logs) = resp["logs"].as_array() {
                    for l in logs.iter().rev().take(10) {
                        if let Some(fields) = l["fields"].as_array() {
                            let fmap: std::collections::HashMap<&str, &str> = fields.iter()
                                .filter_map(|f| f["Revealed"].as_object().map(|r| (r["key"].as_str().unwrap_or(""), r["value"].as_str().unwrap_or(""))))
                                .collect();
                            eprintln!("  {:15} {:20} {}", fmap.get("type").unwrap_or(&"?"), fmap.get("from").unwrap_or(&"?"), fmap.get("msg").unwrap_or(&""));
                        }
                    }
                }
            }
            MeshCmd::Peers => {
                let resp: serde_json::Value = ureq::get(&format!("{}/mesh/peers", cli.endpoint))
                    .call().unwrap().into_json().unwrap();
                eprintln!("This node: {} ({})", resp["node"], resp["address"]);
                if let Some(peers) = resp["peers"].as_array() {
                    for p in peers {
                        eprintln!("  {:15} {:15} {}", p["node"].as_str().unwrap_or("?"), p["address"].as_str().unwrap_or("?"), p["endpoint"].as_str().unwrap_or(""));
                    }
                }
            }
            MeshCmd::Ping { msg } => {
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
                let body = serde_json::json!({"type":"cli-ping","from":"solfunmeme-cli","msg":msg,"ts":ts});
                let resp = ureq::post(&format!("{}/mesh/logs", cli.endpoint)).send_json(&body).unwrap();
                eprintln!("Posted: {}", resp.status());
            }
            MeshCmd::Sync => {
                let home = std::env::var("HOME").unwrap_or_default();
                let _ = std::process::Command::new("bash")
                    .arg(format!("{}/.solfunmeme/mesh-sync.sh", home)).status();
            }
        },
    }
}

#[cfg(not(feature = "native"))]
fn main() { eprintln!("requires native feature"); }

// Note: URL encoding support via erdfa-clean
// Pass state as URL params: ?vote=yea&holder=PUBKEY&endpoint=http://...
// Or as erdfa URL: erdfa convert "https://solana.solfunmeme.com/solfunmeme/paste?vote=yea"
