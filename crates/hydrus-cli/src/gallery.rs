//! `hydrus gallery`: start gallery searches (the reference's gallery
//! downloader page), one per query; a running `hydrus serve` works them.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use clap::Args;

use hydrus_parse::Downloaders;
use hydrus_store::Store;

#[derive(Args)]
pub struct Search {
    /// The downloader, by name (`hydrus subscriptions <dir> downloaders`
    /// lists them).
    #[arg(long)]
    downloader: String,
    /// Queries, one search each.
    queries: Vec<String>,
    /// A file of queries, one a line (`-`: standard input).
    #[arg(long)]
    from: Option<PathBuf>,
    /// Stop each search after this many new files (default: the client's
    /// gallery file limit; 0: no limit).
    #[arg(long)]
    limit: Option<u64>,
    /// The gallery page to put them on.
    #[arg(long)]
    page: Option<String>,
}

pub fn run(dir: &Path, search: Search) -> Result<()> {
    let store = Store::open(dir)?;
    let mut queries = search.queries;
    if let Some(from) = &search.from {
        queries.extend(crate::subscriptions::read_queries(from)?);
    }
    if queries.is_empty() {
        bail!("no queries given");
    }
    let definitions: Downloaders = store.read(hydrus_store::settings::get)?;
    let Some(gug) = definitions
        .gugs
        .gugs
        .iter()
        .rev()
        .find(|g| g.name() == search.downloader)
    else {
        bail!("no downloader is called \"{}\"", search.downloader);
    };
    let limit = search.limit.map(|n| (n > 0).then_some(n));
    let how = hydrus_store::gallery::NewSearches {
        page_name: search.page.as_deref(),
        gug_key: gug.key(),
        gug_name: gug.name(),
        file_limit: limit,
        ..Default::default()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let made =
        hydrus_store::gallery::create_gallery_searches(&store, &definitions, &how, &queries, now)
            .context("starting the searches")?
            .queues;
    println!(
        "started {} search{} on \"{}\"",
        made.len(),
        if made.len() == 1 { "" } else { "es" },
        made.first().map_or("gallery", |q| q.name.as_str())
    );
    Ok(())
}
