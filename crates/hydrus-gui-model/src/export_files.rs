//! Manual exports: preview names in selection order and execute an immutable plan
//! off the GUI thread. Trashing is permitted only after complete success.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use hydrus_core::HashId;
use hydrus_parse::folders::parse_export_phrase;
use hydrus_parse::sidecar::Router;
use hydrus_store::{Store, settings};

#[path = "export_files_tags.rs"]
pub mod tags;

/// The reference's destructive export confirmation.
pub const TRASH_WARNING: &str = "THE FILES WILL BE SENT TO THE TRASH IN THE CLIENT AFTERWARDS";
/// The reference's remove-from-preview question.
pub const REMOVE_QUESTION: &str = "Remove all selected?";

/// The shared folder/manual export pattern menu copies phrases; it does not
/// replace the current filename pattern.
pub const PATTERN_SHORTCUT_HEADING: &str = "click on a phrase to copy to clipboard";
pub const PATTERN_SHORTCUTS: [(&str, &str); 7] = [
    ("unique numerical file id - {file_id}", "{file_id}"),
    ("the file's hash - {hash}", "{hash}"),
    ("all the file's tags - {tags}", "{tags}"),
    (
        "all the file's non-namespaced tags - {nn tags}",
        "{nn tags}",
    ),
    ("file order - {#}", "{#}"),
    (
        "all instances of a particular namespace - [\u{2026}]",
        "[\u{2026}]",
    ),
    (
        "a particular tag, if the file has it - (\u{2026})",
        "(\u{2026})",
    ),
];

/// Out-of-range native menu callbacks cannot produce a clipboard payload.
pub fn pattern_shortcut(index: i32) -> Option<&'static str> {
    if index == 7 {
        return Some(PATTERN_SHORTCUT_HEADING);
    }
    usize::try_from(index)
        .ok()
        .and_then(|index| PATTERN_SHORTCUTS.get(index))
        .map(|(_, phrase)| *phrase)
}

/// Remembered manual export choices, independent of scheduled export folders.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Previously remembered destination, retained for settings compatibility.
    /// New panels use the shared ExportSettings default directory.
    pub destination: String,
    /// Whether to send exported media to the client's trash.
    pub trash: bool,
    /// Media metadata routes to sidecars.
    pub routers: Vec<Router>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            destination: std::env::var("HOME")
                .map_or_else(|_| String::new(), |h| format!("{h}/hydrus_export")),
            trash: false,
            routers: Vec::new(),
        }
    }
}
impl settings::Setting for Preferences {
    const KEY: &'static str = "manual_export";
}

/// Resolve the shared default used whenever a manual export panel opens.
/// Portable relative paths are relative to this client's database directory.
pub fn default_directory(store: &Store, naming: &settings::ExportSettings) -> String {
    let path = naming.default_directory.as_ref().map_or_else(
        || {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join("hydrus_export"))
        },
        |path| {
            let path = Path::new(path);
            Some(if path.is_absolute() {
                path.to_path_buf()
            } else {
                store.dir().join(path)
            })
        },
    );
    path.map(|path| normalise(&path).to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A preview row, retaining the original file identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// File exported by this row.
    pub file: HashId,
    /// One-based order used by `{#}`.
    pub number: usize,
    /// The displayed filetype.
    pub mime: String,
    /// Absolute destination preview.
    pub destination: PathBuf,
}

/// A validated, immutable export request.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Root directory the worker may write inside.
    pub directory: PathBuf,
    /// Preview rows in selection order.
    pub rows: Vec<Row>,
    /// Sidecar actions from the existing router editor.
    pub routers: Vec<Router>,
    /// Trashing disables symlinks.
    pub trash: bool,
    /// Export symbolic links instead of copies.
    pub symlinks: bool,
}

/// `os.path.normpath`: `.` and `..` resolved by text, trailing separators
/// dropped.
pub fn normpath(path: &Path) -> PathBuf {
    normalise(path)
}

fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(c.as_os_str()),
        }
    }
    out
}

/// Resolve the chosen export root using the same normalization as previews.
pub fn directory_path(directory: &str) -> Result<PathBuf, String> {
    if directory.trim().is_empty() {
        return Err("Please choose an export path.".into());
    }
    Ok(normalise(
        &std::path::absolute(directory).map_err(|e| e.to_string())?,
    ))
}

/// The prefix the reference shows before a row's path, or its error, when
/// the row cannot be exported under the destination.
pub const INVALID_PREFIX: &str = "INVALID, above destination directory: ";

/// A preview row as the reference's list shows it: the destination, or the
/// reason there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// File exported by this row.
    pub file: HashId,
    /// One-based order used by `{#}`.
    pub number: usize,
    /// The displayed filetype.
    pub mime: String,
    /// Where the file goes, or the text shown in its place (after
    /// [`INVALID_PREFIX`]).
    pub destination: Result<PathBuf, String>,
}

impl Shown {
    /// The destination column's text while the destination field reads
    /// `directory`: the path, or the error, after [`INVALID_PREFIX`] unless
    /// it begins with the field's text (the reference's string test, made
    /// as the list is drawn).
    pub fn text(&self, directory: &str) -> String {
        let raw = match &self.destination {
            Ok(path) => path.to_string_lossy().into_owned(),
            Err(why) => why.clone(),
        };
        if raw.starts_with(directory) {
            raw
        } else {
            format!("{INVALID_PREFIX}{raw}")
        }
    }
}

/// Every row's name with the shared filename generator, as the reference's
/// list shows them: a name that cannot be made (too long for the limits,
/// say) or that leaves the destination is that row's error, and the rest
/// are still named. Names already used in this export get " (1)", " (2)"...
/// (existing files on disk will be overwritten). Only a missing destination
/// or an unparsable phrase fails the whole list.
pub fn shown(
    store: &Store,
    files: &[HashId],
    directory: &str,
    phrase: &str,
) -> Result<Vec<Shown>, String> {
    let directory = directory_path(directory)?;
    let terms = parse_export_phrase(phrase)
        .map_err(|e| format!("Problem parsing export phrase!\n\n{e}"))?;
    let snapshot = store.snapshot();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, files))
        .map_err(|e| e.to_string())?;
    let mut used = HashSet::new();
    let mut rows = Vec::new();
    for (i, file) in files.iter().enumerate() {
        let media = batch
            .results
            .iter()
            .find(|m| m.hash_id == *file)
            .ok_or("File metadata is missing.")?;
        let destination = hydrus_download::export::filename_for_media(
            store,
            &directory.to_string_lossy(),
            media,
            &terms,
            i + 1,
        )
        .and_then(|name| {
            let path = Path::new(&name);
            let ext = path
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let stem = name.strip_suffix(&ext).unwrap_or(&name);
            let mut unique = name.clone();
            let mut suffix = 1;
            while used.contains(&unique) {
                unique = format!("{stem} ({suffix}){ext}");
                suffix += 1;
            }
            used.insert(unique.clone());
            let destination = normalise(&directory.join(unique));
            if destination == directory || !destination.starts_with(&directory) {
                return Err(destination.to_string_lossy().into_owned());
            }
            Ok(destination)
        });
        rows.push(Shown {
            file: *file,
            number: i + 1,
            mime: media.info.as_ref().map_or_else(
                || hydrus_core::Mime::ApplicationUnknown.human_name().into(),
                |info| info.mime.human_name().into(),
            ),
            destination,
        });
    }
    Ok(rows)
}

