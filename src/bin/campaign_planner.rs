/// campaign-planner: scan files → generate .dzn → call minizinc → print plan
///
/// Usage:
///   campaign-planner <dir-or-manifest.json> [--devnet] [--ecc golay|hamming|both]
///
/// Reads SHARD_MANIFEST.json or scans directory, generates MiniZinc .dzn,
/// invokes minizinc batch_campaign.mzn, prints per-file distribution plan.

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

#[cfg(feature = "native")]
use clap::Parser;

#[cfg(feature = "native")]
#[derive(Parser)]
#[command(name = "campaign-planner")]
struct Args {
    /// Path to directory or SHARD_MANIFEST.json
    path: PathBuf,
    /// Use devnet (all blockchain costs = $0)
    #[arg(long)]
    devnet: bool,
    /// Max total carriers
    #[arg(long, default_value = "500")]
    max_carriers: usize,
}

/// ECC codes from erdfa-publish registry + EC Zoo
struct EccCode {
    name: &'static str,
    n: u32,
    k: u32,
    d: u32,
    expansion_pct: u32, // 100 = 1×, 200 = 2×
}

const ECCS: &[EccCode] = &[
    EccCode { name: "none",       n: 1,  k: 1,  d: 1, expansion_pct: 100 },
    EccCode { name: "hamming743", n: 7,  k: 4,  d: 3, expansion_pct: 200 },
    EccCode { name: "golay24128", n: 24, k: 12, d: 8, expansion_pct: 200 },
];

/// Platform specs from erdfa-publish plugin registry
struct PlatformSpec {
    name: &'static str,
    capacity: u32,
    max_carriers: u32,
    cost_microusd: u32,
    cost_devnet: u32,
}

const PLATFORMS: &[PlatformSpec] = &[
    PlatformSpec { name: "tweet280",       capacity: 55,     max_carriers: 50,  cost_microusd: 0,      cost_devnet: 0 },
    PlatformSpec { name: "discord",        capacity: 972,    max_carriers: 20,  cost_microusd: 0,      cost_devnet: 0 },
    PlatformSpec { name: "instagram",      capacity: 537,    max_carriers: 20,  cost_microusd: 0,      cost_devnet: 0 },
    PlatformSpec { name: "tiktok",         capacity: 537,    max_carriers: 20,  cost_microusd: 0,      cost_devnet: 0 },
    PlatformSpec { name: "solana-memo",    capacity: 558,    max_carriers: 100, cost_microusd: 10000,  cost_devnet: 0 },
    PlatformSpec { name: "solana-account", capacity: 10240,  max_carriers: 100, cost_microusd: 70000,  cost_devnet: 0 },
    PlatformSpec { name: "nft-tile",       capacity: 196600, max_carriers: 10,  cost_microusd: 50000,  cost_devnet: 0 },
    PlatformSpec { name: "mastodon",       capacity: 112,    max_carriers: 20,  cost_microusd: 0,      cost_devnet: 0 },
    PlatformSpec { name: "bluesky",        capacity: 60,     max_carriers: 20,  cost_microusd: 0,      cost_devnet: 0 },
    PlatformSpec { name: "eth-sepolia",    capacity: 10240,  max_carriers: 50,  cost_microusd: 500000, cost_devnet: 0 },
];

struct FileEntry {
    name: String,
    bytes: usize,
}

fn load_manifest(path: &std::path::Path) -> Vec<FileEntry> {
    let data = std::fs::read_to_string(path).expect("read manifest");
    let v: serde_json::Value = serde_json::from_str(&data).expect("parse json");
    v["shards"].as_array().unwrap_or(&vec![]).iter().filter_map(|s| {
        let name = s["file"].as_str()?.to_string();
        let bytes = s["bytes"].as_u64()? as usize;
        if bytes > 0 { Some(FileEntry { name, bytes }) } else { None }
    }).collect()
}

fn scan_dir(path: &std::path::Path) -> Vec<FileEntry> {
    let mut files: Vec<FileEntry> = std::fs::read_dir(path).expect("read dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let bytes = e.metadata().ok()?.len() as usize;
            if bytes > 0 {
                Some(FileEntry { name: e.file_name().to_string_lossy().into(), bytes })
            } else { None }
        }).collect();
    files.sort_by_key(|f| f.name.clone());
    files
}

