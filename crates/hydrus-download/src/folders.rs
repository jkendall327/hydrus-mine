//! Import folders at work (`ImportFolder.DoWork`): check a folder for new
//! files, import them with their sidecars and filename tags, then delete,
//! move or leave each as the folder says.

use std::collections::{BTreeSet, HashMap};

use hydrus_core::content::{ContentStatus, TimestampType};
use hydrus_core::import_options::{CallerType, FullImportOptions, NoteImportOptions};
use hydrus_core::{CanvasType, HashId, ServiceKey, ServiceType, Sha256, Tag};
use hydrus_import::paths;
use hydrus_parse::folders::FolderAction;
use hydrus_parse::sidecar::{
    self, Exporter, MediaMetadata, SidecarError, Source, TagDisplay, TimestampLocation,
    TimestampStub,
};
use hydrus_store::content::FileTime;
use hydrus_store::content::MappingAction;
use hydrus_store::import_folders::{self, ImportFolder};
use hydrus_store::queues::{self, FileSeed, NewFileSeed, SeedStatus, SeedType};
use hydrus_store::settings::FolderSettings;
use hydrus_store::{Store, StoreError, master};

use crate::seeds::{Stop, set_status};
use crate::{Downloader, WorkError, now};

/// A file's own metadata in the store, for routers' media ends.
pub(crate) struct StoreMedia<'a> {
    pub(crate) store: &'a Store,
    pub(crate) hash_id: HashId,
    pub(crate) now: i64,
}

fn media_error(e: impl std::fmt::Display) -> SidecarError {
    SidecarError::Media(e.to_string())
}

impl StoreMedia<'_> {
    fn service(&self, key_hex: &str) -> Option<std::sync::Arc<hydrus_store::services::Service>> {
        let key = ServiceKey::from_hex(key_hex).ok()?;
        self.store.snapshot().services.by_key(&key).ok().cloned()
    }

    fn file_time(&self, stub: &TimestampStub) -> Result<Option<FileTime>, SidecarError> {
        let service = |key: &str| -> Result<Option<hydrus_core::ServiceId>, SidecarError> {
            Ok(self.service(key).map(|s| s.id))
        };
        Ok(match (stub.kind, &stub.location) {
            (TimestampType::ModifiedDomain, TimestampLocation::Domain(d)) => {
                Some(FileTime::DomainModified(d.clone()))
            }
            (TimestampType::ModifiedFile, _) => Some(FileTime::FileModified),
            (TimestampType::Imported, TimestampLocation::Service(k)) => {
                service(k)?.map(FileTime::Imported)
            }
            (TimestampType::Deleted, TimestampLocation::Service(k)) => {
                service(k)?.map(FileTime::Deleted)
            }
            (TimestampType::PreviouslyImported, TimestampLocation::Service(k)) => {
                service(k)?.map(FileTime::PreviouslyImported)
            }
            (TimestampType::Archived, _) => Some(FileTime::Archived),
            (TimestampType::LastViewed, TimestampLocation::Canvas(c)) => u8::try_from(*c)
                .ok()
                .and_then(CanvasType::from_code)
                .map(FileTime::LastViewed),
            _ => None,
        })
    }

    fn media_result(&self) -> Result<hydrus_store::media::MediaResult, SidecarError> {
        let snapshot = self.store.snapshot();
        let batch = self
            .store
            .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, &[self.hash_id]))
            .map_err(media_error)?;
        batch
            .results
            .into_iter()
            .next()
            .ok_or_else(|| SidecarError::Media("the file is not in the database".into()))
    }

    /// `TagsManager.GetCurrent(service, display)`, as strings.
    fn current_tags(
        &self,
        key_hex: &str,
        display: TagDisplay,
    ) -> Result<Vec<String>, SidecarError> {
        let snapshot = self.store.snapshot();
        let Some(wanted) = self.service(key_hex) else {
            return Ok(Vec::new());
        };
        let m = self.media_result()?;
        let combined = wanted.service_type() == ServiceType::CombinedTag;
        let mut ids = BTreeSet::new();
        for service in snapshot.services.tag_services() {
            if !combined && service.id != wanted.id {
                continue;
            }
            let Some(tags) = m.tags.get(&service.id) else {
                continue;
            };
            let Some(current) = tags.by_status.get(&ContentStatus::Current) else {
                continue;
            };
            match display {
                TagDisplay::Storage => ids.extend(current.iter().copied()),
                TagDisplay::DisplayActual => {
                    let graph = snapshot.display.get(service.id);
                    ids.extend(current.iter().flat_map(|t| graph.display_tags(*t)));
                }
            }
        }
        let ids: Vec<_> = ids.into_iter().collect();
        let names = self
            .store
            .read(|conn| master::tags(conn, &ids))
            .map_err(media_error)?;
        let mut tags: Vec<String> = Vec::new();
        for tag in names.values() {
            let tag = sidecar::undouble_leading_colon(tag.as_str());
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        hydrus_core::sort::human_sort(&mut tags);
        Ok(tags)
    }
}

