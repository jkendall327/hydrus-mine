//! Manual exports: preview names in selection order and execute an immutable plan
//! off the GUI thread. Trashing is permitted only after complete success.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use hydrus_core::HashId;
use hydrus_parse::folders::parse_export_phrase;
use hydrus_parse::sidecar::Router;
use hydrus_store::{Store, settings};

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
    /// Destination last used for manual exports.
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

/// Build the preview with the shared filename generator, distinguishing only
/// collisions within this export (existing files on disk will be overwritten).
pub fn preview(
    store: &Store,
    files: &[HashId],
    directory: &str,
    phrase: &str,
) -> Result<Vec<Row>, String> {
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
        let name = hydrus_download::export::filename_for_media(
            store,
            &directory.to_string_lossy(),
            media,
            &terms,
            i + 1,
        )?;
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
            return Err(format!(
                "INVALID, above destination directory: {}",
                destination.display()
            ));
        }
        rows.push(Row {
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
    /// Files sent to the client trash after complete success.
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
    if plan.symlinks && !plan.trash {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&source, &destination).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&source, &destination).map_err(|e| format!("The symlink creation failed. It may be you need to run hydrus as Admin for this to work! {e}"))?;
    } else {
        copy_atomically(&source, &destination)?;
    }
    hydrus_download::export::route_sidecars(
        store,
        row.file,
        &destination.to_string_lossy(),
        &plan.routers,
    )
}

/// Execute on a background thread, reporting after each file. Cancellation
/// stops between files; an in-flight copy completes before cancellation returns.
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
    if plan.trash
        && !progress.cancelled
        && progress.error.is_none()
        && progress.completed == progress.total
    {
        let files: Vec<_> = plan.rows.iter().map(|r| r.file).collect();
        let reason = format!(
            "Deleted after manual export to \"{}\".",
            plan.directory.display()
        );
        match store.write_content(move |w| {
            let domain = w.roles().combined_local_media;
            w.delete_files(domain, &files, Some(&reason))
        }) {
            Ok(()) => progress.trashed = progress.completed,
            Err(e) => progress.error = Some(e.to_string()),
        }
    }
    progress.finished = true;
    report(progress.clone());
    progress
}
