//! `hydrus`: import a hydrus install, serve the Client API, run maintenance.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

use hydrus_api::AppState;
use hydrus_api::server::{ServerOptions, serve};
use hydrus_core::ServiceType;
use hydrus_store::Store;
use hydrus_store::services::ServiceKind;
use hydrus_store::store::DB_FILE_NAME;
use hydrus_store::transfer::{TransferMode, transfer_media};

/// The reference client's default Client API port.
const DEFAULT_PORT: u16 = 45869;

mod api_keys;
mod duplicates;
mod folders;
mod gallery;
mod pauses;
mod queues;
mod subscriptions;

#[derive(Parser)]
#[command(name = "hydrus", version, about = "A fast, native hydrus client.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import an existing hydrus install (its `db` directory) into a new
    /// hydrus-rs store. The install is only read, never changed (except with
    /// `--files move`).
    ImportLegacy {
        /// The hydrus `db` directory (holding client.db).
        source: PathBuf,
        /// A new directory for the hydrus-rs store.
        dest: PathBuf,
        /// How to bring the media files across.
        #[arg(long, value_enum, default_value_t = FilesMode::Hardlink)]
        files: FilesMode,
    },
    /// Serve the Client API.
    Serve {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        /// Port to listen on (default: the Client API service's setting).
        #[arg(long)]
        port: Option<u16>,
        /// Address to listen on (default: localhost, or every interface if
        /// the Client API service allows non-local connections).
        #[arg(long)]
        bind: Option<IpAddr>,
    },
    /// Delete from disk the files that were deleted from local storage (to
    /// the recycle bin, if hydrus's "delete to recycle bin" option was on).
    Purge { dir: PathBuf },
    /// Start gallery searches (a gallery downloader page), one per query.
    /// A running `serve` picks them up within a minute.
    Gallery {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[command(flatten)]
        search: gallery::Search,
    },
    /// Manage subscriptions: list them, add many queries at once, check,
    /// pause and resume. A running `serve` picks changes up within minutes.
    Subscriptions {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[command(subcommand)]
        action: subscriptions::Action,
    },
    /// Import folders: list them, check one now, pause, resume, or run one
    /// here. A running `serve` checks them when due.
    ImportFolders {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[command(subcommand)]
        action: folders::Action,
    },
    /// Export folders: list them, or run one now. A running `serve` runs
    /// them when due.
    ExportFolders {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[command(subcommand)]
        action: folders::ExportAction,
    },
    /// Client API access keys: list, add and remove them, or accept the keys
    /// tools ask for. A running `serve` notices changes within a minute.
    ApiKeys {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[command(subcommand)]
        action: api_keys::Action,
    },
    /// The downloaders' queues: URL downloaders, gallery searches and
    /// watchers, by page.
    Queues {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[command(subcommand)]
        action: queues::Action,
    },
    /// Duplicates auto-resolution: rules' progress, and approving or denying
    /// the pairs semi-automatic rules are waiting on.
    Duplicates {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[command(subcommand)]
        action: duplicates::Action,
    },
    /// Pause subscriptions, the network or the downloader queues, as
    /// hydrus's "network > pause" menu does (your pauses come across from
    /// hydrus). With nothing to pause, says what is paused.
    Pause {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[arg(value_enum)]
        what: Option<pauses::What>,
    },
    /// Resume what `pause` paused.
    Resume {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[arg(value_enum)]
        what: pauses::What,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum FilesMode {
    /// New directory entries for the same files: no extra space, and the
    /// two installs stay independent. Needs the same filesystem.
    Hardlink,
    /// An independent copy.
    Copy,
    /// Take the files from the old install, which is left without them.
    Move,
    /// Keep using the old install's files where they are. hydrus-rs then
    /// never deletes media from disk.
    InPlace,
}

impl From<FilesMode> for TransferMode {
    fn from(mode: FilesMode) -> Self {
        match mode {
            FilesMode::Hardlink => TransferMode::Hardlink,
            FilesMode::Copy => TransferMode::Copy,
            FilesMode::Move => TransferMode::Move,
            FilesMode::InPlace => TransferMode::InPlace,
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    match Cli::parse().command {
        Command::ImportLegacy {
            source,
            dest,
            files,
        } => import_legacy(&source, &dest, files.into()),
        Command::Serve { dir, port, bind } => run_server(&dir, port, bind),
        Command::Gallery { dir, search } => {
            if !dir.join(DB_FILE_NAME).exists() {
                bail!(
                    "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
                    dir.display()
                );
            }
            gallery::run(&dir, search)
        }
        Command::Subscriptions { dir, action } => {
            if !dir.join(DB_FILE_NAME).exists() {
                bail!(
                    "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
                    dir.display()
                );
            }
            subscriptions::run(&dir, action)
        }
        Command::Queues { dir, action } => {
            if !dir.join(DB_FILE_NAME).exists() {
                bail!(
                    "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
                    dir.display()
                );
            }
            queues::run(&dir, action)
        }
        Command::ImportFolders { dir, action } => {
            if !dir.join(DB_FILE_NAME).exists() {
                bail!(
                    "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
                    dir.display()
                );
            }
            folders::run(&dir, action)
        }
        Command::ExportFolders { dir, action } => {
            if !dir.join(DB_FILE_NAME).exists() {
                bail!(
                    "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
                    dir.display()
                );
            }
            folders::run_export(&dir, action)
        }
        Command::ApiKeys { dir, action } => {
            if !dir.join(DB_FILE_NAME).exists() {
                bail!(
                    "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
                    dir.display()
                );
            }
            api_keys::run(&dir, action)
        }
        Command::Duplicates { dir, action } => {
            if !dir.join(DB_FILE_NAME).exists() {
                bail!(
                    "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
                    dir.display()
                );
            }
            duplicates::run(&dir, action)
        }
        Command::Pause { dir, what } => pauses::run(&dir, what, true),
        Command::Resume { dir, what } => pauses::run(&dir, Some(what), false),
        Command::Purge { dir } => {
            let store = Store::open(&dir)?;
            let report = hydrus_store::maintenance::purge_deleted_media(&store, usize::MAX)?;
            println!(
                "deleted {} files and {} thumbnails",
                report.files_deleted, report.thumbnails_deleted
            );
            Ok(())
        }
    }
}

fn import_legacy(source: &Path, dest: &Path, mode: TransferMode) -> Result<()> {
    if dest.join(DB_FILE_NAME).exists() {
        bail!("{} already holds a hydrus-rs store", dest.display());
    }
    if mode == TransferMode::Move
        && ["client.db-wal", "client.mappings.db-wal"]
            .iter()
            .any(|f| source.join(f).exists())
    {
        bail!(
            "the hydrus install at {} looks open (it has WAL files); close it before moving its files",
            source.display()
        );
    }
    std::fs::create_dir_all(dest)?;
    let db = dest.join(DB_FILE_NAME);
    println!("importing the database from {} ...", source.display());
    let started = std::time::Instant::now();
    let report = hydrus_store::import::import_legacy(source, &db)
        .with_context(|| format!("importing {}", source.display()))?;
    println!(
        "  {} services ({} with tags), {} rows, in {:.1?}",
        report.services,
        report.tag_services,
        report.rows.values().sum::<u64>(),
        started.elapsed()
    );
    let media = dest.join("client_files");
    println!("bringing the media across ({}) ...", mode_name(mode));
    let transfer = transfer_media(&db, &media, mode);
    let transfer = match transfer {
        Ok(t) => t,
        Err(e) => {
            // leave nothing half-done: the store would point at the old files
            let _ = std::fs::remove_file(&db);
            return Err(e).context("transferring media");
        }
    };
    if mode == TransferMode::InPlace {
        println!("  using the files in {}", transfer.destination.display());
    } else {
        println!(
            "  {} files, {:.1} GiB, into {}",
            transfer.files,
            transfer.bytes as f64 / f64::from(1u32 << 30),
            transfer.destination.display()
        );
        if transfer.skipped > 0 {
            println!(
                "  {} files left behind: deleted files hydrus hadn't cleared away yet, or files it doesn't know",
                transfer.skipped
            );
        }
    }
    // custom assets (the user's own star shapes, icons ...)
    let assets = source.join("static");
    if assets.is_dir() {
        let copied = copy_dir(&assets, &dest.join("static")).context("copying custom assets")?;
        println!("  {copied} custom assets (static) copied");
    }
    println!(
        "done. run `hydrus serve {}` to start the Client API.",
        dest.display()
    );
    Ok(())
}

/// Copy a directory tree, returning how many files were copied.
fn copy_dir(from: &Path, to: &Path) -> Result<u64> {
    std::fs::create_dir_all(to)?;
    let mut copied = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copied += copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

fn mode_name(mode: TransferMode) -> &'static str {
    match mode {
        TransferMode::Hardlink => "hardlinking",
        TransferMode::Copy => "copying",
        TransferMode::Move => "moving",
        TransferMode::InPlace => "in place",
    }
}

fn run_server(dir: &Path, port: Option<u16>, bind: Option<IpAddr>) -> Result<()> {
    if !dir.join(DB_FILE_NAME).exists() {
        bail!(
            "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
            dir.display()
        );
    }
    let store = Store::open(dir)?;
    let snap = store.snapshot();
    let config = snap
        .services
        .of_type(ServiceType::ClientApiService)
        .find_map(|s| match &s.kind {
            ServiceKind::ClientApi(config) => Some(config.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let port = port.or(config.port).unwrap_or(DEFAULT_PORT);
    let ip = bind.unwrap_or(if config.allow_non_local_connections {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    });
    if config.use_https {
        tracing::warn!(
            "the Client API service asks for https, which hydrus-rs does not serve yet; serving http"
        );
    }
    let options = ServerOptions {
        addr: SocketAddr::new(ip, port),
        cors: config.support_cors,
    };
    let state = AppState::new(store.clone())?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        // physical deletes, in the background
        let purger = store.clone();
        tokio::spawn(async move {
            loop {
                let store = purger.clone();
                match tokio::task::spawn_blocking(move || {
                    hydrus_store::maintenance::purge_deleted_media(&store, 1024)
                })
                .await
                {
                    Ok(Ok(report)) if report.files_deleted > 0 => {
                        tracing::info!(files = report.files_deleted, "purged deleted files");
                    }
                    Ok(Err(e)) => tracing::error!(error = %e, "purging deleted files failed"),
                    _ => {}
                }
                tokio::time::sleep(Duration::from_secs(600)).await;
            }
        });
        // emptying the trash: 30 seconds after starting, then hourly
        let trash = store.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            loop {
                let store = trash.clone();
                match tokio::task::spawn_blocking(move || {
                    hydrus_store::trash::maintain_trash(&store, 256)
                })
                .await
                {
                    Ok(Ok(report)) if report.total() > 0 => tracing::info!(
                        over_size = report.over_size,
                        over_age = report.over_age,
                        "deleted files from the trash"
                    ),
                    Ok(Err(e)) => tracing::error!(error = %e, "emptying the trash failed"),
                    _ => {}
                }
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });
        if let Some(downloads) = &state.downloads
            && let Err(e) = downloads.start_all()
        {
            tracing::error!(error = %e, "starting the download queues failed");
        }
        if let Some(subscriptions) = &state.subscriptions {
            subscriptions.start();
        }
        // the similar-files search, finding potential duplicates as files come in
        let searcher = store.clone();
        tokio::spawn(async move {
            loop {
                let store = searcher.clone();
                let done = tokio::task::spawn_blocking(move || {
                    hydrus_store::similar::run_search(&store, 1000)
                })
                .await;
                match done {
                    Ok(Ok(n)) if n > 0 => {
                        tracing::debug!(files = n, "searched for similar files");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Ok(Err(e)) => {
                        tracing::error!(error = %e, "the similar-files search failed");
                        tokio::time::sleep(Duration::from_secs(600)).await;
                    }
                    _ => tokio::time::sleep(Duration::from_secs(30)).await,
                }
            }
        });
        // duplicates auto-resolution: bursts of work with rests between, as
        // the reference's manager does (it counts us as always active)
        let resolver = store.clone();
        tokio::spawn(async move {
            loop {
                let store = resolver.clone();
                let settings: hydrus_store::duplicates::auto::AutoResolutionSettings = match store
                    .read(hydrus_store::settings::get)
                {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, "reading the auto-resolution settings failed");
                        tokio::time::sleep(Duration::from_secs(600)).await;
                        continue;
                    }
                };
                if !settings.during_active {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
                let budget = Duration::from_millis(u64::from(settings.work_time_ms_active));
                let started = std::time::Instant::now();
                let done = tokio::task::spawn_blocking(move || {
                    hydrus_duplicates::work_rules(
                        &store,
                        budget,
                        &mut hydrus_duplicates::Shuffle,
                        &hydrus_search::Clock::system(),
                    )
                })
                .await;
                let rest = match done {
                    Ok(Ok(done)) if done.more_to_do => {
                        tracing::debug!(?done, "auto-resolution worked");
                        let worked = started.elapsed().min(budget * 5);
                        worked * settings.rest_percentage_active / 100
                    }
                    Ok(Ok(done)) => {
                        if done != hydrus_duplicates::WorkDone::default() {
                            tracing::debug!(?done, "auto-resolution worked");
                        }
                        // the reference rests ten minutes unless woken by
                        // new pairs; checking every minute stands in for that
                        Duration::from_secs(60)
                    }
                    Ok(Err(e)) => {
                        tracing::error!(error = %e, "duplicates auto-resolution failed");
                        Duration::from_secs(600)
                    }
                    Err(_) => Duration::from_secs(600),
                };
                tokio::time::sleep(rest.max(Duration::from_millis(100))).await;
            }
        });
        // import folders, each checked when due
        if let Some(downloads) = state.downloads.clone() {
            tokio::spawn(async move {
                let mut schedule = hydrus_download::folders::ImportFolderSchedule::new();
                loop {
                    let downloader = std::sync::Arc::clone(downloads.downloader());
                    let done = tokio::task::spawn_blocking(move || {
                        let result = hydrus_download::folders::work_due_import_folders(
                            &downloader,
                            &mut schedule,
                        );
                        (schedule, result)
                    })
                    .await;
                    let wait = match done {
                        // (checking at least every minute, to notice folders
                        // changed from the command line)
                        Ok((kept, Ok(seconds))) => {
                            schedule = kept;
                            seconds.clamp(1, 60)
                        }
                        Ok((kept, Err(e))) => {
                            schedule = kept;
                            tracing::error!(error = %e, "import folders failed");
                            1800
                        }
                        Err(_) => {
                            schedule = hydrus_download::folders::ImportFolderSchedule::new();
                            1800
                        }
                    };
                    tokio::time::sleep(Duration::from_secs(wait.unsigned_abs())).await;
                }
            });
        }
        // export folders, each run when due (looked at every three minutes)
        let exporter = store.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            loop {
                let store = exporter.clone();
                let done = tokio::task::spawn_blocking(move || {
                    hydrus_download::export::work_export_folders(&store)
                })
                .await;
                match done {
                    Ok(Ok(runs)) => {
                        for (name, run) in runs {
                            if let Some(e) = &run.error {
                                tracing::error!(folder = %name, error = %e, "export folder failed");
                            } else {
                                tracing::info!(
                                    folder = %name,
                                    exported = run.copied,
                                    removed = run.deleted_paths,
                                    "export folder ran"
                                );
                            }
                        }
                    }
                    Ok(Err(e)) => tracing::error!(error = %e, "export folders failed"),
                    Err(_) => {}
                }
                tokio::time::sleep(Duration::from_secs(180)).await;
            }
        });
        // queues made by other processes (the command line)
        if let Some(downloads) = state.downloads.clone() {
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    if let Err(e) = downloads.start_new() {
                        tracing::error!(error = %e, "looking for new download queues failed");
                    }
                }
            });
        }
        println!("Client API at http://{}", options.addr);
        let net = state
            .downloads
            .as_ref()
            .map(|d| std::sync::Arc::clone(d.downloader().net()));
        let served = serve(state, &options, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
        // (the bandwidth used since the last minute's save)
        if let Some(net) = net
            && let Err(e) = net.save_bandwidth()
        {
            tracing::error!(error = %e, "saving bandwidth usage failed");
        }
        served
    })?;
    Ok(())
}