/// Python's `int(text)` for decimal text.
fn py_int(text: &str) -> Option<i64> {
    let t = text.trim();
    let (negative, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    let value: i64 = digits.replace('_', "").parse().ok()?;
    Some(if negative { -value } else { value })
}

impl MediaMetadata for StoreMedia<'_> {
    fn import(&mut self, source: &Source) -> Result<Vec<String>, SidecarError> {
        match source {
            Source::MediaTags {
                service_key,
                display,
            } => self.current_tags(service_key, *display),
            Source::MediaUrls => {
                let mut urls = self.media_result()?.urls;
                urls.sort();
                Ok(urls)
            }
            Source::MediaNotes => {
                let notes = self.media_result()?.notes;
                Ok(sidecar::notes_to_rows(
                    notes.iter().map(|(n, t)| (n.as_str(), t.as_str())),
                ))
            }
            Source::MediaTimestamp(stub) => {
                let m = self.media_result()?;
                if stub.kind == TimestampType::Archived && m.inbox {
                    return Ok(Vec::new());
                }
                let ms: Option<i64> = if stub.kind == TimestampType::ModifiedAggregate {
                    m.aggregate_modified().map(|t| t.0)
                } else {
                    match self.file_time(stub)? {
                        None => None,
                        Some(time) => {
                            let id = self.hash_id;
                            self.store
                                .write_content(move |w| w.file_time(id, &time))
                                .map_err(media_error)?
                        }
                    }
                };
                Ok(ms
                    .map(|ms| ms.div_euclid(1000).to_string())
                    .into_iter()
                    .collect())
            }
            Source::Txt { .. } | Source::Json { .. } => {
                unreachable!("sidecar sources are read by the router")
            }
        }
    }

    fn export(&mut self, exporter: &Exporter, rows: &[String]) -> Result<(), SidecarError> {
        let id = self.hash_id;
        match exporter {
            Exporter::MediaTags { service_key } => {
                let Some(service) = self.service(service_key) else {
                    return Err(SidecarError::Media(format!(
                        "the tag service {service_key} does not exist"
                    )));
                };
                let pend = service.service_type() != ServiceType::LocalTag;
                let tags: BTreeSet<String> = rows
                    .iter()
                    .filter_map(|r| hydrus_core::tag::clean_tag_checked(r))
                    .collect();
                if tags.is_empty() {
                    return Ok(());
                }
                let service = service.id;
                self.store
                    .write_content(move |w| {
                        let action = if pend {
                            MappingAction::Pend
                        } else {
                            MappingAction::Add
                        };
                        for tag in &tags {
                            let Some(tag) = Tag::new(tag) else { continue };
                            let tag_id = master::intern_tag(w.conn(), &tag)?;
                            w.update_mappings(service, &action, tag_id, &[id])?;
                        }
                        Ok(())
                    })
                    .map_err(media_error)
            }
            Exporter::MediaUrls => {
                let snapshot = self.store.snapshot();
                let classes = &snapshot.url_classes;
                // (anything that normalises is added, URL or not, as in the
                // reference)
                let urls: Vec<String> = rows
                    .iter()
                    .filter_map(|r| {
                        let encoded = hydrus_core::url::functions::ensure_url_is_encoded(
                            r,
                            true,
                            classes.settings().collapse_leading_slashes,
                        );
                        classes.normalise(&encoded, false).ok()
                    })
                    .collect();
                self.store
                    .write_content(move |w| w.add_urls(&[id], &urls))
                    .map_err(media_error)
            }
            Exporter::MediaNotes { forced_name } => {
                let incoming = sidecar::rows_to_notes(rows, forced_name.as_deref());
                self.store
                    .write_content(move |w| {
                        let existing = w.notes(id)?;
                        let updates = NoteImportOptions::default().updates(&existing, &incoming);
                        for (name, note) in &updates {
                            w.set_note(id, name, note)?;
                        }
                        Ok(())
                    })
                    .map_err(media_error)
            }
            Exporter::MediaTimestamp(stub) => {
                let Some(seconds) = rows.first().and_then(|r| py_int(r)) else {
                    return Ok(());
                };
                if seconds > self.now {
                    return Ok(());
                }
                let Some(time) = self.file_time(stub)? else {
                    return Ok(());
                };
                self.store
                    .write_content(move |w| w.set_file_time(&[id], &time, seconds * 1000))
                    .map_err(media_error)
            }
            Exporter::Txt { .. } | Exporter::Json { .. } => {
                unreachable!("sidecar exporters are written by the router")
            }
        }
    }
}

