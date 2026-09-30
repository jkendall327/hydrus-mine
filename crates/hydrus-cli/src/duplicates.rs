//! `hydrus duplicates`: duplicates auto-resolution rules from the command
//! line: their progress, the pairs waiting for approval, and approving or
//! denying them.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use clap::Subcommand;

use hydrus_core::HashId;
use hydrus_store::Store;
use hydrus_store::duplicates::auto::{self, OperationMode, PairStatus, Rule};

#[derive(Subcommand)]
pub enum Action {
    /// Every rule, with how many pairs have each status.
    Rules,
    /// The pairs waiting for approval on a semi-automatic rule, as A then B.
    Pending {
        rule: String,
        /// At most this many.
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Apply a rule's action to pairs waiting for approval: the named pairs
    /// (`A,B` hashes), or with `--all` every waiting pair.
    Approve {
        rule: String,
        pairs: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Deny pairs waiting for approval (`A,B` hashes, or `--all`): the rule
    /// won't ask about them again unless its denials are reset.
    Deny {
        rule: String,
        pairs: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    Pause {
        rule: String,
    },
    Resume {
        rule: String,
    },
    /// Search every pair again.
    ResetSearch {
        rule: String,
    },
    /// Test every matching pair again.
    ResetTest {
        rule: String,
    },
    /// Search denied pairs again.
    ResetDenied {
        rule: String,
    },
    /// Work the rules now until they have nothing left to do (a running
    /// `serve` does this in the background).
    Run,
}

pub fn run(dir: &Path, action: Action) -> Result<()> {
    let store = Store::open(dir)?;
    match action {
        Action::Rules => list(&store),
        Action::Pending { rule, limit } => {
            let (id, _) = find(&store, &rule)?;
            for (_, a, b) in store.read(|conn| auto::pending_pairs(conn, id, Some(limit)))? {
                println!("{},{}", hash(&store, a)?, hash(&store, b)?);
            }
            Ok(())
        }
        Action::Approve { rule, pairs, all } => {
            let (id, _) = find(&store, &rule)?;
            let pairs = chosen_pairs(&store, id, &pairs, all)?;
            let n = hydrus_duplicates::approve(&store, id, &pairs)?;
            println!("approved {n} of {} pairs", pairs.len());
            Ok(())
        }
        Action::Deny { rule, pairs, all } => {
            let (id, _) = find(&store, &rule)?;
            let pairs = chosen_pairs(&store, id, &pairs, all)?;
            let n = hydrus_duplicates::deny(&store, id, &pairs)?;
            println!("denied {n} pairs");
            Ok(())
        }
        Action::Pause { rule } => {
            let (id, _) = find(&store, &rule)?;
            store.write(move |ctx| auto::set_paused(ctx.conn(), id, true))?;
            Ok(())
        }
        Action::Resume { rule } => {
            let (id, _) = find(&store, &rule)?;
            store.write(move |ctx| auto::set_paused(ctx.conn(), id, false))?;
            Ok(())
        }
        Action::ResetSearch { rule } => {
            let (id, _) = find(&store, &rule)?;
            store.write(move |ctx| auto::reset_search_progress(ctx.conn(), id))?;
            Ok(())
        }
        Action::ResetTest { rule } => {
            let (id, _) = find(&store, &rule)?;
            store.write(move |ctx| auto::reset_test_progress(ctx.conn(), id))?;
            Ok(())
        }
        Action::ResetDenied { rule } => {
            let (id, _) = find(&store, &rule)?;
            store.write(move |ctx| auto::reset_denied(ctx.conn(), id))?;
            Ok(())
        }
        Action::Run => {
            let clock = hydrus_search::Clock::system();
            let mut total = hydrus_duplicates::WorkDone::default();
            loop {
                let done = hydrus_duplicates::work_rules(
                    &store,
                    Duration::from_secs(60),
                    &mut hydrus_duplicates::Shuffle,
                    &clock,
                )?;
                total.searched += done.searched;
                total.tested += done.tested;
                total.actioned += done.actioned;
                total.queued += done.queued;
                if done.searched + done.tested == 0 && !done.more_to_do {
                    break;
                }
            }
            println!(
                "searched {} pairs, tested {}: {} actioned, {} waiting for approval",
                total.searched, total.tested, total.actioned, total.queued
            );
            Ok(())
        }
    }
}

fn find(store: &Store, name: &str) -> Result<(i64, Rule)> {
    store
        .read(auto::rules)?
        .into_iter()
        .find(|(_, r)| r.name == name)
        .with_context(|| format!("no auto-resolution rule is called \"{name}\""))
}

fn hash(store: &Store, id: HashId) -> Result<String> {
    let hashes = store.read(|conn| hydrus_store::master::hashes(conn, &[id]))?;
    Ok(hashes
        .get(&id)
        .map(hydrus_core::Sha256::to_hex)
        .unwrap_or_default())
}

fn chosen_pairs(
    store: &Store,
    rule_id: i64,
    given: &[String],
    all: bool,
) -> Result<Vec<(HashId, HashId)>> {
    if all {
        return Ok(store
            .read(|conn| auto::pending_pairs(conn, rule_id, None))?
            .into_iter()
            .map(|(_, a, b)| (a, b))
            .collect());
    }
    if given.is_empty() {
        bail!("name the pairs (as A,B hashes), or use --all");
    }
    let mut out = Vec::new();
    for pair in given {
        let Some((a, b)) = pair.split_once(',') else {
            bail!("\"{pair}\" is not a pair of hashes (A,B)");
        };
        let id = |text: &str| -> Result<HashId> {
            let hash: hydrus_core::Sha256 = text
                .trim()
                .parse()
                .map_err(|_| anyhow::anyhow!("\"{text}\" is not a sha256 hash"))?;
            store
                .read(|conn| hydrus_store::master::hash_id(conn, &hash))?
                .with_context(|| format!("no file has the hash {text}"))
        };
        out.push((id(a)?, id(b)?));
    }
    Ok(out)
}

fn list(store: &Store) -> Result<()> {
    let mut rules = store.read(auto::rules)?;
    rules.sort_by_cached_key(|(_, r)| hydrus_core::sort::human_sort_key(&r.name));
    for (id, rule) in rules {
        let counts = store.read(|conn| auto::counts(conn, id))?;
        let mode = match rule.mode {
            OperationMode::SemiAutomatic => "semi-automatic",
            OperationMode::FullyAutomatic => "fully automatic",
        };
        let paused = if rule.paused { ", paused" } else { "" };
        println!("{} ({mode}{paused})", rule.name);
        for status in PairStatus::ALL {
            let n = counts[&status];
            if n > 0 {
                println!("  {}: {n}", status.description());
            }
        }
    }
    Ok(())
}
