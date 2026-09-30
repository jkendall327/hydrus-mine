//! `hydrus subscriptions`: list, create and fill subscriptions from the
//! command line, many queries at once.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use clap::Subcommand;

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_parse::Downloaders;
use hydrus_store::Store;
use hydrus_store::queues::{self, SeedStatus};
use hydrus_store::subscriptions::{self as subs, Subscription};

#[derive(Subcommand)]
pub enum Action {
    /// Every subscription, with its downloader and queries.
    List,
    /// The downloaders (gallery URL generators) a subscription can use.
    Downloaders,
    /// A subscription's queries and what they have found.
    Show {
        name: String,
    },
    /// Add queries to a subscription, making it if it doesn't exist. Queries
    /// come from the arguments and from `--from` (a file with one query a
    /// line, or `-` for standard input; blank lines and `#` comments are
    /// skipped). Queries the subscription already has are left alone.
    Add {
        name: String,
        queries: Vec<String>,
        /// A file of queries, one a line (`-`: standard input).
        #[arg(long)]
        from: Option<PathBuf>,
        /// The downloader, by name (needed for a new subscription).
        #[arg(long)]
        downloader: Option<String>,
        /// Files a new query may get on its first check (0: no limit).
        #[arg(long)]
        initial_limit: Option<u64>,
        /// Files a query may get on a later check (0: no limit).
        #[arg(long)]
        periodic_limit: Option<u64>,
    },
    /// Remove queries (with their history), or the whole subscription if
    /// none are named.
    Remove {
        name: String,
        queries: Vec<String>,
    },
    /// Check queries (all, if none are named) at the next chance.
    CheckNow {
        name: String,
        queries: Vec<String>,
    },
    Pause {
        name: String,
    },
    Resume {
        name: String,
    },
}

pub fn run(dir: &Path, action: Action) -> Result<()> {
    let store = Store::open(dir)?;
    match action {
        Action::List => list(&store),
        Action::Downloaders => {
            let downloaders: Downloaders = store.read(hydrus_store::settings::get)?;
            for gug in &downloaders.gugs.gugs {
                println!("{}", gug.name());
            }
            Ok(())
        }
        Action::Show { name } => show(&store, &name),
        Action::Add {
            name,
            mut queries,
            from,
            downloader,
            initial_limit,
            periodic_limit,
        } => {
            if let Some(from) = from {
                queries.extend(read_queries(&from)?);
            }
            add(
                &store,
                &name,
                queries,
                downloader.as_deref(),
                initial_limit,
                periodic_limit,
            )
        }
        Action::Remove { name, queries } => {
            let sub = find(&store, &name)?;
            if queries.is_empty() {
                store.write(move |ctx| subs::delete_subscription(ctx.conn(), sub.id))?;
                println!("removed \"{name}\"");
                return Ok(());
            }
            let mut removed = 0;
            for query in store.read(|conn| subs::queries(conn, sub.id))? {
                if queries.contains(&query.state.query_text) {
                    store.write(move |ctx| subs::remove_query(ctx.conn(), query.queue_id))?;
                    removed += 1;
                }
            }
            println!("removed {} from \"{name}\"", queries_word(removed));
            Ok(())
        }
        Action::CheckNow { name, queries } => {
            let sub = find(&store, &name)?;
            let mut n = 0;
            for mut query in store.read(|conn| subs::queries(conn, sub.id))? {
                if queries.is_empty() || queries.contains(&query.state.query_text) {
                    query.state.check_now();
                    store.write(move |ctx| {
                        subs::set_query_state(ctx.conn(), query.queue_id, &query.state)
                    })?;
                    n += 1;
                }
            }
            println!(
                "{} of \"{name}\" will be checked at the next chance",
                queries_word(n)
            );
            Ok(())
        }
        Action::Pause { name } => set_paused(&store, &name, true),
        Action::Resume { name } => set_paused(&store, &name, false),
    }
}

