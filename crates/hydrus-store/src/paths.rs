//! Copying and deleting files outside the database: deleting for good, or
//! to the recycle bin as the client's `delete_to_recycle_bin` option asks;
//! copying with or without the file's permissions, as its
//! `do_not_do_chmod_mode` asks.

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// `HydrusPaths.DO_NOT_DO_CHMOD_MODE`, which, as in the reference, holds
/// for the whole process.
static DO_NOT_CHMOD: AtomicBool = AtomicBool::new(false);

/// Leave files' permissions alone (for filesystems that refuse them).
pub fn set_do_not_chmod(on: bool) {
    DO_NOT_CHMOD.store(on, Ordering::Relaxed);
}

/// Whether files' permissions are left alone.
pub fn do_not_chmod() -> bool {
    DO_NOT_CHMOD.load(Ordering::Relaxed)
}

/// Copy a file's contents, and its permission bits unless they are to be
/// left alone (`safe_copy2`'s copy: `shutil.copyfile` in "do not chmod"
/// mode, where a filesystem that refuses permissions would fail the
/// copy); how many bytes.
pub fn copy_file(source: impl AsRef<Path>, dest: impl AsRef<Path>) -> io::Result<u64> {
    if do_not_chmod() {
        let mut from = std::fs::File::open(source)?;
        let mut to = std::fs::File::create(dest)?;
        io::copy(&mut from, &mut to)
    } else {
        std::fs::copy(source, dest)
    }
}

/// `HydrusPaths.DeletePath`: delete a file (a link itself, not its target)
/// for good; nothing if it isn't there.
pub fn delete_path(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
    }
}

/// `ClientPaths.DeletePath`: to the recycle bin if asked (falling back to
/// deleting for good, as the reference does when recycling fails), else
/// for good.
pub fn delete_or_recycle(path: impl AsRef<Path>, recycle: bool) -> io::Result<()> {
    let path = path.as_ref();
    if !recycle {
        return delete_path(path);
    }
    if std::fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    match ::trash::delete(path) {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::warn!(
                "could not recycle {} ({e}); deleting it instead",
                path.display()
            );
            delete_path(path)
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn copies_keep_permissions_unless_told_not_to() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::write(&source, b"contents").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o604)).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;

        let kept = dir.path().join("kept");
        assert_eq!(copy_file(&source, &kept).unwrap(), 8);
        assert_eq!(mode(&kept), 0o604);

        set_do_not_chmod(true);
        let left = dir.path().join("left");
        let copied = copy_file(&source, &left);
        set_do_not_chmod(false);
        assert_eq!(copied.unwrap(), 8);
        assert_eq!(std::fs::read(&left).unwrap(), b"contents");
        assert_ne!(mode(&left), 0o604, "a new file's own permissions");
    }
}
