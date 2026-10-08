//! A file imported with several import destinations lands in each of them
//! (the reference's `LocationImportOptions` destination location context).

use hydrus_core::import_options::{CallerType, ImportOptionsManager};
use hydrus_core::service::{ServiceType, builtin_keys};
use hydrus_import::{FileImportOptions, FileImporter, ImportStatus};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::media;

#[test]
fn a_new_file_goes_to_every_import_destination() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let snapshot = store.snapshot();
    let domains: Vec<_> = snapshot
        .services
        .of_type(ServiceType::LocalFileDomain)
        .collect();
    assert!(domains.len() >= 2, "the fixture has two local file domains");
    let my_files_key = hydrus_core::ServiceKey::new(builtin_keys::MY_FILES.to_vec());
    let art = domains
        .iter()
        .find(|s| s.name == "art")
        .expect("the art domain");
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let path = hydrus_testkit::fixture_path("media").join("png_rgb.png");
    let mut full = ImportOptionsManager::default().full(CallerType::ClientApi, None, &[]);
    full.locations.destinations = vec![my_files_key.to_hex(), art.key.to_hex()];
    let options = FileImportOptions::from_full(&full, &snapshot.services);
    let result = importer.import_path(&path, &options).unwrap();
    assert_eq!(result.status, ImportStatus::SuccessfulAndNew);
    let hash = result.hash.unwrap();
    let id = store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    let current = store
        .read(|c| media::current_domains(c, &[id]))
        .unwrap()
        .remove(&id)
        .unwrap_or_default();
    let my_files_id = snapshot.services.by_key(&my_files_key).unwrap().id;
    assert!(current.contains(&my_files_id), "in my files: {current:?}");
    assert!(current.contains(&art.id), "in art: {current:?}");
}
