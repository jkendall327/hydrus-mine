//! Files outside the store, as import and export folders handle them: the
//! reference's `ClientFiles.GetAllFilePaths` and the `HydrusPaths` helpers
//! they use. Paths are strings, as in the reference (a name that isn't
//! UTF-8 is skipped).

use std::collections::HashSet;
use std::io;
use std::path::{Component, Path, PathBuf};

use hydrus_core::sort::human_sort;
pub use hydrus_store::paths::{delete_or_recycle, delete_path};

/// `os.path.normpath`, lexically.
fn normpath(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// `has_sidecar_ext`: the extensions sidecars have (case matters).
fn has_sidecar_ext(path: &str) -> bool {
    [".txt", ".json", ".xml"]
        .iter()
        .any(|ext| path.ends_with(ext))
}

/// `get_comparable_sidecar_prefix`: the path up to its name's first dot.
fn comparable_sidecar_prefix(path: &str) -> String {
    let p = Path::new(path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or(path);
    let stem = name.split_once('.').map_or(name, |(stem, _)| stem);
    match p.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(stem).to_string_lossy().into_owned(),
        _ => stem.to_owned(),
    }
}

/// `PopulateComparableSidecarPrefixes`: note the prefixes sidecars of
/// `paths` would have (each that isn't itself sidecar-like's).
pub fn add_sidecar_prefixes<'a>(
    paths: impl IntoIterator<Item = &'a str>,
    prefixes: &mut HashSet<String>,
) {
    for path in paths {
        if !has_sidecar_ext(path) {
            prefixes.insert(comparable_sidecar_prefix(path));
        }
    }
}

/// `LooksLikeSidecarPath`: a `.txt`, `.json` or `.xml` whose name before
/// its first dot is one of `prefixes`.
pub fn looks_like_sidecar(path: &str, prefixes: &HashSet<String>) -> bool {
    has_sidecar_ext(path) && prefixes.contains(&comparable_sidecar_prefix(path))
}

/// `GetAllFilePaths`: the files under `root` (or `root` itself if it is a
/// file), in human order, split into files and the sidecars beside them (a
/// `.txt`, `.json` or `.xml` whose name before its first dot is also some
/// other file's).
pub fn all_file_paths(
    root: &str,
    search_subdirectories: bool,
) -> io::Result<(Vec<String>, Vec<String>)> {
    all_file_paths_noting(root, search_subdirectories, &mut HashSet::new())
}

/// [`all_file_paths`], noting the prefixes its files give sidecars in
/// `prefixes` and taking those already there into account (as the
/// reference's import window does across all it is given).
pub fn all_file_paths_noting(
    root: &str,
    search_subdirectories: bool,
    prefixes: &mut HashSet<String>,
) -> io::Result<(Vec<String>, Vec<String>)> {
    let mut all = Vec::new();
    // (path, the directories above it, to stop following links back up)
    let mut jobs: Vec<(PathBuf, HashSet<PathBuf>)> = vec![(PathBuf::from(root), HashSet::new())];
    while !jobs.is_empty() {
        let mut next = Vec::new();
        for (path, parents) in jobs {
            if !path.is_dir() {
                if let Some(p) = path.to_str() {
                    all.push(p.to_owned());
                }
                continue;
            }
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                let entry_path = entry.path();
                // (following links, as os.scandir's is_dir and is_file do)
                let Ok(meta) = std::fs::metadata(&entry_path) else {
                    continue;
                };
                if meta.is_dir() {
                    if search_subdirectories && !parents.contains(&normpath(&entry_path)) {
                        let mut sub_parents = parents.clone();
                        sub_parents.insert(normpath(&path));
                        next.push((entry_path, sub_parents));
                    }
                } else if meta.is_file()
                    && let Some(p) = entry_path.to_str()
                {
                    all.push(p.to_owned());
                }
            }
        }
        jobs = next;
    }
    human_sort(&mut all);
    add_sidecar_prefixes(all.iter().map(String::as_str), prefixes);
    let (sidecars, files) = all
        .into_iter()
        .partition(|p| looks_like_sidecar(p, prefixes));
    Ok((files, sidecars))
}