fn generate_dzn(files: &[FileEntry], devnet: bool, max_carriers: usize) -> String {
    let mut out = String::new();
    out.push_str(&format!("num_files = {};\n", files.len()));
    out.push_str(&format!("file_bytes = [{}];\n",
        files.iter().map(|f| f.bytes.to_string()).collect::<Vec<_>>().join(", ")));
    out.push_str(&format!("file_name = [{}];\n",
        files.iter().map(|f| format!("\"{}\"", f.name.replace('"', "'"))).collect::<Vec<_>>().join(", ")));

    out.push_str(&format!("\nnum_platforms = {};\n", PLATFORMS.len()));
    out.push_str(&format!("platform_name = [{}];\n",
        PLATFORMS.iter().map(|p| format!("\"{}\"", p.name)).collect::<Vec<_>>().join(", ")));
    out.push_str(&format!("capacity = [{}];\n",
        PLATFORMS.iter().map(|p| p.capacity.to_string()).collect::<Vec<_>>().join(", ")));
    out.push_str(&format!("max_carriers = [{}];\n",
        PLATFORMS.iter().map(|p| p.max_carriers.to_string()).collect::<Vec<_>>().join(", ")));
    out.push_str(&format!("cost_microusd = [{}];\n",
        PLATFORMS.iter().map(|p| if devnet { p.cost_devnet } else { p.cost_microusd }.to_string())
            .collect::<Vec<_>>().join(", ")));

    out.push_str(&format!("\nnum_eccs = {};\n", ECCS.len()));
    out.push_str(&format!("ecc_name = [{}];\n",
        ECCS.iter().map(|e| format!("\"{}\"", e.name)).collect::<Vec<_>>().join(", ")));
    out.push_str(&format!("ecc_n = [{}];\n",
        ECCS.iter().map(|e| e.n.to_string()).collect::<Vec<_>>().join(", ")));
    out.push_str(&format!("ecc_k = [{}];\n",
        ECCS.iter().map(|e| e.k.to_string()).collect::<Vec<_>>().join(", ")));
    out.push_str(&format!("ecc_d = [{}];\n",
        ECCS.iter().map(|e| e.d.to_string()).collect::<Vec<_>>().join(", ")));
    out.push_str(&format!("ecc_expansion_pct = [{}];\n",
        ECCS.iter().map(|e| e.expansion_pct.to_string()).collect::<Vec<_>>().join(", ")));

    out.push_str(&format!("\nmax_total_carriers = {};\n", max_carriers));
    out.push_str(&format!("devnet = {};\n", devnet));
    out
}

#[cfg(feature = "native")]
fn main() {
    let args = Args::parse();
    let files = if args.path.extension().map(|e| e == "json").unwrap_or(false) {
        load_manifest(&args.path)
    } else {
        scan_dir(&args.path)
    };

    if files.is_empty() {
        eprintln!("No files found");
        std::process::exit(1);
    }

    let total: usize = files.iter().map(|f| f.bytes).sum();
    eprintln!("Files: {}  Total: {} bytes  Devnet: {}", files.len(), total, args.devnet);

    let dzn = generate_dzn(&files, args.devnet, args.max_carriers);

    // Write .dzn to temp file next to the model
    let mzn_dir = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    // Look for batch_campaign.mzn relative to binary or in minizinc/
    let model_candidates = [
        PathBuf::from("minizinc/batch_campaign.mzn"),
        mzn_dir.join("../minizinc/batch_campaign.mzn"),
    ];
    let model = model_candidates.iter().find(|p| p.exists())
        .expect("batch_campaign.mzn not found — run from erdfa-publish root");

    let dzn_path = model.parent().unwrap().join("_batch_generated.dzn");
    std::fs::write(&dzn_path, &dzn).expect("write .dzn");
    eprintln!("Generated: {}", dzn_path.display());

    // Call minizinc
    let output = Command::new("minizinc")
        .arg(model.to_str().unwrap())
        .arg(dzn_path.to_str().unwrap())
        .arg("--solver").arg("Gecode")
        .arg("--time-limit").arg("30000")
        .output()
        .expect("minizinc not found — install via nix-shell -p minizinc");

    std::io::stdout().write_all(&output.stdout).unwrap();
    if !output.stderr.is_empty() {
        std::io::stderr().write_all(&output.stderr).unwrap();
    }

    // Cleanup
    let _ = std::fs::remove_file(&dzn_path);
}

#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("campaign-planner requires native feature (clap)");
}
