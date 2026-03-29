/// git-index — Index git repos under a root directory
/// Uses std::process::Command for git calls, rayon for parallelism
/// Part of erdfa-publish
use anyhow::Result;
use clap::Parser;
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "git-index", about = "Index git repos with last commit info")]
struct Args {
    /// Root directory to scan
    root: PathBuf,
    /// Output directory
    #[arg(short, long, default_value = "data")]
    output: PathBuf,
    /// Max directory depth
    #[arg(short, long, default_value = "2")]
    depth: usize,
}

#[derive(Serialize, Clone)]
struct RepoEntry {
    path: String,
    branch: String,
    last_commit: String,
    subject: String,
    remote: String,
}

fn git(repo: &Path, args: &[&str]) -> String {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn find_repos(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    find_repos_inner(root, 0, max_depth, &mut repos);
    repos
}

fn find_repos_inner(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
    if depth > max_depth {
        return;
    }
    if dir.join(".git").exists() || dir.join("HEAD").exists() {
        out.push(dir.to_path_buf());
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                let name = p.file_name().unwrap_or_default().to_str().unwrap_or("");
                if name != ".git" && name != "target" && name != "node_modules" {
                    find_repos_inner(&p, depth + 1, max_depth, out);
                }
            }
        }
    }
}

fn index_repo(repo: &Path, root: &Path) -> RepoEntry {
    let rel = repo.strip_prefix(root).unwrap_or(repo).to_string_lossy().to_string();
    RepoEntry {
        path: rel,
        branch: git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        last_commit: git(repo, &["log", "-1", "--format=%ci"]).get(..10).unwrap_or("unknown").to_string(),
        subject: {
            let s = git(repo, &["log", "-1", "--format=%s"]);
            s.chars().take(80).collect()
        },
        remote: git(repo, &["remote", "get-url", "origin"]),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output)?;

    eprintln!("Scanning {} (depth {})...", args.root.display(), args.depth);
    let repos = find_repos(&args.root, args.depth);
    eprintln!("Found {} repos, indexing...", repos.len());

    let root = args.root.clone();
    let mut entries: Vec<RepoEntry> = repos
        .par_iter()
        .map(|r| index_repo(r, &root))
        .collect();

    entries.sort_by(|a, b| b.last_commit.cmp(&a.last_commit));

    // JSON
    let json = serde_json::json!({"total": entries.len(), "repos": entries});
    fs::write(args.output.join("repo-index.json"), serde_json::to_string_pretty(&json)?)?;

    // Markdown
    let mut md = fs::File::create(args.output.join("repo-index.md"))?;
    writeln!(md, "# Git Repository Index\n")?;
    writeln!(md, "**Root**: {}", args.root.display())?;
    writeln!(md, "**Total**: {} repos\n", entries.len())?;
    writeln!(md, "| Last Commit | Repo | Branch | Last Message | Remote |")?;
    writeln!(md, "|-------------|------|--------|-------------|--------|")?;
    for e in &entries {
        let remote_short = if e.remote.len() > 60 { &e.remote[..60] } else { &e.remote };
        writeln!(md, "| {} | `{}` | {} | {} | {} |", e.last_commit, e.path, e.branch, e.subject, remote_short)?;
    }

    eprintln!("Done: {} repos → {}/repo-index.md", entries.len(), args.output.display());
    Ok(())
}