/// `FilterOlderModifiedFiles`: the paths last modified before `grace`
/// seconds ago.
pub fn filter_older_modified(paths: Vec<String>, grace: i64, now: i64) -> Vec<String> {
    let only_older_than = (now - grace) as f64;
    paths
        .into_iter()
        .filter(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .is_some_and(|d| d.as_secs_f64() < only_older_than)
        })
        .collect()
}

/// `PathIsFree`: whether nothing else seems to have the file open (on
/// Windows, renaming an open file to itself fails).
pub fn path_is_free(path: &str) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.permissions().readonly() {
        return std::fs::rename(path, path).is_ok();
    }
    std::fs::File::open(path).is_ok()
}

/// `os.path.splitext` on a whole path: `(root, ext)`.
fn split_ext(path: &str) -> (&str, &str) {
    let name_start = path.rfind(std::path::is_separator).map_or(0, |i| i + 1);
    let name = &path[name_start..];
    match name.rfind('.') {
        Some(i) if name[..i].chars().any(|c| c != '.') => {
            (&path[..name_start + i], &path[name_start + i..])
        }
        _ => (path, ""),
    }
}

/// `AppendPathUntilNoConflicts`: `name.jpg`, else `name_0.jpg`,
/// `name_1.jpg`, ...
pub fn append_path_until_no_conflicts(path: &str) -> String {
    let (root, ext) = split_ext(path);
    let mut candidate = path.to_owned();
    let mut i = 0;
    while Path::new(&candidate).exists() {
        candidate = format!("{root}_{i}{ext}");
        i += 1;
    }
    candidate
}

fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        false
    }
}

fn whole_seconds(meta: &std::fs::Metadata) -> Option<i64> {
    let modified = meta.modified().ok()?;
    Some(match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs_f64().ceil() as i64),
    })
}

/// Move a file, across devices too (`shutil.move`).
fn move_file(source: &str, dest: &str) -> io::Result<()> {
    if std::fs::rename(source, dest).is_ok() {
        return Ok(());
    }
    hydrus_store::paths::copy_file(source, dest)?;
    if let Ok(meta) = std::fs::metadata(source)
        && let Ok(modified) = meta.modified()
    {
        let _ = std::fs::File::options()
            .write(true)
            .open(dest)
            .and_then(|f| f.set_modified(modified));
    }
    std::fs::remove_file(source)
}

/// `MergeFile`: move `source` to `dest`, unless `dest` already has the same
/// size and modified second, when `source` is just deleted. Whether a move
/// happened.
pub fn merge_file(source: &str, dest: &str) -> io::Result<bool> {
    let source_meta = std::fs::metadata(source).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            io::Error::other(format!(
                "Cannot file-merge \"{source}\" to \"{dest}\"--the source does not exist!"
            ))
        } else {
            e
        }
    })?;
    if source_meta.is_dir() {
        return Err(io::Error::other(format!(
            "Cannot file-merge \"{source}\" to \"{dest}\"--the source is a directory, not a file!"
        )));
    }
    if let Ok(dest_meta) = std::fs::metadata(dest) {
        if dest_meta.is_dir() {
            return Err(io::Error::other(format!(
                "Cannot file-merge \"{source}\" to \"{dest}\"--the destination is a directory, not a file!"
            )));
        }
        if same_file(&source_meta, &dest_meta) {
            return Err(io::Error::other(format!(
                "Woah, \"{source}\" and \"{dest}\" are the same file!"
            )));
        }
        if source_meta.len() == dest_meta.len()
            && whole_seconds(&source_meta) == whole_seconds(&dest_meta)
        {
            delete_path(source)?;
            return Ok(false);
        }
    }
    move_file(source, dest)?;
    Ok(true)
}

