//! Deleting files outside the database: for good, or to the recycle bin
//! as the client's `delete_to_recycle_bin` option asks.

use std::io;
use std::path::Path;

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
