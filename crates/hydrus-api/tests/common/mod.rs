//! Test fixtures: reference databases imported into native stores.

// each test binary uses its own subset
#![allow(dead_code)]

use std::sync::Arc;

use hydrus_api::AppState;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

/// An imported fixture: the unpacked reference db (whose client_files the
/// native store points at), and the native store with its API state.
pub struct Fixture {
    pub legacy_dir: tempfile::TempDir,
    /// Kept alive for the store's lifetime.
    pub _native_dir: tempfile::TempDir,
    pub state: Arc<AppState>,
}

/// Import the reference fixture database `name` exactly as a user's install
/// would be imported.
pub fn imported_store(name: &str) -> Fixture {
    let legacy_dir = hydrus_testkit::legacy_fixture(name);
    let native_dir = tempfile::tempdir().unwrap();
    import_legacy(
        legacy_dir.path(),
        &native_dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native_dir.path()).unwrap();
    let state = AppState::new(store).unwrap();
    Fixture {
        legacy_dir,
        _native_dir: native_dir,
        state,
    }
}