/// `MirrorFile`: copy `source` to `dest` (keeping its modified time)
/// unless `dest` already has the same size and modified second. Whether a
/// copy happened.
pub fn mirror_file(source: &str, dest: &str) -> io::Result<bool> {
    let source_meta = std::fs::metadata(source).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            io::Error::other(format!(
                "Cannot file-mirror \"{source}\" to \"{dest}\"--the source does not exist!"
            ))
        } else {
            e
        }
    })?;
    if source_meta.is_dir() {
        return Err(io::Error::other(format!(
            "Cannot file-mirror \"{source}\" to \"{dest}\"--the source is a directory, not a file!"
        )));
    }
    if let Ok(dest_meta) = std::fs::metadata(dest) {
        if dest_meta.is_dir() {
            return Err(io::Error::other(format!(
                "Cannot file-mirror \"{source}\" to \"{dest}\"--the destination is a directory, not a file!"
            )));
        }
        if same_file(&source_meta, &dest_meta)
            || (source_meta.len() == dest_meta.len()
                && whole_seconds(&source_meta) == whole_seconds(&dest_meta))
        {
            return Ok(false);
        }
        if dest_meta.permissions().readonly() {
            let mut permissions = dest_meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(dest, permissions);
        }
    }
    hydrus_store::paths::copy_file(source, dest)?;
    if let Ok(modified) = source_meta.modified() {
        let _ = std::fs::File::options()
            .write(true)
            .open(dest)
            .and_then(|f| f.set_modified(modified));
    }
    Ok(true)
}

/// `PROCESS_UMASK`'s bits among 0666: read from a new file asked for 0666,
/// rather than by setting the umask, which other threads could see.
#[cfg(unix)]
fn umask() -> u32 {
    use std::os::unix::fs::PermissionsExt;
    static UMASK: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *UMASK.get_or_init(|| {
        tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o666))
            .tempfile()
            .and_then(|f| f.as_file().metadata())
            .map_or(0o022, |m| !m.permissions().mode() & 0o666)
    })
}

/// `TryToGiveFileNicePermissionBits`: make sure the owner can read and
/// write the file and others can read it (0644, less what the umask
/// withholds); nothing in "do not chmod" mode.
pub fn give_nice_permission_bits(path: impl AsRef<Path>) {
    let path = path.as_ref();
    if hydrus_store::paths::do_not_chmod() {
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let bits = meta.permissions().mode();
            let desired = 0o644 & !umask();
            if bits & desired != desired {
                let _ =
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(bits | desired));
            }
        }
    }
    #[cfg(not(unix))]
    {
        if let Ok(meta) = std::fs::metadata(path)
            && meta.permissions().readonly()
        {
            let mut permissions = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
}

/// `GetFileSystemType`: the type of the filesystem `path` is on, as psutil
/// reports it (Linux: from the mount table; the longest mount point that
/// prefixes the path, physical filesystems first). `None` elsewhere.
pub fn file_system_type(path: &str) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let physical: HashSet<String> = std::fs::read_to_string("/proc/filesystems")
        .ok()?
        .lines()
        .filter_map(|line| match line.strip_prefix("nodev") {
            None => Some(line.trim().to_owned()),
            Some(rest) if rest.trim() == "zfs" => Some("zfs".into()),
            Some(_) => None,
        })
        .collect();
    let mounts = std::fs::read_to_string("/proc/self/mounts").ok()?;
    let unescape = |s: &str| -> String {
        s.replace("\\040", " ")
            .replace("\\011", "\t")
            .replace("\\012", "\n")
            .replace("\\134", "\\")
    };
    let entries: Vec<(String, String, String)> = mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let device = fields.next()?.to_owned();
            let mountpoint = unescape(fields.next()?);
            let fstype = fields.next()?.to_owned();
            Some((device, mountpoint, fstype))
        })
        .collect();
    let path = path.to_lowercase();
    for scan_all in [false, true] {
        let mut candidates: Vec<&(String, String, String)> = entries
            .iter()
            .filter(|(device, _, fstype)| {
                scan_all || (!device.is_empty() && device != "none" && physical.contains(fstype))
            })
            .collect();
        candidates.sort_by_key(|(_, mountpoint, _)| std::cmp::Reverse(mountpoint.len()));
        if let Some((_, _, fstype)) = candidates
            .iter()
            .find(|(_, mountpoint, _)| path.starts_with(&mountpoint.to_lowercase()))
        {
            return Some(fstype.clone());
        }
    }
    None
}

