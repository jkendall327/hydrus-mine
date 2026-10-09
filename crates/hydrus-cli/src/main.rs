//! `hydrus`: import a hydrus install, serve the Client API, run maintenance.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

use hydrus_api::AppState;
use hydrus_store::Store;
use hydrus_store::settings::{ClientApiState, ClientApiStatus};
use hydrus_store::store::DB_FILE_NAME;
use hydrus_store::transfer::{TransferMode, transfer_media};

mod api_keys;
mod client_api_listener;
mod duplicates;
mod folder_wait;
mod folders;
mod gallery;
mod pauses;
mod physical_deletes;
mod queues;
mod subscriptions;
mod trash_maintenance;

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
        /// Port to listen on (default: the Client API service's setting;
        /// with none set there, as when hydrus's Client API is off, there is
        /// no Client API unless this is given).
        #[arg(long)]
        port: Option<u16>,
        /// Address to listen on (default: localhost, or every interface if
        /// the Client API service allows non-local connections).
        #[arg(long)]
        bind: Option<IpAddr>,
        /// Stop when standard input closes: as when the program that
        /// started this one (the desktop client) exits, or crashes.
        #[arg(long)]
        attached: bool,
    },
    /// Upkeep of the store's own data.
    Maintenance {
        /// The hydrus-rs store directory.
        dir: PathBuf,
        #[command(subcommand)]
        action: MaintenanceAction,
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

#[derive(Subcommand)]
enum MaintenanceAction {
    /// Drop and rebuild every derived table (autocomplete counts and word
    /// indexes, the notes' search index) from the primary data.
    RebuildCaches,
    /// List the file maintenance jobs waiting (those hydrus had queued come
    /// across), and which this build runs.
    Jobs,
    /// Run the due file maintenance jobs this build runs (checking files'
    /// metadata flags, regenerating their hashes...).
    Files {
        /// At most this many jobs.
        #[arg(long)]
        limit: Option<u64>,
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
        Command::Serve {
            dir,
            port,
            bind,
            attached,
        } => run_server(&dir, port, bind, attached),
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
        Command::Maintenance {
            dir,
            action: MaintenanceAction::RebuildCaches,
        } => {
            let _lock = lock_store(&dir, "rebuilding the caches")?;
            let store = Store::open(&dir)?;
            let started = std::time::Instant::now();
            store.write(|ctx| hydrus_store::maintenance::rebuild_caches(ctx.conn()))?;
            println!("rebuilt the caches in {:.1?}", started.elapsed());
            Ok(())
        }
        Command::Maintenance {
            dir,
            action: MaintenanceAction::Jobs,
        } => {
            use hydrus_store::file_maintenance::job_counts;
            let store = Store::open(&dir)?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
            let counts = store.read(|conn| job_counts(conn, now))?;
            if counts.is_empty() {
                println!("no file maintenance jobs are queued");
            }
            for (job, (due, waiting)) in counts {
                println!("{}: {due} due, {waiting} waiting", job.description());
            }
            Ok(())
        }
        Command::Maintenance {
            dir,
            action: MaintenanceAction::Files { limit },
        } => {
            let _lock = lock_store(&dir, "file maintenance")?;
            let store = Store::open(&dir)?;
            let importer = hydrus_import::FileImporter::new(
                std::sync::Arc::clone(&store),
                hydrus_media::MediaTools::new(),
            );
            let started = std::time::Instant::now();
            let report = importer.run_file_maintenance(limit.unwrap_or(u64::MAX), u64::MAX)?;
            for (job, n) in &report.done {
                println!("{}: {n}", job.description());
            }
            println!("{} jobs done in {:.1?}", report.total(), started.elapsed());
            if report.bad_files > 0 {
                println!(
                    "{} files were missing or damaged: see {}",
                    report.bad_files,
                    dir.join(hydrus_import::maintenance::ERROR_DIR_NAME)
                        .display()
                );
            }
            if !report.redownload.is_empty() {
                // (queued for `hydrus serve` to download)
                let network: hydrus_store::network::NetworkSettings =
                    store.read(hydrus_store::settings::get)?;
                let net = std::sync::Arc::new(hydrus_net::NetEngine::new(
                    std::sync::Arc::clone(&store),
                    hydrus_net::NetOptions::from_settings(&network),
                )?);
                let downloader =
                    hydrus_download::Downloader::new(std::sync::Arc::clone(&store), net, importer)?;
                let runner = hydrus_download::QueueRunner::new(
                    std::sync::Arc::new(downloader),
                    network.downloader_network_error_delay,
                );
                let added = runner.redownload(&report.redownload)?;
                println!(
                    "{added} URLs queued in \"{}\" to download them again",
                    hydrus_import::maintenance::REDOWNLOAD_PAGE_NAME
                );
            }
            Ok(())
        }
        Command::Purge { dir } => {
            let _lock = lock_store(&dir, "a purge")?;
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
        println!(
            "  using the files in {}",
            transfer.destinations[0].display()
        );
    } else {
        println!(
            "  {} files, {:.1} GiB, into {}",
            transfer.files,
            transfer.bytes as f64 / f64::from(1u32 << 30),
            transfer.destinations[0].display()
        );
        for other in &transfer.destinations[1..] {
            println!(
                "  and, for the media on another drive, into {}",
                other.display()
            );
        }
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

fn run_server(dir: &Path, port: Option<u16>, bind: Option<IpAddr>, attached: bool) -> Result<()> {
    if !dir.join(DB_FILE_NAME).exists() {
        bail!(
            "{} is not a hydrus-rs store (no {DB_FILE_NAME})",
            dir.display()
        );
    }
    let _lock = lock_store(dir, "a second hydrus serve")?;
    let store = Store::open(dir)?;
    let snap = store.snapshot();
    let holds_files = store.read(|conn| {
        let storage = hydrus_store::content::DomainRoles::new(&snap.services)?.local_file_storage;
        Ok(conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM file_domain_current WHERE service_id = ?1)",
            [storage],
            |r| r.get::<_, bool>(0),
        )?)
    })?;
    let missing = snap.storage.missing_locations(holds_files);
    if !missing.is_empty() {
        let list: Vec<String> = missing
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect();
        bail!(
            "these media locations are missing (is a drive not mounted?):\n{}\n\
             hydrus serve won't start without them, so new files aren't written where they'd be hidden",
            list.join("\n")
        );
    }
    // what the Client API is doing, for the desktop client to show
    let say = {
        let store = store.clone();
        let pid = std::process::id();
        move |state: ClientApiState| {
            let status = ClientApiStatus { pid, state };
            if let Err(e) = store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &status))
            {
                tracing::error!(error = %e, "keeping the Client API's status failed");
            }
        }
    };
    say(ClientApiState::Starting);
    // An attached daemon belongs to an already-booted GUI; restarting it
    // must not undo a live Resume. Standalone serve is its own client boot.
    if !attached {
        hydrus_store::settings::apply_network_boot_pause(&store)?;
    }
    let state = AppState::new(store.clone())?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        // An owned worker wakes between-pair waits on signal or early exit.
        let physical_deletes = physical_deletes::Worker::start(store.clone());
        // leftovers in the scratch folder: at the start, then hourly
        let scratch = store.clone();
        tokio::spawn(async move {
            loop {
                let store = scratch.clone();
                if let Ok(n) = tokio::task::spawn_blocking(move || sweep_scratch(&store)).await
                    && n > 0
                {
                    tracing::info!(files = n, "cleared old temporary files");
                }
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });
        // Both automatic workers are normal-time consumers without a GUI monitor.
        let trash_maintenance = trash_maintenance::Worker::start(store.clone());
        // (what an earlier daemon's queues were doing is over, and so is the
        // work its popups showed)
        if let Err(e) = store.write(|ctx| hydrus_store::live::clear(ctx.conn())) {
            tracing::error!(error = %e, "clearing the queues' live state failed");
        }
        forget_unfinished_popups(&store);
        if let Some(downloads) = &state.downloads
            && let Err(e) = downloads.start_all()
        {
            tracing::error!(error = %e, "starting the download queues failed");
        }
        if let Some(subscriptions) = &state.subscriptions {
            subscriptions.start();
        }
        // the similar-files search, finding potential duplicates as files come
        // in: packets of work and rests, by the GUI's idle state
        let searcher = store.clone();
        tokio::spawn(async move {
            loop {
                let wait = similar_files_step(&searcher).await;
                tokio::time::sleep(wait).await;
            }
        });
        // file maintenance, as the reference's manager does while active (a
        // server is never idle): at most its throttle's worth of work a
        // window, starting a minute in
        let maintainer = store.clone();
        let redownloader = state.downloads.clone();
        tokio::spawn(async move {
            use hydrus_store::file_maintenance::FileMaintenanceSettings;
            tokio::time::sleep(Duration::from_secs(60)).await;
            let importer = std::sync::Arc::new(hydrus_import::FileImporter::new(
                maintainer.clone(),
                hydrus_media::MediaTools::new(),
            ));
            loop {
                let settings: FileMaintenanceSettings = maintainer
                    .read(hydrus_store::settings::get)
                    .unwrap_or_default();
                let idle = hydrus_store::idle_state::is_idle(
                    maintainer.dir(),
                    hydrus_core::time::TimestampMs::now().millis(),
                );
                let (allowed, files, seconds) = settings.allowance(idle);
                if !allowed {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    continue;
                }
                let window = Duration::from_secs(seconds.max(1));
                let budget = files.saturating_mul(100);
                let started = std::time::Instant::now();
                let worker = importer.clone();
                let redownloader = redownloader.clone();
                let done = tokio::task::spawn_blocking(move || {
                    let report = worker.run_file_maintenance(u64::MAX, budget)?;
                    if !report.redownload.is_empty() {
                        if let Some(runner) = &redownloader {
                            runner.redownload(&report.redownload)?;
                        } else {
                            tracing::error!(
                                urls = ?report.redownload,
                                "missing files could be downloaded again, but the downloader is not running"
                            );
                        }
                    }
                    Ok::<_, anyhow::Error>(report)
                })
                .await;
                match done {
                    Ok(Ok(report)) if report.total() > 0 => {
                        tracing::debug!(jobs = report.total(), "file maintenance");
                        tokio::time::sleep(window.saturating_sub(started.elapsed())).await;
                    }
                    Ok(Err(e)) => {
                        tracing::error!(error = %e, "file maintenance failed");
                        tokio::time::sleep(Duration::from_secs(600)).await;
                    }
                    _ => tokio::time::sleep(Duration::from_secs(60)).await,
                }
            }
        });
        // duplicates auto-resolution: bursts of work with rests between, as
        // the reference's manager does, in idle or normal time by the GUI's
        // published idle state (a daemon with no GUI is never idle)
        let resolver = store.clone();
        tokio::spawn(async move {
            loop {
                let wait = auto_resolution_step(&resolver, |store, budget| {
                    hydrus_duplicates::work_rules(
                        &store,
                        budget,
                        &mut hydrus_duplicates::Shuffle,
                        &hydrus_search::Clock::system(),
                    )
                })
                .await;
                tokio::time::sleep(wait).await;
            }
        });
        // import folders, each checked when due
        if let Some(downloads) = state.downloads.clone() {
            tokio::spawn(async move {
                let mut schedule = hydrus_download::folders::ImportFolderSchedule::new();
                loop {
                    let before_work = folder_wait::capture(
                        downloads.downloader().store(),
                        hydrus_store::folder_activity::Kind::Import,
                    );
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
                    folder_wait::wait(
                        downloads.downloader().store(),
                        hydrus_store::folder_activity::Kind::Import,
                        before_work,
                        Duration::from_secs(wait.unsigned_abs()),
                    ).await;
                }
            });
        }
        // export folders, each run when due (looked at every three minutes)
        let exporter = store.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            loop {
                let before_work = folder_wait::capture(
                    &exporter,
                    hydrus_store::folder_activity::Kind::Export,
                );
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
                folder_wait::wait(
                    &exporter,
                    hydrus_store::folder_activity::Kind::Export,
                    before_work,
                    Duration::from_secs(180),
                ).await;
            }
        });
        // queues another process (the desktop client) made or changed, as
        // it nudges them: looked at every second
        if let Some(downloads) = state.downloads.clone() {
            let subscriptions = state.subscriptions.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    if let Err(e) = downloads.take_nudges(subscriptions.as_ref()) {
                        tracing::error!(error = %e, "acting on nudged queues failed");
                    }
                }
            });
        }
        // The full network engine (including subscriptions), independent of
        // whether the optional Client API listener is enabled.
        let (network_stop, mut network_stopped) = tokio::sync::watch::channel(false);
        let network_publisher = state.downloads.clone().map(|downloads| {
            let net = downloads.downloader().net().clone();
            tokio::spawn(async move {
                loop {
                    let net = net.clone();
                    match tokio::task::spawn_blocking(move || net.publish_runtime()).await {
                        Ok(Err(e)) => tracing::warn!(error = %e, "publishing network runtime failed"),
                        Err(e) => tracing::warn!(error = %e, "network runtime worker failed"),
                        Ok(Ok(())) => {}
                    }
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_millis(250)) => {},
                        _ = network_stopped.changed() => break,
                    }
                }
            })
        });
        // what the queues are doing, for their pages: what changed, four
        // times a second (until the daemon stops, when it is cleared)
        let publisher = state.downloads.clone().map(|downloads| {
            let store = store.clone();
            tokio::spawn(async move {
                let mut last = std::collections::HashMap::new();
                // (and the network's use, for the client's status bar: as it
                // changes, and every few seconds to say it is still so)
                let mut said: Option<hydrus_store::live::DaemonLive> = None;
                loop {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    let (started, bytes, speed) = downloads.downloader().net().session_usage();
                    let at = hydrus_core::time::TimestampMs::now().millis() / 1000;
                    let queues = downloads.live();
                    let usage = hydrus_store::live::DaemonLive {
                        started,
                        bytes,
                        speed,
                        at,
                        jobs: queues
                            .iter()
                            .map(|(_, live)| live.jobs_in_flight())
                            .sum(),
                    };
                    let stale = said.is_none_or(|s| {
                        (s.started, s.bytes, s.speed, s.jobs)
                            != (started, bytes, speed, usage.jobs)
                            || at - s.at >= 5
                    });
                    if stale {
                        said = Some(usage);
                        if let Err(e) =
                            store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &usage))
                        {
                            tracing::error!(error = %e, "keeping the network's use failed");
                        }
                    }
                    let changes = hydrus_store::live::changes(&mut last, queues);
                    if changes.is_empty() {
                        continue;
                    }
                    if let Err(e) =
                        store.write(move |ctx| hydrus_store::live::publish(ctx.conn(), &changes))
                    {
                        tracing::error!(error = %e, "keeping the queues' live state failed");
                    }
                }
            })
        });
        // the network and downloader options, as they are changed (by the
        // client's options window, say): the reference reads its options as
        // it goes, so changes apply without a restart
        if let Some(downloads) = state.downloads.clone() {
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    match downloads.reload_settings() {
                        Ok(true) => tracing::info!("the network options changed"),
                        Ok(false) => {}
                        Err(e) => {
                            tracing::error!(error = %e, "reading the network options failed");
                        }
                    }
                }
            });
        }
        // The desktop client has its own Store. Its service, relation and URL
        // class editors publish a revision so this process reloads too.
        {
            let store = store.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    match store.refresh_if_changed() {
                        Ok(true) => tracing::info!("the store snapshot changed"),
                        Ok(false) => {}
                        Err(e) => tracing::error!(error = %e, "refreshing the snapshot failed"),
                    }
                }
            });
        }
        // queues made by other processes that don't nudge (older ones)
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
        let net = state
            .downloads
            .as_ref()
            .map(|d| std::sync::Arc::clone(d.downloader().net()));
        // noticing the computer waking from sleep (requests then wait a
        // little for its network)
        if let Some(net) = net.clone() {
            tokio::spawn(async move {
                loop {
                    net.sleep_check();
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            });
        }
        // (file paths wait after a wake too, when the options say so)
        {
            let store = store.clone();
            tokio::spawn(async move {
                loop {
                    store.sleep_check();
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            });
        }
        // stopping: on a signal, or (attached) with the input closing
        let (stop, stopped) = tokio::sync::watch::channel(false);
        let stopping_subscriptions = state.subscriptions.clone();
        let stopping_physical_deletes = physical_deletes.control();
        let stopping_trash_maintenance = trash_maintenance.control();
        tokio::spawn(async move {
            if attached {
                tokio::select! {
                    () = shutdown_signal() => {}
                    () = input_closed() => {}
                }
            } else {
                shutdown_signal().await;
            }
            println!("stopping");
            stopping_physical_deletes.cancel();
            stopping_trash_maintenance.cancel();
            if let Some(subscriptions) = &stopping_subscriptions {
                subscriptions.shutdown();
            }
            let _ = stop.send(true);
        });
        // the Client API, if it is on: one that can't start stops nothing
        // else, as in the reference
        let subscriptions = state.subscriptions.clone();
        let served = client_api_listener::run(state, port, bind, stopped, say).await;
        physical_deletes.shutdown().await;
        trash_maintenance.shutdown().await;
        if let Some(subscriptions) = &subscriptions {
            subscriptions.shutdown();
            subscriptions.wait_stopped().await;
        }
        // (the queues' live state stops being kept before it is cleared,
        // so it isn't kept again after)
        let _ = network_stop.send(true);
        if let Some(publisher) = network_publisher {
            let _ = publisher.await;
        }
        if let Err(e) = store.write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::network_runtime::Snapshot::default(),
            )
        }) {
            tracing::warn!(error = %e, "clearing network runtime failed");
        }
        if let Some(publisher) = publisher {
            publisher.abort();
            let _ = publisher.await;
        }
        if let Err(e) = store.write(|ctx| hydrus_store::live::clear(ctx.conn())) {
            tracing::error!(error = %e, "clearing the queues' live state failed");
        }
        forget_unfinished_popups(&store);
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