/// The rows to export: [`shown`], failing on the first row that has no
/// destination.
pub fn preview(
    store: &Store,
    files: &[HashId],
    directory: &str,
    phrase: &str,
) -> Result<Vec<Row>, String> {
    shown(store, files, directory, phrase)?
        .into_iter()
        .map(|row| match row.destination {
            Ok(destination) => Ok(Row {
                file: row.file,
                number: row.number,
                mime: row.mime,
                destination,
            }),
            Err(why) => Err(format!("{INVALID_PREFIX}{why}")),
        })
        .collect()
}

impl Plan {
    /// Ask before destructive export, or always when exporting and closing.
    pub fn confirmation(&self, close_after: bool) -> Option<String> {
        match (close_after, self.trash) {
            (true, true) => Some(format!("Export as shown?\n\n{TRASH_WARNING}")),
            (true, false) => Some("Export as shown?".into()),
            (false, true) => Some(TRASH_WARNING.into()),
            (false, false) => None,
        }
    }
}

/// Worker progress; errors and cancellation leave every source in the client.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// Successfully exported file count.
    pub completed: usize,
    /// Planned file count.
    pub total: usize,
    /// Worker has stopped.
    pub finished: bool,
    /// User cancelled before completion.
    pub cancelled: bool,
    /// The first export failure.
    pub error: Option<String>,
    /// Successfully exported files submitted to the client trash.
    pub trashed: usize,
}

// Replace directory entries rather than truncating existing files: an existing
// destination (including a sidecar) may be a hardlink to client source data.
fn copy_atomically(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or("Destination has no directory.")?;
    let temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    hydrus_import::paths::mirror_file(
        &source.to_string_lossy(),
        &temporary.path().to_string_lossy(),
    )
    .map_err(|e| e.to_string())?;
    hydrus_import::paths::give_nice_permission_bits(temporary.path());
    temporary.persist(destination).map_err(|e| e.to_string())?;
    Ok(())
}

// Check each existing ancestor before creating the next directory, so a
// symlinked subfolder cannot cause even directory creation outside the root.
fn create_parent(directory: &Path, destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or("Destination has no directory.")?;
    let relative = parent
        .strip_prefix(directory)
        .map_err(|_| "Destination directory is outside the export directory.")?;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let root = std::fs::canonicalize(directory).map_err(|e| e.to_string())?;
    let mut current = directory.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if !current.exists() {
            std::fs::create_dir(&current).map_err(|e| e.to_string())?;
        }
        let resolved = std::fs::canonicalize(&current).map_err(|e| e.to_string())?;
        if !resolved.starts_with(&root) {
            return Err("Destination subfolder is outside the export directory.".into());
        }
        if !resolved.is_dir() {
            return Err("Destination parent is not a directory.".into());
        }
    }
    std::fs::canonicalize(parent).map_err(|e| e.to_string())
}