/// Whether filenames under `dir` must follow Windows rules (on Windows, or
/// on a filesystem Windows made).
pub fn needs_ntfs_rules(dir: &str, always: bool) -> bool {
    if always {
        return true;
    }
    let Some(fst) = file_system_type(dir) else {
        return false;
    };
    let fst = fst.to_lowercase();
    let fst = fst.strip_prefix("fuse.").unwrap_or(&fst);
    matches!(
        fst,
        "ntfs" | "exfat" | "vfat" | "msdos" | "fat" | "fat32" | "cifs" | "smbfs" | "fuseblk"
    )
}

fn windows_rules(force_ntfs: bool) -> bool {
    cfg!(windows) || force_ntfs
}

const NTFS_DISALLOWED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// `SanitizeFilename`.
pub fn sanitize_filename(name: &str, force_ntfs: bool) -> String {
    if !windows_rules(force_ntfs) {
        return name.replace('/', "_").trim().to_owned();
    }
    let mut clean: String = name
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    while clean.ends_with('.') || clean.ends_with(' ') {
        clean.pop();
    }
    let mut clean = clean.trim().to_owned();
    while NTFS_DISALLOWED.contains(&clean.to_lowercase().as_str()) {
        clean.pop();
    }
    clean
}

fn too_long(name: &str, limit: i64, force_ntfs: bool) -> bool {
    let len = if windows_rules(force_ntfs) {
        name.chars().count()
    } else {
        name.len()
    };
    len as i64 > limit
}

fn pop_char(s: &mut String) {
    s.pop();
}

fn max_path(path_limit: Option<i64>, force_ntfs: bool) -> i64 {
    match path_limit {
        Some(n) => n,
        None if windows_rules(force_ntfs) => 260,
        None if cfg!(target_os = "macos") => 1024,
        None => 4096,
    }
}

fn length(s: &str, force_ntfs: bool) -> i64 {
    if windows_rules(force_ntfs) {
        s.chars().count() as i64
    } else {
        s.len() as i64
    }
}

/// `ElideSubdirsSafely`.
fn elide_subdirs(
    destination: &str,
    subdirs: &str,
    path_limit: Option<i64>,
    dirname_limit: Option<i64>,
    force_ntfs: bool,
) -> Result<String, String> {
    if subdirs.is_empty() {
        return Ok(String::new());
    }
    let max_path = max_path(path_limit, force_ntfs);
    let max_dirname = dirname_limit.unwrap_or(if windows_rules(force_ntfs) { 128 } else { 256 });
    let dirnames: Vec<&str> = subdirs.split(std::path::MAIN_SEPARATOR).collect();
    let n = dirnames.len() as i64;
    // (the reference measures the destination in characters here, even
    // where it counts names in bytes)
    let typical_filename = max_path / 4;
    let left = max_path - destination.chars().count() as i64 - typical_filename - n - 10;
    let per_dirname = left / n;
    if per_dirname < 4 {
        return Err(
            "Sorry, it looks like the combined export filename or directory would be too long! Try shortening the export directory name!".into(),
        );
    }
    let per_dirname = per_dirname.min(max_dirname);
    let mut elided = Vec::new();
    for dirname in dirnames {
        let mut d = sanitize_filename(dirname, force_ntfs);
        if d.is_empty() {
            d = "empty".into();
        }
        while too_long(&d, per_dirname, force_ntfs) {
            pop_char(&mut d);
            d = sanitize_filename(&d, force_ntfs);
        }
        let mut d = d.trim().to_owned();
        if d.is_empty() {
            d = "truncated".into();
        }
        elided.push(d);
    }
    Ok(elided.join(std::path::MAIN_SEPARATOR_STR))
}