/// Until the daemon is told to stop.
async fn until(mut stopped: tokio::sync::watch::Receiver<bool>) {
    let _ = stopped.wait_for(|stopped| *stopped).await;
}

/// Ctrl-C, or (on Unix) the SIGTERM a service manager stops a program with.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

/// Standard input closing (`serve --attached`): the program that started
/// this one has exited, or crashed.
async fn input_closed() {
    let (closed, on_close) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        let mut buffer = [0; 256];
        loop {
            match std::io::Read::read(&mut input, &mut buffer) {
                Ok(0) => break,
                Err(e) if e.kind() != std::io::ErrorKind::Interrupted => break,
                _ => {}
            }
        }
        let _ = closed.send(());
    });
    let _ = on_close.await;
}

/// Delete what an import or download left in the store's scratch folder
/// when it was killed: anything there over an hour old (anything newer
/// may be in use).
fn sweep_scratch(store: &Store) -> usize {
    let Ok(entries) = std::fs::read_dir(store.dir().join("tmp")) else {
        return 0;
    };
    let hour = Duration::from_secs(3600);
    let mut swept = 0;
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|age| age > hour));
        if old && hydrus_store::paths::delete_path(entry.path()).is_ok() {
            swept += 1;
        }
    }
    swept
}

/// Take the lock a running `hydrus serve` holds on its store, for serving
/// or for work it does itself (`what`), which two processes mustn't do at
/// once: its "is this file being imported" claims only reach its own
/// purges. Held until the returned file is dropped.
pub(crate) fn lock_store(dir: &Path, what: &str) -> Result<std::fs::File> {
    match hydrus_store::store::lock_serving(dir)
        .with_context(|| format!("opening the lock file in {}", dir.display()))?
    {
        Some(file) => Ok(file),
        None => bail!(
            "hydrus serve is running on {}, so {what} can't run alongside it; stop it first",
            dir.display()
        ),
    }
}

