//! Project automation. Run with `cargo xtask <command>`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Deserialize;

#[derive(Parser)]
#[command(about = "hydrus-rs project automation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check (or raise) the parity baseline: coverage may never go backwards.
    Ratchet {
        /// Fail if coverage dropped below the baseline (default behaviour, for CI).
        #[arg(long)]
        check: bool,
        /// Raise the baseline to current coverage. Refuses to lower it.
        #[arg(long, conflicts_with = "check")]
        update: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Ratchet { update, .. } => ratchet(update),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace")
        .to_path_buf()
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
enum Status {
    Todo,
    Partial,
    Done,
}

#[derive(Debug, Deserialize)]
struct Item {
    #[serde(alias = "path", alias = "name")]
    id: String,
    status: Status,
}

/// Coverage for one category of the manifest: `done` and `partial` counts, and
/// the set of item ids that are `done` (so a regression in a specific item is
/// caught even if the totals are unchanged).
#[derive(Debug, Default, serde::Serialize, Deserialize, PartialEq)]
struct Coverage {
    total: usize,
    done: usize,
    partial: usize,
    done_items: Vec<String>,
}

fn load_manifest(root: &Path) -> Result<BTreeMap<String, Coverage>> {
    let path = root.join("parity/manifest.toml");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest: BTreeMap<String, Vec<Item>> =
        toml::from_str(&text).context("parsing parity manifest")?;
    let mut out = BTreeMap::new();
    for (category, items) in manifest {
        let mut cov = Coverage {
            total: items.len(),
            ..Coverage::default()
        };
        for item in items {
            match item.status {
                Status::Done => {
                    cov.done += 1;
                    cov.done_items.push(item.id);
                }
                Status::Partial => cov.partial += 1,
                Status::Todo => {}
            }
        }
        cov.done_items.sort();
        out.insert(category, cov);
    }
    Ok(out)
}

fn ratchet(update: bool) -> Result<()> {
    let root = workspace_root();
    let current = load_manifest(&root)?;
    let baseline_path = root.join("parity/baseline.json");
    let baseline: BTreeMap<String, Coverage> = match std::fs::read_to_string(&baseline_path) {
        Ok(text) => serde_json::from_str(&text).context("parsing parity baseline")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e).context("reading parity baseline"),
    };

    let mut regressions = Vec::new();
    for (category, base) in &baseline {
        let Some(cur) = current.get(category) else {
            regressions.push(format!(
                "category `{category}` disappeared from the manifest"
            ));
            continue;
        };
        for item in &base.done_items {
            if !cur.done_items.contains(item) {
                regressions.push(format!("{category}: `{item}` was done and no longer is"));
            }
        }
        if cur.done + cur.partial < base.done + base.partial {
            regressions.push(format!(
                "{category}: coverage fell from {} to {}",
                base.done + base.partial,
                cur.done + cur.partial
            ));
        }
    }

    for (category, cov) in &current {
        let pct = if cov.total == 0 {
            0.0
        } else {
            100.0 * cov.done as f64 / cov.total as f64
        };
        println!(
            "{category:>10}: {:>3}/{:<3} done ({pct:5.1}%), {:>3} partial",
            cov.done, cov.total, cov.partial
        );
    }

    if !regressions.is_empty() {
        bail!("parity regressed:\n  {}", regressions.join("\n  "));
    }

    if update {
        let json = serde_json::to_string_pretty(&current)? + "\n";
        std::fs::write(&baseline_path, json).context("writing parity baseline")?;
        println!("baseline updated");
    } else if current != baseline {
        println!(
            "coverage is ahead of the baseline; run `cargo xtask ratchet --update` to lock it in"
        );
    }
    Ok(())
}