/// `ElideFilenameSafely`: `(subdirs, filename)` shortened and cleaned to
/// fit the limits; `ext` is the extension to come.
#[allow(clippy::too_many_arguments)]
pub fn elide_filename(
    destination: &str,
    subdirs: &str,
    base: &str,
    ext: &str,
    path_limit: Option<i64>,
    dirname_limit: Option<i64>,
    filename_limit: i64,
    force_ntfs: bool,
) -> Result<(String, String), String> {
    let too_long_path = || {
        "Sorry, it looks like the combined export filename or directory would be too long! Try shortening the export directory name!".to_owned()
    };
    let base = if base.is_empty() { "empty" } else { base };
    let subdirs = elide_subdirs(destination, subdirs, path_limit, dirname_limit, force_ntfs)?;
    let destination = if subdirs.is_empty() {
        destination.to_owned()
    } else {
        Path::new(destination)
            .join(&subdirs)
            .to_string_lossy()
            .into_owned()
    };
    let mut filename_limit = filename_limit;
    let max_path = max_path(path_limit, force_ntfs);
    if windows_rules(force_ntfs) {
        let max_path = max_path - 10;
        let with_full = destination.chars().count() as i64 + 1 + filename_limit;
        if with_full > max_path {
            filename_limit -= with_full - max_path;
            if filename_limit <= 10 {
                return Err(too_long_path());
            }
        }
    } else {
        let max_path = max_path - 20;
        let with_full = destination.len() as i64 + 1 + filename_limit;
        if with_full > max_path {
            filename_limit -= with_full - max_path;
            if filename_limit <= 18 {
                return Err(too_long_path());
            }
        }
    }
    filename_limit -= length(ext, force_ntfs);
    let too_long_name = || {
        "Sorry, it looks like the export filename would be too long! Try shortening the export phrase or directory!".to_owned()
    };
    if filename_limit <= 0 {
        return Err(too_long_name());
    }
    let mut name = sanitize_filename(base, force_ntfs);
    while too_long(&name, filename_limit, force_ntfs) {
        pop_char(&mut name);
        name = sanitize_filename(&name, force_ntfs);
    }
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(too_long_name());
    }
    Ok((subdirs, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn nice_permission_bits_keep_to_the_umask() {
        use std::os::unix::fs::PermissionsExt;
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let reported = status
            .lines()
            .find_map(|l| l.strip_prefix("Umask:"))
            .map(|v| u32::from_str_radix(v.trim(), 8).unwrap())
            .unwrap();
        assert_eq!(umask(), reported & 0o666);

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o200)).unwrap();
        give_nice_permission_bits(&file);
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o200 | (0o644 & !reported), "umask {reported:o}");
    }

    #[test]
    fn sidecars_are_told_apart_from_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir(d.join("sub")).unwrap();
        for name in [
            "a.jpg",
            "a.jpg.txt",
            "a.json",
            "b.txt",
            "page 10.png",
            "page 2.png",
            "c.TXT",
        ] {
            std::fs::write(d.join(name), b"x").unwrap();
        }
        std::fs::write(d.join("sub").join("x.gif"), b"x").unwrap();
        let (files, sidecars) = all_file_paths(d.to_str().unwrap(), true).unwrap();
        let names = |v: &[String]| -> Vec<String> {
            v.iter()
                .map(|p| {
                    Path::new(p)
                        .strip_prefix(d)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        };
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            names(&files),
            vec![
                "a.jpg",
                "b.txt",
                "c.TXT",
                "page 2.png",
                "page 10.png",
                &format!("sub{sep}x.gif")
            ]
        );
        assert_eq!(names(&sidecars), vec!["a.jpg.txt", "a.json"]);
        let (files, _) = all_file_paths(d.to_str().unwrap(), false).unwrap();
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn conflicting_names_get_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.jpg");
        std::fs::write(&p, b"x").unwrap();
        std::fs::write(dir.path().join("f_0.jpg"), b"x").unwrap();
        let got = append_path_until_no_conflicts(p.to_str().unwrap());
        assert!(got.ends_with("f_1.jpg"), "{got}");
    }
}