/// What a run of a folder did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FolderRun {
    pub checked: bool,
    pub new_files: usize,
    pub imported: usize,
    /// Set if the folder hit a problem and was paused.
    pub error: Option<String>,
    /// Problems that didn't stop it (metadata routing, filename tags).
    pub warnings: Vec<String>,
}

fn seed_hash(seed: &FileSeed) -> Option<Sha256> {
    seed.meta
        .hash("sha256")
        .and_then(|h| hex::decode(h).ok())
        .and_then(|b| Sha256::from_slice(&b).ok())
}

impl Downloader {
    /// `FileSeed.ImportPath`: import a path seed and write what it carries.
    pub fn import_path_seed(
        &self,
        seed: &mut FileSeed,
        options: &FullImportOptions,
        copy_to_temp: bool,
    ) -> Result<(), WorkError> {
        let path = std::path::PathBuf::from(&seed.data);
        if seed.seed_type != SeedType::Path {
            set_status(
                seed,
                SeedStatus::Vetoed,
                "Attempted to import as a path, but I do not think I am a path!".into(),
            );
            return Ok(());
        }
        if !path.exists() {
            set_status(
                seed,
                SeedStatus::Vetoed,
                "Source file does not exist!".into(),
            );
            return Ok(());
        }
        // (the importer always works from a copy, as the reference does
        // with copy_import_files_to_temp_dir, its default)
        let _ = copy_to_temp;
        let result = self.import_file(seed, &path, options);
        match result {
            Ok(()) => {}
            Err(Stop::Veto(note)) => {
                set_status(seed, SeedStatus::Vetoed, note);
                return Ok(());
            }
            Err(Stop::Error(note)) => {
                set_status(seed, SeedStatus::Error, note);
                return Ok(());
            }
            Err(Stop::Failed(e)) => {
                set_status(seed, SeedStatus::Error, e.to_string());
                return Ok(());
            }
        }
        if let Err(e) = self.write_content_updates(seed, options) {
            set_status(seed, SeedStatus::Error, e.to_string());
        }
        Ok(())
    }
}

/// `_CheckFolder`: add the folder's new, settled, free files as seeds.
fn check_folder(store: &Store, folder: &mut ImportFolder, now: i64) -> Result<usize, String> {
    let settings = &folder.settings;
    let (files, _sidecars) = paths::all_file_paths(&settings.path, settings.search_subdirectories)
        .map_err(|e| e.to_string())?;
    let known: BTreeSet<String> = store
        .read(|conn| queues::file_seeds(conn, folder.id()))
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| s.data_for_comparison)
        .collect();
    let new: Vec<String> = files.into_iter().filter(|p| !known.contains(p)).collect();
    let settled = paths::filter_older_modified(new, settings.last_modified_time_skip_period, now);
    let free: Vec<String> = settled
        .into_iter()
        .filter(|p| paths::path_is_free(p))
        .collect();
    let seeds: Vec<NewFileSeed> = free
        .iter()
        .map(|p| NewFileSeed {
            seed_type: SeedType::Path,
            data: p.clone(),
            data_for_comparison: p.clone(),
            source_time: None,
            referral_url: None,
            meta: queues::FileSeedMeta::default(),
        })
        .collect();
    let id = folder.id();
    let count = seeds.len();
    store
        .write(move |ctx| queues::add_file_seeds(ctx.conn(), id, &seeds, false, now).map(|_| ()))
        .map_err(|e| e.to_string())?;
    folder.settings.last_checked = now;
    folder.settings.check_now = false;
    Ok(count)
}

