//! Export folders at work (`ExportFolder.DoWork`): export the files a
//! search finds, named by a phrase, copied or linked, with sidecars; when
//! synchronising, remove whatever else is in the folder; optionally delete
//! the exported files from the client.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use hydrus_core::content::ContentStatus;
use hydrus_core::{HashId, ServiceType};
use hydrus_import::paths;
use hydrus_parse::folders::{ExportFolder, ExportType, PhraseTerm, parse_export_phrase};
use hydrus_parse::sidecar::{self, Exporter};
use hydrus_search::exec::FileSort;
use hydrus_store::media::MediaResult;
use hydrus_store::settings::{ExportFolders, ExportSettings, FolderSettings};
use hydrus_store::{Store, StoreError, master};

use crate::folders::StoreMedia;
use crate::now;

/// What a file's name can draw on.
struct NameFacts<'a> {
    media: &'a MediaResult,
    /// Current and pending tags on all services, siblings and parents
    /// applied (`DISPLAY_ACTUAL` on the combined tag service).
    display_tags: BTreeSet<String>,
}

fn clean_tag_text(text: &str) -> String {
    if cfg!(windows) {
        text.replace('\\', "_")
    } else {
        text.replace('/', "_")
    }
}

/// `GenerateExportFilename` (without the "do not use" list, which export
/// folders don't pass).
fn export_filename(
    destination: &str,
    facts: &NameFacts<'_>,
    terms: &[PhraseTerm],
    index: usize,
    settings: &ExportSettings,
    force_ntfs: bool,
) -> Result<String, String> {
    let mut name = String::new();
    for term in terms {
        match term {
            PhraseTerm::Text(text) => name.push_str(text),
            PhraseTerm::Namespace(namespace) => {
                let prefix = format!("{namespace}:");
                let mut subtags: Vec<&str> = facts
                    .display_tags
                    .iter()
                    .filter(|t| t.starts_with(&prefix))
                    .map(|t| hydrus_core::tag::split_tag(t).1)
                    .collect();
                subtags.sort_unstable();
                name.push_str(&clean_tag_text(&subtags.join(", ")));
            }
            PhraseTerm::Predicate(p) if p == "tags" || p == "nn tags" => {
                let tags: Vec<&str> = if p == "nn tags" {
                    facts
                        .display_tags
                        .iter()
                        .filter(|t| !t.contains(':'))
                        .map(String::as_str)
                        .collect()
                } else {
                    facts
                        .display_tags
                        .iter()
                        .map(|t| hydrus_core::tag::split_tag(t).1)
                        .collect()
                };
                name.push_str(&clean_tag_text(&tags.join(", ")));
            }
            PhraseTerm::Predicate(p) if p == "hash" => name.push_str(&facts.media.hash.to_hex()),
            PhraseTerm::Predicate(p) if p == "file_id" => {
                name.push_str(&facts.media.hash_id.0.to_string());
            }
            PhraseTerm::Predicate(p) if p == "#" => name.push_str(&index.to_string()),
            PhraseTerm::Predicate(_) => {}
            PhraseTerm::Tag(tag) => {
                let subtag = hydrus_core::tag::split_tag(tag).1;
                if facts.display_tags.contains(subtag) {
                    name.push_str(&clean_tag_text(subtag));
                }
            }
        }
    }
    let sep = std::path::MAIN_SEPARATOR;
    let mut name = name.trim_start_matches(sep).to_owned();
    let doubled: String = [sep, sep].iter().collect();
    while name.contains(&doubled) {
        name = name.replace(&doubled, &sep.to_string());
    }
    let mime = facts
        .media
        .info
        .as_ref()
        .map_or(hydrus_core::Mime::ApplicationUnknown, |i| i.mime);
    let ext = mime.extension().unwrap_or("");
    if !ext.is_empty()
        && let Some(stripped) = name.strip_suffix(ext)
    {
        name = stripped.to_owned();
    }
    let (subdirs, true_name) = match name.rfind(sep) {
        Some(i) => (name[..i].to_owned(), name[i + 1..].to_owned()),
        None => (String::new(), name.clone()),
    };
    let true_name = if true_name.is_empty() {
        facts.media.hash.to_hex()
    } else {
        true_name
    };
    let (subdirs, elided) = paths::elide_filename(
        destination,
        &subdirs,
        &true_name,
        ext,
        settings.path_character_limit,
        settings.dirname_character_limit,
        settings.filename_character_limit,
        force_ntfs,
    )?;
    let joined = if subdirs.is_empty() {
        elided
    } else {
        format!("{subdirs}{sep}{elided}")
    };
    Ok(format!("{joined}{ext}"))
}

