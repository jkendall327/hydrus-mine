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

/// The repository's `oracle/fixtures` directory. `HYDRUS_FIXTURE_DIR` can
/// select the current checkout when worktrees share cached test helpers.
pub fn fixtures_dir() -> PathBuf {
    std::env::var_os("HYDRUS_FIXTURE_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/fixtures"),
        PathBuf::from,
    )
}

/// Path of a file under `oracle/fixtures`.
pub fn fixture_path(relative: impl AsRef<Path>) -> PathBuf {
    fixtures_dir().join(relative)
}

/// Parse a JSON fixture under `oracle/fixtures`, preserving recorded float values.
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

    #[test]
    fn runtime_fixture_override() {
        // A subprocess gives the override its own environment, without
        // changing the fixture directory used by concurrently running tests.
        if std::env::var_os("HYDRUS_FIXTURE_OVERRIDE_CHILD").is_some() {
            assert_eq!(
                fixture_json("worktree-only.json")["worktree"],
                "other checkout"
            );
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("worktree-only.json"),
            r#"{"worktree":"other checkout"}"#,
        )
        .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::runtime_fixture_override"])
            .env("HYDRUS_FIXTURE_DIR", directory.path())
            .env("HYDRUS_FIXTURE_OVERRIDE_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}

/// `oracle/record_trash_maintenance.py`'s `bmp()`, which several recorders
/// import: a 64x64 24-bit BMP of exactly `size` bytes, its
/// pixel data starting as late as the size needs.
pub fn bmp(seed: u32, size: usize) -> Vec<u8> {
    let (width, height) = (64u32, 64u32);
    let row = width as usize * 3;
    let pixels: Vec<u8> = (0..row * height as usize)
        .map(|i| (((seed as usize * 31 + i * 7) ^ (i >> 8)) & 0xFF) as u8)
        .collect();
    let offset = size - pixels.len();
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    for value in [40u32, width, height] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    for value in [0u32, pixels.len() as u32, 2835, 2835, 0, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.resize(offset, 0);
    out.extend_from_slice(&pixels);
    assert_eq!(out.len(), size);
    out
}
