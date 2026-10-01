//! `hydrus import-folders`: see import folders, check one now, pause or
//! resume them, or run one here and now.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use clap::Subcommand;

use hydrus_parse::folders::FolderAction;
use hydrus_store::Store;
use hydrus_store::import_folders::{self, ImportFolder};
use hydrus_store::queues::{self, SeedStatus};

#[derive(Subcommand)]
pub enum Action {
    /// Every import folder: its path, schedule and what it has seen.
    List,
    /// Check a folder at the next chance (a running `serve` picks it up
    /// within a minute); this also resumes it.
    CheckNow {
        name: String,
    },
    Pause {
        name: String,
    },
    Resume {
        name: String,
    },
    /// Check a folder and import what it finds, now, in this process.
    Run {
        name: String,
    },
}

fn find(store: &Store, name: &str) -> Result<ImportFolder> {
    store
        .read(|conn| import_folders::find_import_folder(conn, name))?
        .with_context(|| format!("there is no import folder called {name:?}"))
}

fn action_name(action: &FolderAction) -> String {
    match action {
        FolderAction::Ignore => "leave".into(),
        FolderAction::Delete => "delete".into(),
        FolderAction::Move(to) => format!("move to {to}"),
    }
}

fn list(store: &Store) -> Result<()> {
    let folders = store.read(import_folders::import_folders)?;
    if folders.is_empty() {
        println!("no import folders");
    }
    for f in folders {
        let s = &f.settings;
        let counts = store.read(|conn| queues::file_seed_counts(conn, f.id()))?;
        let count = |status: SeedStatus| counts.get(&status).copied().unwrap_or(0);
        let state = if f.paused() {
            "paused".to_owned()
        } else if s.check_regularly {
            format!("every {}s", s.period)
        } else {
            "only when asked".to_owned()
        };
        println!("{} ({state}): {}", f.name(), s.path);
        println!(
            "  new files: {}; already in: {}; deleted: {}; errors: {}",
            action_name(&s.actions.successful_and_new),
            action_name(&s.actions.successful_but_redundant),
            action_name(&s.actions.deleted),
            action_name(&s.actions.error)
        );
        println!(
            "  {} sidecar routers, filename tags for {} services; remembered: {} to do, {} errors, {} vetoed, {} other",
            s.routers.len(),
            s.filename_tagging.len(),
            count(SeedStatus::Unknown),
            count(SeedStatus::Error),
            count(SeedStatus::Vetoed),
            counts.values().sum::<usize>()
                - count(SeedStatus::Unknown)
                - count(SeedStatus::Error)
                - count(SeedStatus::Vetoed)
        );
    }
    Ok(())
}

pub fn run(dir: &Path, action: Action) -> Result<()> {
    let store = Arc::new(Store::open(dir)?);
    match action {
        Action::List => list(&store),
        Action::CheckNow { name } => {
            let mut folder = find(&store, &name)?;
            folder.settings.check_now = true;
            let (id, settings) = (folder.id(), folder.settings);
            store.write(move |ctx| {
                import_folders::set_settings(ctx.conn(), id, &settings)?;
                queues::set_paused(ctx.conn(), id, Some(false), None)
            })?;
            println!("{name:?} will be checked at the next chance");
            Ok(())
        }
        Action::Pause { name } => set_paused(&store, &name, true),
        Action::Resume { name } => set_paused(&store, &name, false),
        Action::Run { name } => {
            let _lock = crate::lock_store(dir, "an import folder run")?;
            let mut folder = find(&store, &name)?;
            if folder.paused() {
                bail!("{name:?} is paused; resume it first");
            }
            folder.settings.check_now = true;
            let (id, settings) = (folder.id(), folder.settings);
            store.write(move |ctx| import_folders::set_settings(ctx.conn(), id, &settings))?;
            let network: hydrus_store::network::NetworkSettings =
                store.read(hydrus_store::settings::get)?;
            let net = Arc::new(hydrus_net::NetEngine::new(
                Arc::clone(&store),
                hydrus_net::NetOptions::from_settings(&network),
            )?);
            let importer = hydrus_import::FileImporter::new(
                Arc::clone(&store),
                hydrus_media::MediaTools::new(),
            );
            let downloader = hydrus_download::Downloader::new(Arc::clone(&store), net, importer)?;
            let run = downloader.work_on_import_folder(id)?;
            for warning in &run.warnings {
                eprintln!("warning: {warning}");
            }
            if let Some(error) = &run.error {
                bail!("{name:?} was paused: {error}");
            }
            println!(
                "{name:?}: {} new files found, {} imported",
                run.new_files, run.imported
            );
            Ok(())
        }
    }
}

fn set_paused(store: &Store, name: &str, paused: bool) -> Result<()> {
    let id = find(store, name)?.id();
    store.write(move |ctx| queues::set_paused(ctx.conn(), id, Some(paused), None))?;
    println!("{name:?} {}", if paused { "paused" } else { "resumed" });
    Ok(())
}

#[derive(Subcommand)]
pub enum ExportAction {
    /// Every export folder: its path, search, naming and schedule.
    List,
    /// Run a folder at the next chance (a running `serve` looks every three
    /// minutes).
    RunNow { name: String },
    /// Run a folder now, in this process, whether or not it is due.
    Run { name: String },
}

pub fn run_export(dir: &Path, action: ExportAction) -> Result<()> {
    use hydrus_store::settings::ExportFolders;
    let store = Store::open(dir)?;
    let mut folders: ExportFolders = store.read(hydrus_store::settings::get)?;
    match action {
        ExportAction::List => {
            if folders.0.is_empty() {
                println!("no export folders");
            }
            for f in &folders.0 {
                let kind = match f.export_type {
                    hydrus_parse::folders::ExportType::Regular => "regular",
                    hydrus_parse::folders::ExportType::Synchronise => "synchronise",
                };
                let schedule = if f.run_regularly {
                    format!("every {}s", f.period)
                } else {
                    "only when asked".into()
                };
                println!("{} ({kind}, {schedule}): {}", f.name, f.path);
                println!(
                    "  named {:?}{}{}; {} sidecar routers; {} predicates",
                    f.phrase,
                    if f.export_symlinks { ", as links" } else { "" },
                    if f.delete_from_client_after_export {
                        ", deleting from the client after"
                    } else {
                        ""
                    },
                    f.routers.len(),
                    f.search.predicates.len()
                );
                if !f.last_error.is_empty() {
                    println!("  last error: {}", f.last_error);
                }
            }
            Ok(())
        }
        ExportAction::RunNow { name } | ExportAction::Run { name }
            if !folders.0.iter().any(|f| f.name == name) =>
        {
            bail!("there is no export folder called {name:?}")
        }
        ExportAction::RunNow { name } => {
            for f in &mut folders.0 {
                if f.name == name {
                    f.run_now = true;
                }
            }
            store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &folders))?;
            println!("{name:?} will run at the next chance");
            Ok(())
        }
        ExportAction::Run { name } => {
            let _lock = crate::lock_store(dir, "an export folder run")?;
            for f in &mut folders.0 {
                if f.name == name {
                    f.run_now = true;
                }
            }
            store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &folders))?;
            let run = hydrus_download::export::work_on_export_folder(&store, &name)?;
            if let Some(error) = &run.error {
                bail!("{name:?} failed (and will no longer run regularly): {error}");
            }
            println!(
                "{name:?}: {} files exported ({} copied or linked), {} old files removed, {} deleted from the client",
                run.exported, run.copied, run.deleted_paths, run.deleted_from_client
            );
            Ok(())
        }
    }
}
