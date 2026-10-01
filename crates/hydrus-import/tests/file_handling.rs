//! An importer applies its store's file handling settings to the process
//! (as the reference applies its options at boot). They hold for the whole
//! process, so this is tested alone here.

use std::sync::Arc;

use hydrus_core::Mime;
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::settings::FileHandlingSettings;

#[test]
fn an_importer_applies_the_store_s_file_handling() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let cbz = hydrus_testkit::fixture_path("media/cbz_flat.cbz");
    let import = |store: &Arc<Store>| {
        FileImporter::new(Arc::clone(store), MediaTools::new())
            .import_path(&cbz, &FileImportOptions::default())
            .unwrap()
            .mime
    };
    assert_eq!(import(&store), Some(Mime::ApplicationCbz));

    // as a migration brings over `allow_comic_book_archive_detection` off
    let settings = FileHandlingSettings {
        comic_book_detection: false,
        ..FileHandlingSettings::default()
    };
    // (a fresh store: the first has the file already)
    let other = tempfile::tempdir().unwrap();
    let fresh = Store::open(other.path()).unwrap();
    fresh
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings))
        .unwrap();
    assert_eq!(import(&fresh), Some(Mime::ApplicationZip));
}
