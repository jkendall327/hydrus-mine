//! Test-only helpers shared by the hydrus crates.
//!
//! Everything here is for tests and benchmarks: helpers panic with a
//! descriptive message instead of returning errors, because a missing or
//! corrupt fixture is a broken checkout, not a condition to handle.
//!
//! Fixtures live in `oracle/fixtures/` and are produced by the scripts in
//! `oracle/` from the reference implementation (see `oracle/README.md`).

use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use tempfile::TempDir;

/// The repository's `oracle/fixtures` directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/fixtures")
}

/// Path of a file under `oracle/fixtures`.
pub fn fixture_path(relative: impl AsRef<Path>) -> PathBuf {
    fixtures_dir().join(relative)
}

/// Parse a JSON fixture under `oracle/fixtures`.
pub fn fixture_json(relative: impl AsRef<Path>) -> serde_json::Value {
    let path = fixture_path(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("fixture {} is not valid json: {e}", path.display()))
}

/// The ffmpeg the media fixtures were recorded with: Ubuntu 24.04's, on x86-64.
pub const RECORDED_FFMPEG: &str = "ffmpeg version 6.1.1-";

/// Whether the `ffmpeg` on `PATH` is the one the fixtures were recorded
/// with. Other versions and builds (and ffmpeg's ARM code) decode frames and
/// report durations a little differently, for the reference as for us (it
/// renders the same files through ffmpeg), so tests only compare values
/// ffmpeg produces when this holds.
pub fn recording_ffmpeg() -> bool {
    let Ok(out) = std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .output()
    else {
        return false;
    };
    cfg!(target_arch = "x86_64") && out.stdout.starts_with(RECORDED_FFMPEG.as_bytes())
}

/// Extract the reference database fixture `oracle/fixtures/legacy_db/<name>.tar.gz`
/// into a fresh temporary directory, which is the database directory
/// (`client.db`, `client.*.db`, `client_files/`).
///
/// The directory is deleted when the returned guard is dropped. Every call
/// extracts a new copy, so tests may modify it freely.
pub fn legacy_fixture(name: &str) -> TempDir {
    let archive = fixture_path(format!("legacy_db/{name}.tar.gz"));
    let file = File::open(&archive)
        .unwrap_or_else(|e| panic!("opening fixture {}: {e}", archive.display()));
    let dir = tempfile::Builder::new()
        .prefix(&format!("hydrus-fixture-{name}-"))
        .tempdir()
        .expect("creating a temporary directory");
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    tar.set_preserve_permissions(false);
    tar.unpack(dir.path())
        .unwrap_or_else(|e| panic!("extracting fixture {}: {e}", archive.display()));
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_fixture_extracts_with_empty_subfolders() {
        let dir = legacy_fixture("basic");
        for db in [
            "client.db",
            "client.caches.db",
            "client.mappings.db",
            "client.master.db",
        ] {
            assert!(dir.path().join(db).is_file(), "{db} missing");
        }
        // git would not keep empty directories, but the tarball does
        let subfolders = std::fs::read_dir(dir.path().join("client_files"))
            .unwrap()
            .count();
        assert_eq!(subfolders, 512);
    }

    #[test]
    fn each_call_gets_its_own_copy() {
        let a = legacy_fixture("basic");
        let b = legacy_fixture("basic");
        assert_ne!(a.path(), b.path());
    }
}
