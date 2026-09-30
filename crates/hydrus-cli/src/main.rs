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

mod gallery;
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
    /// Delete from disk the files that were deleted from local storage.
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
    }
    println!(
        "done. run `hydrus serve {}` to start the Client API.",
        dest.display()
    );
    Ok(())
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
        if let Some(downloads) = &state.downloads
            && let Err(e) = downloads.start_all()
        {
            tracing::error!(error = %e, "starting the download queues failed");
        }
        if let Some(subscriptions) = &state.subscriptions {
            subscriptions.start();
        }
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
        serve(state, &options, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
    })?;
    Ok(())
}