/// `os.path.normpath`, as text.
fn normpath(path: &str) -> String {
    let p = Path::new(path);
    // a Windows drive or share ("C:", or "\\server\share")
    let mut prefix = String::new();
    let mut out: Vec<String> = Vec::new();
    let absolute = p.has_root();
    for c in p.components() {
        match c {
            std::path::Component::CurDir | std::path::Component::RootDir => {}
            std::path::Component::ParentDir => {
                if out.last().is_some_and(|l| l != "..") {
                    out.pop();
                } else if !absolute {
                    out.push("..".into());
                }
            }
            std::path::Component::Prefix(p) => {
                prefix = p.as_os_str().to_string_lossy().into_owned();
            }
            std::path::Component::Normal(n) => out.push(n.to_string_lossy().into_owned()),
        }
    }
    let sep = std::path::MAIN_SEPARATOR_STR;
    let joined = out.join(sep);
    if absolute {
        format!("{prefix}{sep}{joined}")
    } else if joined.is_empty() && prefix.is_empty() {
        ".".into()
    } else {
        format!("{prefix}{joined}")
    }
}

/// Every file under `root` (links included, not followed into).
fn all_files(root: &Path, out: &mut HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            all_files(&path, out);
        } else if kind.is_symlink() && std::fs::metadata(&path).is_ok_and(|m| m.is_dir()) {
            // (os.walk lists a link to a directory as a directory, and
            // doesn't go into it)
        } else if let Some(p) = path.to_str() {
            out.insert(p.to_owned());
        }
    }
}

/// Directories under `root` with no files and no directories left once
/// these go (bottom up, as `os.walk(topdown=False)` finds them).
fn empty_dirs(root: &Path, dir: &Path, out: &mut Vec<String>) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut has_files = false;
    let mut useful_dirs = false;
    for entry in entries.flatten() {
        let path = entry.path();
        // (os.walk counts links to directories as directories, but doesn't
        // go into them)
        let is_dir = std::fs::metadata(&path).is_ok_and(|m| m.is_dir());
        let is_link = entry.file_type().is_ok_and(|t| t.is_symlink());
        if is_dir && !is_link {
            if !empty_dirs(root, &path, out) {
                useful_dirs = true;
            }
        } else if is_dir {
            useful_dirs = true;
        } else {
            has_files = true;
        }
    }
    let empty = !has_files && !useful_dirs;
    if empty && dir != root {
        out.push(dir.to_string_lossy().into_owned());
    }
    empty
}

/// What a run did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExportRun {
    pub ran: bool,
    pub exported: usize,
    pub copied: usize,
    pub deleted_paths: usize,
    pub deleted_from_client: usize,
    pub error: Option<String>,
}

fn display_tags(store: &Store, media: &MediaResult) -> Result<BTreeSet<String>, StoreError> {
    let snapshot = store.snapshot();
    let mut ids = BTreeSet::new();
    for service in snapshot.services.tag_services() {
        let Some(tags) = media.tags.get(&service.id) else {
            continue;
        };
        let graph = snapshot.display.get(service.id);
        for status in [ContentStatus::Current, ContentStatus::Pending] {
            if let Some(stored) = tags.by_status.get(&status) {
                ids.extend(stored.iter().flat_map(|t| graph.display_tags(*t)));
            }
        }
    }
    let ids: Vec<_> = ids.into_iter().collect();
    let names = store.read(|conn| master::tags(conn, &ids))?;
    Ok(names.values().map(|t| t.as_str().to_owned()).collect())
}