fn export_one(store: &Store, plan: &Plan, row: &Row) -> Result<(), String> {
    let snapshot = store.snapshot();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[row.file]))
        .map_err(|e| e.to_string())?;
    let media = batch.results.first().ok_or("File metadata is missing.")?;
    let source = media.info.as_ref().and_then(|i| snapshot.storage.file_path(&media.hash, i.mime)).filter(|p| p.is_file()).ok_or_else(|| format!("When trying to export {}, I discovered that it was actually missing from your client! The export job should stop now. You should go to _database->file maintenance_ and set up a scan for missing files!", media.hash.to_hex()))?;
    let destination = normalise(&row.destination);
    if !destination.starts_with(&plan.directory) || destination == plan.directory {
        return Err("A destination path was above the main export directory!".into());
    }
    let parent = create_parent(&plan.directory, &destination)?;
    for location in snapshot.storage.locations() {
        for prefix in &location.prefixes {
            if let Ok(storage) = std::fs::canonicalize(
                location
                    .path
                    .join(prefix.chars().take(3).collect::<String>()),
            ) && parent.starts_with(storage)
            {
                return Err("Cannot export into the client's file or thumbnail storage.".into());
            }
        }
    }
    if destination
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("An existing symlink is in the way of this export.".into());
    }
    if std::fs::canonicalize(&destination).ok() == std::fs::canonicalize(&source).ok() {
        return Err("Cannot export a file over its own source.".into());
    }
    for router in &plan.routers {
        let (naming, ext) = match &router.exporter {
            hydrus_parse::sidecar::Exporter::Txt { naming, .. } => (naming, "txt"),
            hydrus_parse::sidecar::Exporter::Json { naming, .. } => (naming, "json"),
            _ => return Err("Manual exports accept only sidecar destinations.".into()),
        };
        let sidecar = normalise(Path::new(&naming.path(&destination.to_string_lossy(), ext)));
        if !sidecar.starts_with(&plan.directory)
            || sidecar == plan.directory
            || sidecar == destination
        {
            return Err("A sidecar destination is outside the export directory.".into());
        }
        if sidecar
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err("An existing symlink is in the way of this sidecar export.".into());
        }
        let parent = create_parent(&plan.directory, &sidecar)?;
        for location in snapshot.storage.locations() {
            for prefix in &location.prefixes {
                if let Ok(storage) = std::fs::canonicalize(
                    location
                        .path
                        .join(prefix.chars().take(3).collect::<String>()),
                ) && parent.starts_with(storage)
                {
                    return Err(
                        "Cannot export sidecars into the client's file or thumbnail storage."
                            .into(),
                    );
                }
            }
        }
        if sidecar.is_file() {
            copy_atomically(&sidecar, &sidecar)?;
        }
    }
    hydrus_download::export::route_sidecars(
        store,
        row.file,
        &destination.to_string_lossy(),
        &plan.routers,
    )?;
    if plan.symlinks && !plan.trash {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, &destination).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&source, &destination).map_err(|e| format!("The symlink creation failed. It may be you need to run hydrus as Admin for this to work! {e}"))?;
    } else {
        copy_atomically(&source, &destination)?;
    }
    Ok(())
}

/// Execute on a background thread, reporting after each file. Cancellation
/// stops between files; an in-flight copy completes before cancellation returns.
/// An export failure still permits trashing its successful prefix, as the
/// reference does. Cancellation before the deletion phase suppresses all trashing.
pub fn run(
    store: &Store,
    plan: &Plan,
    cancel: &AtomicBool,
    mut report: impl FnMut(Progress),
) -> Progress {
    let mut progress = Progress {
        total: plan.rows.len(),
        ..Progress::default()
    };
    for row in &plan.rows {
        if cancel.load(Ordering::Acquire) {
            progress.cancelled = true;
            break;
        }
        if let Err(e) = export_one(store, plan, row) {
            progress.error = Some(format!(
                "Encountered a problem while attempting to export file #{}:\n\n{e}",
                row.number
            ));
            break;
        }
        progress.completed += 1;
        report(progress.clone());
    }
    progress.cancelled |= cancel.load(Ordering::Acquire);
    if plan.trash && !progress.cancelled {
        let reason = format!(
            "Deleted after manual export to \"{}\".",
            plan.directory.display()
        );
        // Only complete file-and-sidecar exports enter this prefix. Commit in
        // the reference's bounded chunks; cancellation is sampled before this
        // phase, not between already-authorised deletion transactions.
        for chunk in plan.rows[..progress.completed].chunks(64) {
            let files: Vec<_> = chunk.iter().map(|r| r.file).collect();
            let reason = reason.clone();
            match store.write_content(move |w| {
                let domain = w.roles().combined_local_media;
                w.delete_files(domain, &files, Some(&reason))
            }) {
                Ok(()) => progress.trashed += chunk.len(),
                Err(e) => {
                    let message = format!("Could not trash exported files: {e}");
                    progress.error = Some(match progress.error.take() {
                        Some(export_error) => format!("{export_error}\n\n{message}"),
                        None => message,
                    });
                    break;
                }
            }
        }
    }
    progress.finished = true;
    report(progress.clone());
    progress
}
