//! CRQ-SWAB-002: Shard deduplication tool.
//! Reads file lists, computes SHA-256 + Hecke eigenvalues, classifies duplicates.
//!
//! Usage:
//!   shard-dedup --lists ~/git/solana.solfunmeme/ --output dedup-full.json
//!   shard-dedup --lists ~/git/solana.solfunmeme/ --output dedup-full.json --execute

use clap::Parser;
use erdfa_publish::hecke::{hecke_eigenvalue, orbifold_coords};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "shard-dedup", about = "CRQ-SWAB-002: Deduplicate shard repositories")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand)]
enum Cmd {
    /// Scan file lists and generate dedup report
    Scan {
        /// Directory containing *-files.txt lists
        #[arg(long, default_value = "~/git/solana.solfunmeme")]
        lists: String,
        /// Output JSON report
        #[arg(long, default_value = "~/git/solana.solfunmeme/dedup-full.json")]
        output: String,
    },
    /// Compare repos using existing dedup report
    Compare {
        /// Path to dedup-full.json
        #[arg(long, default_value = "~/git/solana.solfunmeme/dedup-full.json")]
        report: String,
    },
    /// List files safe to delete (target/ dirs, confirmed dupes)
    Clean {
        /// Path to dedup-full.json
        #[arg(long, default_value = "~/git/solana.solfunmeme/dedup-full.json")]
        report: String,
        /// Actually delete (default: dry-run)
        #[arg(long)]
        execute: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct FileEntry {
    path: String,
    repo: String,
    hash: String,
    size: u64,
    lines: usize,
    shard_id: usize,
    orbifold: (u8, u8, u8),
    class: String, // UNIQUE, DUPLICATE, CROSS-REPO
}

#[derive(Debug, Serialize, Deserialize)]
struct DedupReport {
    total_files: usize,
    unique: usize,
    within_repo_dupes: usize,
    cross_repo_dupes: usize,
    wasted_bytes: u64,
    repos: Vec<RepoSummary>,
    duplicates: Vec<DuplicateGroup>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RepoSummary {
    name: String,
    files: usize,
    bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct DuplicateGroup {
    hash: String,
    size: u64,
    copies: Vec<String>, // repo:path
    class: String,
}

fn expand_home(p: &str) -> PathBuf {
    if p.starts_with("~/") {
        PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(&p[2..])
    } else {
        PathBuf::from(p)
    }
}

fn scan_repo(list_path: &Path, repo_name: &str, base_dir: &str) -> Vec<FileEntry> {
    let content = match std::fs::read_to_string(list_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut entries = Vec::new();
    for line in content.lines() {
        let rel = line.trim();
        if rel.is_empty() { continue; }
        let full = PathBuf::from(base_dir).join(rel);
        let meta = match std::fs::metadata(&full) {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };
        let size = meta.len();
        if size > 10_000_000 { continue; } // skip >10MB
        let data = match std::fs::read(&full) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let lines = data.iter().filter(|&&b| b == b'\n').count();
        let hash = hex::encode(Sha256::digest(&data));
        let ev = hecke_eigenvalue(&data, lines, size as usize);
        let orb = orbifold_coords(&data);
        entries.push(FileEntry {
            path: rel.to_string(),
            repo: repo_name.to_string(),
            hash,
            size,
            lines,
            shard_id: ev.shard_id,
            orbifold: orb,
            class: String::new(),
        });
    }
    entries
}

fn run_scan(lists: &str, output: &str) {
    let lists_dir = expand_home(lists);

    let repos: Vec<(&str, &str, &str)> = vec![
        ("introspector", "introspector-files.txt", "/mnt/data1/introspector/introspector"),
        ("shard70", "shard70-work-files.txt", "/mnt/data1/git/shards/shard70-work"),
        ("shards71", "shards-71-files.txt", "/mnt/data1/shards/71"),
    ];

    let mut all_entries = Vec::new();
    let mut repo_summaries = Vec::new();

    for (name, list, base) in &repos {
        let list_path = lists_dir.join(list);
        eprintln!("Scanning {}...", name);
        let entries = scan_repo(&list_path, name, base);
        let bytes: u64 = entries.iter().map(|e| e.size).sum();
        eprintln!("  {} files, {} MB", entries.len(), bytes / 1024 / 1024);
        repo_summaries.push(RepoSummary { name: name.to_string(), files: entries.len(), bytes });
        all_entries.extend(entries);
    }

    let mut by_hash: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in all_entries.iter().enumerate() {
        by_hash.entry(e.hash.clone()).or_default().push(i);
    }

    let mut unique = 0;
    let mut within = 0;
    let mut cross = 0;
    let mut wasted: u64 = 0;
    let mut dupe_groups = Vec::new();

    for (hash, indices) in &by_hash {
        if indices.len() == 1 {
            all_entries[indices[0]].class = "UNIQUE".into();
            unique += 1;
        } else {
            let repos_in: std::collections::HashSet<&str> =
                indices.iter().map(|&i| all_entries[i].repo.as_str()).collect();
            let class = if repos_in.len() > 1 { "CROSS-REPO" } else { "DUPLICATE" };
            let size = all_entries[indices[0]].size;
            let copies: Vec<String> = indices.iter()
                .map(|&i| format!("{}:{}", all_entries[i].repo, all_entries[i].path))
                .collect();
            for &i in indices { all_entries[i].class = class.into(); }
            wasted += size * (indices.len() as u64 - 1);
            if class == "CROSS-REPO" { cross += indices.len(); } else { within += indices.len(); }
            dupe_groups.push(DuplicateGroup { hash: hash.clone(), size, copies, class: class.into() });
        }
    }

    dupe_groups.sort_by(|a, b| (b.size * b.copies.len() as u64).cmp(&(a.size * a.copies.len() as u64)));

    let report = DedupReport {
        total_files: all_entries.len(), unique, within_repo_dupes: within,
        cross_repo_dupes: cross, wasted_bytes: wasted, repos: repo_summaries, duplicates: dupe_groups,
    };

    eprintln!("\n═══ DEDUP REPORT ═══");
    eprintln!("  Total: {} Unique: {} Within: {} Cross: {} Wasted: {:.1}MB",
        report.total_files, report.unique, report.within_repo_dupes,
        report.cross_repo_dupes, report.wasted_bytes as f64 / 1024.0 / 1024.0);

    let out_path = expand_home(output);
    std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!("Saved: {}", out_path.display());
}

fn run_compare(report_path: &str) {
    let report: DedupReport = serde_json::from_str(
        &std::fs::read_to_string(expand_home(report_path)).expect("cannot read report")
    ).expect("invalid JSON");

    eprintln!("═══ CROSS-REPO COMPARISON ═══\n");

    // Per-repo stats
    for r in &report.repos {
        eprintln!("  {:15} {:>6} files  {:>6} MB", r.name, r.files, r.bytes / 1024 / 1024);
    }

    // Cross-repo overlap
    let mut overlap: HashMap<String, (usize, u64)> = HashMap::new();
    for g in &report.duplicates {
        if g.class != "CROSS-REPO" { continue; }
        let mut repos: Vec<&str> = g.copies.iter().map(|c| c.split(':').next().unwrap()).collect();
        repos.sort(); repos.dedup();
        let key = repos.join("↔");
        let e = overlap.entry(key).or_insert((0, 0));
        e.0 += 1;
        e.1 += g.size * (g.copies.len() as u64 - 1);
    }

    eprintln!("\n  Cross-repo overlap:");
    for (pair, (count, bytes)) in &overlap {
        eprintln!("    {} — {} groups, {:.1} MB wasted", pair, count, *bytes as f64 / 1024.0 / 1024.0);
    }

    // Unique-to-repo files (files only in one repo, not duplicated anywhere)
    let mut repo_unique: HashMap<String, usize> = HashMap::new();
    let total_unique = report.unique;
    eprintln!("\n  Unique files: {}", total_unique);
    eprintln!("  Duplicate groups: {}", report.duplicates.len());
    eprintln!("  Wasted: {:.1} MB", report.wasted_bytes as f64 / 1024.0 / 1024.0);

    // Recommendation
    if let Some((pair, (count, bytes))) = overlap.iter().max_by_key(|(_, (_, b))| *b) {
        eprintln!("\n  ⚠️  Highest overlap: {} ({} groups, {:.1}MB)", pair, count, *bytes as f64 / 1024.0 / 1024.0);
        if pair.contains("introspector") && pair.contains("shard70") {
            eprintln!("  → shard70-work appears to be a copy of introspector");
            eprintln!("  → Safe to remove after confirming via: shard-dedup clean --report {}", report_path);
        }
    }
}

fn run_clean(report_path: &str, execute: bool) {
    let report: DedupReport = serde_json::from_str(
        &std::fs::read_to_string(expand_home(report_path)).expect("cannot read report")
    ).expect("invalid JSON");

    let mode = if execute { "EXECUTE" } else { "DRY-RUN" };
    eprintln!("═══ CLEAN [{}] ═══\n", mode);

    let mut to_delete: Vec<(String, u64)> = Vec::new();

    for g in &report.duplicates {
        if g.copies.len() < 2 { continue; }
        // Keep first copy (canonical), mark rest for deletion
        for copy in &g.copies[1..] {
            let parts: Vec<&str> = copy.splitn(2, ':').collect();
            if parts.len() == 2 {
                let repo = parts[0];
                let path = parts[1];
                let full = match repo {
                    "introspector" => format!("/mnt/data1/introspector/introspector/{}", path),
                    "shard70" => format!("/mnt/data1/git/shards/shard70-work/{}", path),
                    "shards71" => format!("/mnt/data1/shards/71/{}", path),
                    _ => continue,
                };
                to_delete.push((full, g.size));
            }
        }
    }

    let total_bytes: u64 = to_delete.iter().map(|(_, s)| s).sum();
    eprintln!("  Files to delete: {}", to_delete.len());
    eprintln!("  Space to free: {:.1} MB", total_bytes as f64 / 1024.0 / 1024.0);

    if execute {
        let mut deleted = 0;
        for (path, _) in &to_delete {
            if std::fs::remove_file(path).is_ok() { deleted += 1; }
        }
        eprintln!("  Deleted: {}/{}", deleted, to_delete.len());
    } else {
        eprintln!("\n  First 10 candidates:");
        for (path, size) in to_delete.iter().take(10) {
            eprintln!("    rm {} ({}B)", path, size);
        }
        eprintln!("\n  Run with --execute to delete");
    }
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Scan { lists, output } => run_scan(&lists, &output),
        Cmd::Compare { report } => run_compare(&report),
        Cmd::Clean { report, execute } => run_clean(&report, execute),
    }
}
