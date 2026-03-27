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
    /// Directory containing *-files.txt lists
    #[arg(long, default_value = "~/git/solana.solfunmeme")]
    lists: String,

    /// Output JSON report
    #[arg(long, default_value = "dedup-full.json")]
    output: String,
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

fn main() {
    let cli = Cli::parse();
    let lists_dir = expand_home(&cli.lists);

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

    // Group by hash
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

            dupe_groups.push(DuplicateGroup {
                hash: hash.clone(),
                size,
                copies,
                class: class.into(),
            });
        }
    }

    // Sort dupes by wasted space
    dupe_groups.sort_by(|a, b| (b.size * b.copies.len() as u64).cmp(&(a.size * a.copies.len() as u64)));

    let report = DedupReport {
        total_files: all_entries.len(),
        unique,
        within_repo_dupes: within,
        cross_repo_dupes: cross,
        wasted_bytes: wasted,
        repos: repo_summaries,
        duplicates: dupe_groups,
    };

    // Summary
    eprintln!("\n═══ DEDUP REPORT ═══");
    eprintln!("  Total files:      {}", report.total_files);
    eprintln!("  Unique:           {}", report.unique);
    eprintln!("  Within-repo dupes:{}", report.within_repo_dupes);
    eprintln!("  Cross-repo dupes: {}", report.cross_repo_dupes);
    eprintln!("  Wasted space:     {:.1} MB", report.wasted_bytes as f64 / 1024.0 / 1024.0);
    eprintln!("\n  Top duplicates:");
    for d in report.duplicates.iter().take(10) {
        eprintln!("    {}B ×{} [{}] {}", d.size, d.copies.len(), d.class, d.copies[0].split(':').last().unwrap_or("?"));
    }

    // Save
    let out_path = expand_home(&cli.output);
    std::fs::write(&out_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    eprintln!("\nSaved: {}", out_path.display());
}