/// `_DoExport`.
fn export(store: &Store, folder: &mut ExportFolder, run: &mut ExportRun) -> Result<(), String> {
    let e = |e: StoreError| e.to_string();
    let snapshot = store.snapshot();
    let global: FolderSettings = store.read(hydrus_store::settings::get).map_err(e)?;
    let settings: ExportSettings = store.read(hydrus_store::settings::get).map_err(e)?;
    let search = folder.search.clone();
    let clock = hydrus_search::Clock::system();
    // (sorted by file id below, as the reference does)
    let sort = FileSort::default();
    let mut ids: Vec<HashId> = store
        .read(|conn| {
            hydrus_search::search_files(conn, &snapshot, &search, sort, &clock)
                .map_err(|e| StoreError::Invalid(e.to_string()))
        })
        .map_err(e)?;
    ids.sort_unstable();
    let terms = parse_export_phrase(&folder.phrase)?;
    let root = folder.path.clone();
    let mut previous = HashSet::new();
    all_files(Path::new(&root), &mut previous);
    let force_ntfs = paths::needs_ntfs_rules(&root, settings.always_apply_ntfs_rules);
    let mut sync_paths: HashSet<String> = HashSet::new();
    let mut new_sidecars: HashSet<String> = HashSet::new();
    let mut old_sidecars: HashSet<String> = HashSet::new();
    let overwrite = folder.overwrite_sidecars_on_next_run || folder.always_overwrite_sidecars;
    let mut exported_media: Vec<MediaResult> = Vec::new();
    for chunk in ids.chunks(64) {
        if store
            .read(hydrus_store::settings::get::<FolderSettings>)
            .map_err(e)?
            .pause_export_folders
        {
            return Ok(());
        }
        let batch = store
            .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, chunk))
            .map_err(e)?;
        exported_media.extend(batch.results);
    }
    exported_media.sort_by_key(|m| m.hash_id);
    for (i, media) in exported_media.iter().enumerate() {
        let facts = NameFacts {
            media,
            display_tags: display_tags(store, media).map_err(e)?,
        };
        let filename = export_filename(&root, &facts, &terms, i + 1, &settings, force_ntfs)
            .or_else(|_| {
                export_filename(
                    &root,
                    &facts,
                    &[PhraseTerm::Predicate("hash".into())],
                    i + 1,
                    &settings,
                    force_ntfs,
                )
            })?;
        let dest = normpath(&Path::new(&root).join(&filename).to_string_lossy());
        if !dest.starts_with(&root) {
            return Err(format!(
                "It seems a destination path for export folder \"{root}\" was above the main export directory! The file was \"{}\" and its destination path was \"{dest}\".",
                media.hash.to_hex()
            ));
        }
        if let Some(parent) = Path::new(&dest).parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if !sync_paths.contains(&dest) {
            let mime = media.info.as_ref().map(|i| i.mime);
            let source = mime
                .and_then(|m| snapshot.storage.file_path(&media.hash, m))
                .filter(|p| p.exists())
                .ok_or_else(|| {
                    format!(
                        "A file to be exported, hash \"{}\", was missing! You should run \"missing file\" file maintenance to check if any other files in your export folder's search--or your whole database--are also missing.",
                        media.hash.to_hex()
                    )
                })?;
            let source = source.to_string_lossy().into_owned();
            let copied = if folder.export_symlinks {
                // (a broken link in the way fails, as in the reference)
                if Path::new(&dest).exists() {
                    false
                } else {
                    make_symlink(&source, &dest)?;
                    true
                }
            } else {
                let copied = paths::mirror_file(&source, &dest).map_err(|e| e.to_string())?;
                if copied {
                    paths::give_nice_permission_bits(&dest);
                }
                copied
            };
            if copied {
                run.copied += 1;
            }
        }
        sync_paths.insert(dest.clone());
        run.exported += 1;
        for router in &folder.routers {
            let (Exporter::Txt { naming, .. } | Exporter::Json { naming, .. }) = &router.exporter
            else {
                continue;
            };
            let ext = if matches!(router.exporter, Exporter::Txt { .. }) {
                "txt"
            } else {
                "json"
            };
            let sidecar_path = naming.path(&dest, ext);
            if Path::new(&sidecar_path).exists() {
                if !new_sidecars.contains(&sidecar_path)
                    && old_sidecars.insert(sidecar_path.clone())
                    && overwrite
                {
                    // (for good, not to the recycle bin)
                    paths::delete_path(&sidecar_path).map_err(|e| e.to_string())?;
                }
            } else {
                new_sidecars.insert(sidecar_path.clone());
            }
            if new_sidecars.contains(&sidecar_path) || overwrite {
                let mut media_access = StoreMedia {
                    store,
                    hash_id: media.hash_id,
                    now: now(),
                };
                sidecar::work(router, &dest, &mut media_access).map_err(|e| e.to_string())?;
            }
            sync_paths.insert(sidecar_path);
        }
    }
    if folder.export_type == ExportType::Synchronise {
        for path in previous.difference(&sync_paths) {
            paths::delete_or_recycle(path, global.delete_to_recycle_bin)
                .map_err(|e| e.to_string())?;
            run.deleted_paths += 1;
        }
        let mut dirs = Vec::new();
        empty_dirs(Path::new(&root), Path::new(&root), &mut dirs);
        for dir in dirs {
            if Path::new(&dir).exists() {
                paths::delete_path(&dir).map_err(|e| e.to_string())?;
            }
        }
    }
    if folder.export_type == ExportType::Regular && folder.delete_from_client_after_export {
        let Some(combined) = snapshot
            .services
            .of_type(ServiceType::CombinedLocalFileDomains)
            .next()
            .map(|s| s.id)
        else {
            return Ok(());
        };
        let mine: Vec<HashId> = exported_media
            .iter()
            .filter(|m| m.is_current_in(combined))
            .map(|m| m.hash_id)
            .collect();
        let reason = format!("Deleted after export to Export Folder \"{root}\".");
        run.deleted_from_client = mine.len();
        for chunk in mine.chunks(64) {
            let chunk = chunk.to_vec();
            let reason = reason.clone();
            store
                .write_content(move |w| w.delete_files(combined, &chunk, Some(&reason)))
                .map_err(e)?;
        }
    }
    Ok(())
}