/// Forget the popups of work that stopped with a daemon.
fn forget_unfinished_popups(store: &hydrus_store::Store) {
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    if let Err(e) = store.write(move |ctx| hydrus_store::popups::forget_unfinished(ctx.conn(), now))
    {
        tracing::error!(error = %e, "forgetting unfinished popups failed");
    }
}

/// One pass of the similar-files search loop: work for the packet its idle or
/// normal-time switch allows, or hold (nothing is searched) when that switch is
/// off. Returns how long to wait before the next pass.
async fn similar_files_step(store: &std::sync::Arc<Store>) -> Duration {
    use hydrus_store::idle_state::is_idle;
    use hydrus_store::similar::SimilarFilesSettings;
    let store = store.clone();
    let settings: SimilarFilesSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let idle = is_idle(store.dir(), hydrus_core::time::TimestampMs::now().millis());
    let mut pace = settings.pace(idle);
    pace.allowed |= settings.work_hard;
    if !pace.allowed {
        return Duration::from_secs(10);
    }
    let started = std::time::Instant::now();
    let work = pace.work;
    let done = tokio::task::spawn_blocking(move || {
        let mut total = 0;
        loop {
            let n = hydrus_store::similar::run_search(&store, 16)?;
            total += n;
            if n == 0 || started.elapsed() >= work {
                return Ok::<_, hydrus_store::StoreError>((total, n > 0));
            }
        }
    })
    .await;
    match done {
        Ok(Ok((n, more))) if n > 0 => {
            tracing::debug!(files = n, "searched for similar files");
            let rest = if more {
                pace.rest(started.elapsed())
            } else {
                Duration::from_secs(30)
            };
            rest.max(Duration::from_millis(100))
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, "the similar-files search failed");
            Duration::from_secs(600)
        }
        _ => Duration::from_secs(30),
    }
}