/// `_ActionSeed`: delete, move or leave a file the folder has tried. An
/// error pauses the folder.
fn action_seed(
    store: &Store,
    folder: &ImportFolder,
    seed: &FileSeed,
    recycle: bool,
) -> Result<(), String> {
    let actions = &folder.settings.actions;
    let action = match seed.status {
        SeedStatus::SuccessfulAndNew => &actions.successful_and_new,
        SeedStatus::SuccessfulButRedundant => &actions.successful_but_redundant,
        SeedStatus::Deleted => &actions.deleted,
        SeedStatus::Error => &actions.error,
        _ => return Ok(()),
    };
    let path = &seed.data;
    // in the order they are read (lower-case extension first), so that where
    // the filesystem ignores case, the sidecar moves under the name that
    // matched first rather than whichever sorts first
    let mut sidecars: Vec<String> = Vec::new();
    for sidecar in folder
        .settings
        .routers
        .iter()
        .flat_map(|r| r.possible_sidecar_paths(path))
    {
        if !sidecars.contains(&sidecar) {
            sidecars.push(sidecar);
        }
    }
    let is_file = |p: &str| std::path::Path::new(p).exists() && !std::path::Path::new(p).is_dir();
    match action {
        FolderAction::Ignore => return Ok(()),
        FolderAction::Delete => {
            let result = (|| -> std::io::Result<()> {
                if is_file(path) {
                    paths::delete_or_recycle(path, recycle)?;
                }
                for sidecar in &sidecars {
                    if std::path::Path::new(sidecar).exists() {
                        paths::delete_or_recycle(sidecar, recycle)?;
                    }
                }
                Ok(())
            })();
            if result.is_err() {
                return Err(format!("Tried to delete \"{path}\", but could not."));
            }
        }
        FolderAction::Move(dest_dir) => {
            let result = (|| -> std::io::Result<()> {
                if !std::path::Path::new(dest_dir).exists() {
                    return Err(std::io::Error::other(format!(
                        "Tried to move \"{path}\" to \"{dest_dir}\", but the destination directory did not exist."
                    )));
                }
                let move_into = |from: &str| -> std::io::Result<()> {
                    let name = std::path::Path::new(from)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let dest = std::path::Path::new(dest_dir).join(name);
                    let dest = paths::append_path_until_no_conflicts(&dest.to_string_lossy());
                    paths::merge_file(from, &dest).map(|_| ())
                };
                if is_file(path) {
                    move_into(path)?;
                }
                for sidecar in &sidecars {
                    if std::path::Path::new(sidecar).exists() {
                        move_into(sidecar)?;
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                return Err(format!(
                    "Import folder tried to move \"{path}\", but it encountered an error: {e}"
                ));
            }
        }
    }
    let id = seed.id;
    store
        .write(move |ctx| queues::remove_file_seeds_by_id(ctx.conn(), &[id]))
        .map_err(|e| e.to_string())
}

/// Every import folder, with when each is next due (`None`: never, until
/// changed).
pub fn due_times(store: &Store, now: i64) -> Result<Vec<(i64, Option<i64>)>, StoreError> {
    Ok(store
        .read(import_folders::import_folders)?
        .iter()
        .map(|f| (f.id(), f.settings.next_work_time(f.paused(), now)))
        .collect())
}

impl Downloader {
    /// `ImportFolder.DoWork`: check the folder if it is due, and import
    /// what it finds.
    pub fn work_on_import_folder(&self, id: i64) -> Result<FolderRun, StoreError> {
        let mut run = FolderRun::default();
        let store = self.store().clone();
        let Some(mut folder) = store.read(|conn| import_folders::import_folder(conn, id))? else {
            return Ok(run);
        };
        let global: FolderSettings = store.read(hydrus_store::settings::get)?;
        if global.pause_import_folders || folder.paused() {
            return Ok(run);
        }
        let now = now();
        let mut paused = false;
        let outcome = (|| -> Result<bool, String> {
            let options = self
                .full_options(CallerType::LocalImportFolder, &folder.queue.options, &[])
                .map_err(|e| e.to_string())?;
            if options.locations.destinations.is_empty() {
                return Err(
                    "There is no import destination set in the Location Import Options!".into(),
                );
            }
            let due_by_check_now = folder.settings.check_now;
            let due_by_period = folder.settings.check_regularly
                && now > folder.settings.last_checked + folder.settings.period;
            if !(due_by_check_now || due_by_period) {
                return Ok(false);
            }
            let path = std::path::Path::new(&folder.settings.path);
            if !path.is_dir() {
                return Err(format!(
                    "Path \"{}\" does not seem to exist, or is not a directory.",
                    folder.settings.path
                ));
            }
            run.new_files = check_folder(&store, &mut folder, now)?;
            run.checked = true;
            self.import_files(&mut folder, &options, global, &mut run, &mut paused)?;
            Ok(true)
        })();
        if let Err(e) = outcome {
            tracing::warn!("import folder {:?}: {e}; it has been paused", folder.name());
            run.error = Some(e);
            paused = true;
        }
        if run.checked || run.imported > 0 || run.error.is_some() || paused {
            let settings = folder.settings.clone();
            store.write(move |ctx| {
                import_folders::set_settings(ctx.conn(), id, &settings)?;
                if paused {
                    queues::set_paused(ctx.conn(), id, Some(true), None)?;
                }
                Ok(())
            })?;
        }
        Ok(run)
    }

    /// `_ImportFiles`.
    fn import_files(
        &self,
        folder: &mut ImportFolder,
        options: &FullImportOptions,
        global: FolderSettings,
        run: &mut FolderRun,
        paused: &mut bool,
    ) -> Result<(), String> {
        let store = self.store().clone();
        let id = folder.id();
        let mut previous: Option<i64> = None;
        loop {
            let Some(mut seed) = store
                .read(|conn| queues::next_file_seed(conn, id))
                .map_err(|e| e.to_string())?
            else {
                break;
            };
            if *paused {
                break;
            }
            if previous == Some(seed.id) {
                return Err(format!(
                    "Somehow we did not process the file job: {}!",
                    seed.data
                ));
            }
            previous = Some(seed.id);
            let path = seed.data.clone();
            if let Err(e) =
                self.import_path_seed(&mut seed, options, global.copy_import_files_to_temp_dir)
            {
                set_status(&mut seed, SeedStatus::Error, e.to_string());
            }
            {
                let saved = seed.clone();
                store
                    .write(move |ctx| queues::update_file_seed(ctx.conn(), &saved))
                    .map_err(|e| e.to_string())?;
            }
            if seed.status.is_successful() {
                if let Some(hash) = seed_hash(&seed) {
                    let hash_id = store
                        .read(|conn| master::hash_id(conn, &hash))
                        .map_err(|e| e.to_string())?;
                    if let Some(hash_id) = hash_id {
                        self.route_metadata(folder, hash_id, &path, run);
                        self.filename_tags(folder, hash_id, &path, run);
                    }
                }
                run.imported += 1;
            } else if seed.status == SeedStatus::Error {
                tracing::info!(
                    "import folder {:?} failed to import {path:?}",
                    folder.name()
                );
            }
            match action_seed(&store, folder, &seed, global.delete_to_recycle_bin) {
                Ok(()) => {}
                Err(e) if matches!(seed_action(folder, &seed), Some(FolderAction::Move(_))) => {
                    // the reference reports a failed move and pauses
                    run.warnings.push(e);
                    run.error
                        .get_or_insert_with(|| "a file could not be moved".into());
                    *paused = true;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn route_metadata(
        &self,
        folder: &ImportFolder,
        hash_id: HashId,
        path: &str,
        run: &mut FolderRun,
    ) {
        let mut media = StoreMedia {
            store: self.store(),
            hash_id,
            now: now(),
        };
        for router in &folder.settings.routers {
            if let Err(e) = sidecar::work(router, path, &mut media) {
                run.warnings.push(format!(
                    "Trying to run metadata routing in the import folder \"{}\" threw an error: {e}",
                    folder.name()
                ));
            }
        }
    }

    fn filename_tags(
        &self,
        folder: &ImportFolder,
        hash_id: HashId,
        path: &str,
        run: &mut FolderRun,
    ) {
        let snapshot = self.store().snapshot();
        let mut by_service: HashMap<hydrus_core::ServiceId, (bool, BTreeSet<String>)> =
            HashMap::new();
        for (key, tagging) in &folder.settings.filename_tagging {
            let Ok(key) = ServiceKey::from_hex(key) else {
                continue;
            };
            let Ok(service) = snapshot.services.by_key(&key) else {
                continue;
            };
            let tags = tagging.tags(path);
            if !tags.is_empty() {
                let pend = service.service_type() != ServiceType::LocalTag;
                by_service.insert(service.id, (pend, tags));
            }
        }
        if by_service.is_empty() {
            return;
        }
        let result = self.store().write_content(move |w| {
            for (service, (pend, tags)) in &by_service {
                let action = if *pend {
                    MappingAction::Pend
                } else {
                    MappingAction::Add
                };
                for tag in tags {
                    let Some(tag) = Tag::new(tag) else { continue };
                    let tag_id = master::intern_tag(w.conn(), &tag)?;
                    w.update_mappings(*service, &action, tag_id, &[hash_id])?;
                }
            }
            Ok(())
        });
        if let Err(e) = result {
            run.warnings.push(format!(
                "Trying to parse filename tags in the import folder \"{}\" threw an error: {e}",
                folder.name()
            ));
        }
    }
}

/// When each import folder is next due (`ImportFoldersManager`): every
/// folder is due when we start; after working, one is due again at its next
/// work time but not within three minutes; one with none is dropped until
/// it changes. Folders made or set to check now elsewhere (the command
/// line) are noticed when the list is refreshed.
#[derive(Debug, Default)]
pub struct ImportFolderSchedule {
    due: HashMap<i64, i64>,
    started: bool,
}

impl ImportFolderSchedule {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bring the list up to date with the store.
    pub fn refresh(&mut self, store: &Store, now: i64) -> Result<(), StoreError> {
        let folders = store.read(import_folders::import_folders)?;
        let ids: BTreeSet<i64> = folders.iter().map(ImportFolder::id).collect();
        self.due.retain(|id, _| ids.contains(id));
        for f in &folders {
            if f.paused() {
                self.due.remove(&f.id());
            } else if !self.started || f.settings.check_now {
                self.due.insert(f.id(), now);
            } else if !self.due.contains_key(&f.id())
                && let Some(next) = f.settings.next_work_time(false, now)
            {
                self.due.insert(f.id(), next);
            }
        }
        self.started = true;
        Ok(())
    }

    /// A folder that is due, if any.
    pub fn next_due(&self, now: i64) -> Option<i64> {
        self.due
            .iter()
            // (`TimeHasPassed`: strictly after)
            .filter(|(_, due)| now > **due)
            .min_by_key(|(id, due)| (**due, **id))
            .map(|(id, _)| *id)
    }

    /// Record that a folder worked, with when it is next due.
    pub fn worked(&mut self, id: i64, next: Option<i64>, now: i64) {
        match next {
            None => {
                self.due.remove(&id);
            }
            Some(next) => {
                self.due.insert(id, next.max(now + 180));
            }
        }
    }

    /// How long to sleep before looking again (`_GetTimeUntilNextWork`).
    pub fn seconds_until_next(&self, now: i64) -> i64 {
        match self.due.values().min() {
            None => 1800,
            Some(next) => (next - now).clamp(1, 1800),
        }
    }
}

/// Work every due import folder once; how long until one is next due.
pub fn work_due_import_folders(
    downloader: &Downloader,
    schedule: &mut ImportFolderSchedule,
) -> Result<i64, StoreError> {
    let store = downloader.store().clone();
    let global: FolderSettings = store.read(hydrus_store::settings::get)?;
    if global.pause_import_folders {
        return Ok(1800);
    }
    schedule.refresh(&store, now())?;
    while let Some(id) = schedule.next_due(now()) {
        let run = downloader.work_on_import_folder(id)?;
        if run.imported > 0 {
            tracing::info!("import folder {id} imported {} files", run.imported);
        }
        for warning in &run.warnings {
            tracing::warn!("{warning}");
        }
        let next = store
            .read(|conn| import_folders::import_folder(conn, id))?
            .and_then(|f| f.settings.next_work_time(f.paused(), now()));
        schedule.worked(id, next, now());
    }
    Ok(schedule.seconds_until_next(now()))
}

fn seed_action<'a>(folder: &'a ImportFolder, seed: &FileSeed) -> Option<&'a FolderAction> {
    let actions = &folder.settings.actions;
    match seed.status {
        SeedStatus::SuccessfulAndNew => Some(&actions.successful_and_new),
        SeedStatus::SuccessfulButRedundant => Some(&actions.successful_but_redundant),
        SeedStatus::Deleted => Some(&actions.deleted),
        SeedStatus::Error => Some(&actions.error),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::py_int;

    #[test]
    fn integers_parse_as_python_parses_them() {
        assert_eq!(py_int(" 12 "), Some(12));
        assert_eq!(py_int("+1_000"), Some(1000));
        assert_eq!(py_int("-3"), Some(-3));
        assert_eq!(py_int("1__0"), None);
        assert_eq!(py_int("1.5"), None);
        assert_eq!(py_int(""), None);
    }
}