fn make_symlink(source: &str, dest: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, dest).map_err(|e| e.to_string())
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(source, dest).map_err(|_| {
            "The symlink creation failed. It may be you need to run hydrus as Admin for this to work!"
                .to_owned()
        })
    }
}

/// `ExportFolder.DoWork` for the folder named `name`: run it if due, and
/// save what happened.
pub fn work_on_export_folder(store: &Store, name: &str) -> Result<ExportRun, StoreError> {
    let mut run = ExportRun::default();
    let folders: ExportFolders = store.read(hydrus_store::settings::get)?;
    let Some(mut folder) = folders.0.into_iter().find(|f| f.name == name) else {
        return Ok(run);
    };
    let now = now();
    if !folder.is_due(now) {
        return Ok(run);
    }
    run.ran = true;
    let result = (|| -> Result<(), String> {
        let path = Path::new(&folder.path);
        if folder.path.is_empty() {
            return Err("No path set for the folder!".into());
        }
        if !path.exists() {
            return Err(format!("The path, \"{}\", does not exist!", folder.path));
        }
        if !path.is_dir() {
            return Err(format!(
                "The path, \"{}\", is not a directory!",
                folder.path
            ));
        }
        export(store, &mut folder, &mut run)
    })();
    match result {
        Ok(()) => folder.last_error.clear(),
        Err(e) => {
            tracing::warn!("export folder {name:?}: {e}");
            // (the reference's words, run together as it runs them)
            let pause = if folder.run_regularly {
                "It has been set to not run regularly."
            } else {
                ""
            };
            crate::popups::show_error(
                store,
                format!(
                    "The export folder \"{name}\" encountered an error! {pause}Please check the folder's settings and maybe report to hydrus dev if the error is complicated! The error follows:"
                ),
                e.clone(),
            );
            folder.run_regularly = false;
            folder.last_error.clone_from(&e);
            run.error = Some(e);
        }
    }
    folder.last_checked = now;
    folder.overwrite_sidecars_on_next_run = false;
    folder.run_now = false;
    let saved = folder.clone();
    store.write(move |ctx| {
        let mut folders: ExportFolders = hydrus_store::settings::get(ctx.conn())?;
        if let Some(slot) = folders.0.iter_mut().find(|f| f.name == saved.name) {
            *slot = saved;
        }
        hydrus_store::settings::set(ctx.conn(), &folders)
    })?;
    Ok(run)
}

/// `DAEMONCheckExportFolders`: give every export folder the chance to run.
pub fn work_export_folders(store: &Store) -> Result<Vec<(String, ExportRun)>, StoreError> {
    let global: FolderSettings = store.read(hydrus_store::settings::get)?;
    if global.pause_export_folders {
        return Ok(Vec::new());
    }
    let folders: ExportFolders = store.read(hydrus_store::settings::get)?;
    let mut names: Vec<String> = folders.0.into_iter().map(|f| f.name).collect();
    names.sort();
    let mut runs = Vec::new();
    for name in names {
        let paused: FolderSettings = store.read(hydrus_store::settings::get)?;
        if paused.pause_export_folders {
            break;
        }
        let run = work_on_export_folder(store, &name)?;
        if run.ran {
            runs.push((name, run));
        }
    }
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::normpath;

    #[test]
    fn paths_normalise_as_python_normalises_them() {
        if cfg!(windows) {
            assert_eq!(normpath(r"C:\a\b\..\c"), r"C:\a\c");
            assert_eq!(normpath("C:/a/./b"), r"C:\a\b");
            assert_eq!(normpath(r"\\server\share\a\..\b"), r"\\server\share\b");
        } else {
            assert_eq!(normpath("/a/b/../c"), "/a/c");
            assert_eq!(normpath("/a/./b//c/"), "/a/b/c");
            assert_eq!(normpath("a/../../b"), "../b");
            assert_eq!(normpath(""), ".");
        }
    }
}