fn read_queries(from: &Path) -> Result<Vec<String>> {
    let mut text = String::new();
    if from == Path::new("-") {
        std::io::stdin().read_to_string(&mut text)?;
    } else {
        text =
            std::fs::read_to_string(from).with_context(|| format!("reading {}", from.display()))?;
    }
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

fn find(store: &Store, name: &str) -> Result<Subscription> {
    let owned = name.to_owned();
    store
        .read(move |conn| subs::find_subscription(conn, &owned))?
        .with_context(|| format!("no subscription is called \"{name}\""))
}

fn queries_word(n: usize) -> String {
    if n == 1 {
        "1 query".into()
    } else {
        format!("{n} queries")
    }
}

fn limit(n: u64) -> Option<u64> {
    (n > 0).then_some(n)
}

fn add(
    store: &Store,
    name: &str,
    queries: Vec<String>,
    downloader: Option<&str>,
    initial_limit: Option<u64>,
    periodic_limit: Option<u64>,
) -> Result<()> {
    let existing = {
        let owned = name.to_owned();
        store.read(move |conn| subs::find_subscription(conn, &owned))?
    };
    let gug = match downloader {
        Some(wanted) => {
            let downloaders: Downloaders = store.read(hydrus_store::settings::get)?;
            let Some(gug) = downloaders
                .gugs
                .gugs
                .iter()
                .rev()
                .find(|g| g.name() == wanted)
            else {
                bail!(
                    "no downloader is called \"{wanted}\"; `hydrus subscriptions <dir> downloaders` lists them"
                );
            };
            Some((gug.key().to_owned(), gug.name().to_owned()))
        }
        None => None,
    };
    let id = if let Some(mut sub) = existing {
        let mut changed = false;
        if let Some((key, gug_name)) = gug {
            (sub.settings.gug_key, sub.settings.gug_name) = (key, gug_name);
            changed = true;
        }
        if let Some(n) = initial_limit {
            sub.settings.initial_file_limit = limit(n);
            changed = true;
        }
        if let Some(n) = periodic_limit {
            sub.settings.periodic_file_limit = limit(n);
            changed = true;
        }
        if changed {
            let settings = sub.settings.clone();
            store
                .write(move |ctx| subs::set_subscription_settings(ctx.conn(), sub.id, &settings))?;
        }
        sub.id
    } else {
        let Some((gug_key, gug_name)) = gug else {
            bail!("\"{name}\" is a new subscription: say which --downloader it uses");
        };
        let checkers: hydrus_core::subscriptions::CheckerDefaults =
            store.read(hydrus_store::settings::get)?;
        let mut settings = SubscriptionSettings {
            gug_key,
            gug_name,
            checker: checkers.subscriptions,
            ..SubscriptionSettings::default()
        };
        if let Some(n) = initial_limit {
            settings.initial_file_limit = limit(n);
        }
        if let Some(n) = periodic_limit {
            settings.periodic_file_limit = limit(n);
        }
        let owned = name.to_owned();
        store
            .write(move |ctx| subs::create_subscription(ctx.conn(), &owned, &settings))?
            .context("the subscription name is taken")?
    };
    let have: std::collections::BTreeSet<String> = store
        .read(|conn| subs::queries(conn, id))?
        .into_iter()
        .map(|q| q.state.query_text)
        .collect();
    let mut seen = have.clone();
    let fresh: Vec<String> = queries
        .into_iter()
        .filter(|q| seen.insert(q.clone()))
        .collect();
    let n = fresh.len();
    let now = hydrus_core::time::TimestampMs::now().0.div_euclid(1000);
    store.write(move |ctx| {
        for query in &fresh {
            subs::add_query(ctx.conn(), id, &QueryState::new(query.as_str()), now)?;
        }
        Ok(())
    })?;
    println!(
        "added {} to \"{name}\" ({} it already had)",
        queries_word(n),
        have.len()
    );
    Ok(())
}

fn set_paused(store: &Store, name: &str, paused: bool) -> Result<()> {
    let mut sub = find(store, name)?;
    sub.settings.paused = paused;
    if !paused {
        // resuming also clears a delay after errors
        sub.settings.no_work_until = 0;
        sub.settings.no_work_until_reason.clear();
    }
    let settings = sub.settings.clone();
    store.write(move |ctx| subs::set_subscription_settings(ctx.conn(), sub.id, &settings))?;
    Ok(())
}

fn list(store: &Store) -> Result<()> {
    for sub in store.read(subs::subscriptions)? {
        let queries = store.read(|conn| subs::queries(conn, sub.id))?;
        let dead = queries.iter().filter(|q| q.state.dead).count();
        let paused = queries.iter().filter(|q| q.state.paused).count();
        let mut line = format!(
            "{} [{}] {} queries",
            sub.name,
            sub.settings.gug_name,
            queries.len()
        );
        if dead > 0 {
            line += &format!(", {dead} dead");
        }
        if paused > 0 {
            line += &format!(", {paused} paused");
        }
        if sub.settings.paused {
            line += " (paused)";
        }
        if sub.settings.no_work_until > hydrus_core::time::TimestampMs::now().0.div_euclid(1000) {
            line += &format!(" (waiting: {})", sub.settings.no_work_until_reason);
        }
        println!("{line}");
    }
    Ok(())
}

fn show(store: &Store, name: &str) -> Result<()> {
    let sub = find(store, name)?;
    let now = hydrus_core::time::TimestampMs::now().0.div_euclid(1000);
    println!("{} [{}]", sub.name, sub.settings.gug_name);
    for q in store.read(|conn| subs::queries(conn, sub.id))? {
        let counts = store.read(|conn| queues::file_seed_counts(conn, q.queue_id))?;
        let total: usize = counts.values().sum();
        let todo = counts.get(&SeedStatus::Unknown).copied().unwrap_or(0);
        let state = &q.state;
        let when = if state.dead {
            "dead".to_owned()
        } else if state.paused {
            "paused".to_owned()
        } else if state.check_now || state.next_check_time <= now {
            "check due".to_owned()
        } else {
            format!(
                "next check in {}",
                hydrus_core::time::pretty_time_delta(state.next_check_time - now, false)
            )
        };
        println!(
            "  {}: {total} files found, {todo} to download, {when}",
            state.human_name()
        );
    }
    Ok(())
}