/// One pass of the auto-resolution loop: `work` the rules for the packet the
/// idle or normal-time switch allows (given the GUI's published idle state),
/// or hold when that switch is off. Returns how long to wait before the next.
async fn auto_resolution_step<W>(store: &std::sync::Arc<Store>, work: W) -> Duration
where
    W: FnOnce(
            std::sync::Arc<Store>,
            Duration,
        ) -> Result<hydrus_duplicates::WorkDone, hydrus_store::StoreError>
        + Send
        + 'static,
{
    let store = store.clone();
    let settings: hydrus_store::duplicates::auto::AutoResolutionSettings =
        match store.read(hydrus_store::settings::get) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "reading the auto-resolution settings failed");
                return Duration::from_secs(600);
            }
        };
    let pace = settings.pace(hydrus_store::idle_state::is_idle(
        store.dir(),
        hydrus_core::time::TimestampMs::now().millis(),
    ));
    if !pace.allowed {
        return Duration::from_secs(10);
    }
    let budget = pace.work;
    let started = std::time::Instant::now();
    let done = tokio::task::spawn_blocking(move || work(store, budget)).await;
    let rest = match done {
        Ok(Ok(done)) if done.more_to_do => {
            tracing::debug!(?done, "auto-resolution worked");
            pace.rest(started.elapsed())
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
    rest.max(Duration::from_millis(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_process_at_a_time_does_what_serve_does() {
        let dir = tempfile::tempdir().unwrap();
        let held = lock_store(dir.path(), "serving").unwrap();
        let refused = lock_store(dir.path(), "a purge").unwrap_err().to_string();
        assert!(refused.contains("hydrus serve is running"), "{refused}");
        assert!(refused.contains("a purge"), "{refused}");
        drop(held);
        lock_store(dir.path(), "a purge").unwrap();
    }

    /// The idle state the GUI would have published just now.
    fn gui_is_idle(store: &Store, idle: bool) {
        hydrus_store::idle_state::publish(
            store.dir(),
            idle,
            hydrus_core::time::TimestampMs::now().millis(),
        )
        .unwrap();
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    // leaf: audit-media-preparation-scheduling
    #[test]
    fn the_similar_files_loop_holds_and_works_by_the_published_idle_state() {
        use hydrus_store::similar::{SimilarFilesSettings, search_status_counts};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        // one file waiting to be searched
        store
            .write(|ctx| {
                ctx.conn().execute(
                    "INSERT INTO hashes (hash_id, sha256) VALUES (1, zeroblob(32))",
                    [],
                )?;
                ctx.conn().execute(
                    "INSERT INTO similar_search_status (hash_id, searched_distance) VALUES (1, NULL)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        let waiting = |store: &Store| {
            store
                .read(search_status_counts)
                .unwrap()
                .get(&None)
                .copied()
                .unwrap_or(0)
        };
        let set = |idle_switch: bool, normal_switch: bool| {
            store
                .write(move |ctx| {
                    hydrus_store::settings::set(
                        ctx.conn(),
                        &SimilarFilesSettings {
                            during_idle: idle_switch,
                            during_active: normal_switch,
                            ..SimilarFilesSettings::default()
                        },
                    )
                })
                .unwrap();
        };

        // idle time only: normal use holds (and the next pass is soon)
        set(true, false);
        gui_is_idle(&store, false);
        assert_eq!(
            block_on(similar_files_step(&store)),
            Duration::from_secs(10)
        );
        assert_eq!(waiting(&store), 1, "held: nothing was searched");
        // a daemon with no GUI is never idle either
        std::fs::remove_file(dir.path().join("client_idle_state")).unwrap();
        assert_eq!(
            block_on(similar_files_step(&store)),
            Duration::from_secs(10)
        );
        assert_eq!(waiting(&store), 1);
        // the GUI goes idle: it works
        gui_is_idle(&store, true);
        block_on(similar_files_step(&store));
        assert_eq!(waiting(&store), 0, "idle time: searched");

        // normal time only: idle holds, normal use works
        store
            .write(|ctx| {
                ctx.conn().execute(
                    "UPDATE similar_search_status SET searched_distance = NULL",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        set(false, true);
        gui_is_idle(&store, true);
        assert_eq!(
            block_on(similar_files_step(&store)),
            Duration::from_secs(10)
        );
        assert_eq!(waiting(&store), 1, "held: idle, and only normal time is on");
        gui_is_idle(&store, false);
        block_on(similar_files_step(&store));
        assert_eq!(waiting(&store), 0, "normal time: searched");
    }

    // leaf: audit-media-rule-sidebar-scheduling
    #[test]
    fn the_auto_resolution_loop_holds_and_works_by_the_published_idle_state() {
        use hydrus_store::duplicates::auto::AutoResolutionSettings;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let budgets = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let step = |store: &std::sync::Arc<Store>| {
            let budgets = budgets.clone();
            block_on(auto_resolution_step(store, move |_, budget| {
                budgets.lock().unwrap().push(budget);
                Ok(hydrus_duplicates::WorkDone::default())
            }))
        };
        let set = |idle_switch: bool, normal_switch: bool| {
            store
                .write(move |ctx| {
                    hydrus_store::settings::set(
                        ctx.conn(),
                        &AutoResolutionSettings {
                            during_idle: idle_switch,
                            during_active: normal_switch,
                            work_time_ms_active: 100,
                            work_time_ms_idle: 1000,
                            ..AutoResolutionSettings::default()
                        },
                    )
                })
                .unwrap();
        };

        // idle time only: normal use holds, and a daemon with no GUI is
        // never idle
        set(true, false);
        gui_is_idle(&store, false);
        assert_eq!(step(&store), Duration::from_secs(10));
        std::fs::remove_file(dir.path().join("client_idle_state")).unwrap();
        assert_eq!(step(&store), Duration::from_secs(10));
        assert!(budgets.lock().unwrap().is_empty(), "held: no rule worked");
        // idle: works, for the idle packet
        gui_is_idle(&store, true);
        assert_eq!(step(&store), Duration::from_secs(60));
        assert_eq!(*budgets.lock().unwrap(), [Duration::from_millis(1000)]);

        // normal time only: idle holds, normal use works, for its packet
        set(false, true);
        gui_is_idle(&store, true);
        assert_eq!(step(&store), Duration::from_secs(10));
        assert_eq!(budgets.lock().unwrap().len(), 1, "held while idle");
        gui_is_idle(&store, false);
        step(&store);
        assert_eq!(
            *budgets.lock().unwrap(),
            [Duration::from_millis(1000), Duration::from_millis(100)]
        );
    }
}
