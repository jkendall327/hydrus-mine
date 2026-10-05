//! A pre-existing importer and its maintenance tools consume live saved ICC.
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::{
    Store,
    image_colour::{self, ImageColour},
    settings,
};
#[test]
fn actual_import_pixels_and_generated_thumbnails_follow_saved_policy_without_recreating_importer() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let fixture = hydrus_testkit::fixture_json("image_decoder_policies.json");
    let path = hydrus_testkit::fixture_path("image_decoder_policies/embedded-linear.png");
    let imported = importer
        .import_path(&path, &FileImportOptions::default())
        .unwrap();
    let hash = imported.hash.unwrap();
    let id = store
        .read(|c| hydrus_store::master::hash_id(c, &hash))
        .unwrap()
        .unwrap();
    let info = store
        .read(|c| hydrus_store::media::load_basic(c, &[id]))
        .unwrap()
        .remove(0)
        .info
        .unwrap();
    let expected = fixture["cases"][0]["decode"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["file"] == "embedded-linear.png")
        .unwrap();
    assert_eq!(info.pixel_hash.unwrap().to_hex(), expected["pixel_sha256"]);
    let original_hash = info.pixel_hash;
    for case in fixture["cases"].as_array().unwrap() {
        let enabled = case["icc"].as_bool().unwrap();
        store
            .write_and_refresh(move |c| {
                settings::set(
                    c.conn(),
                    &ImageColour {
                        normalise_icc: enabled,
                    },
                )
            })
            .unwrap();
        assert_eq!(
            Store::open(store.dir())
                .unwrap()
                .read(image_colour::load)
                .unwrap()
                .normalise_icc,
            enabled
        );
        let want = case["decode"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["file"] == "embedded-linear.png")
            .unwrap();
        assert_eq!(
            importer
                .tools()
                .load_image(&path, hydrus_core::Mime::ImagePng)
                .unwrap()
                .data(),
            serde_json::from_value::<Vec<u8>>(want["pixels"].clone()).unwrap()
        );
        // Existing content-derived records stay stable; only future decoding
        // and explicit thumbnail regeneration consume the changed policy.
        assert_eq!(
            store
                .read(|c| hydrus_store::media::load_basic(c, &[id]))
                .unwrap()[0]
                .info
                .as_ref()
                .unwrap()
                .pixel_hash,
            original_hash
        );
        let snapshot = store.snapshot();
        let media = store
            .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[id]))
            .unwrap()
            .results
            .remove(0);
        let thumb = importer.regenerate_thumbnail(&media).unwrap().unwrap();
        let mime = importer.tools().detect_mime(&thumb).unwrap();
        let info = importer.tools().inspect_as(&thumb, mime).unwrap();
        assert!(!importer.tools().flags(&thumb, &info).has_icc_profile);
        let once = hydrus_media::decode_image(&std::fs::read(&thumb).unwrap()).unwrap();
        let other =
            hydrus_media::decode_image_with_icc(&std::fs::read(&thumb).unwrap(), false).unwrap();
        assert_eq!(
            once, other,
            "encoded stored thumbnail has no policy-dependent metadata"
        );
    }
    store
        .write_and_refresh(|c| {
            settings::set(
                c.conn(),
                &ImageColour {
                    normalise_icc: false,
                },
            )
        })
        .unwrap();
    let next = hydrus_testkit::fixture_path("image_decoder_policies/embedded-linear.jpg");
    let imported = importer
        .import_path(&next, &FileImportOptions::default())
        .unwrap();
    assert_eq!(
        imported.status,
        hydrus_import::ImportStatus::SuccessfulAndNew
    );
    let id = store
        .read(|c| hydrus_store::master::hash_id(c, &imported.hash.unwrap()))
        .unwrap()
        .unwrap();
    let info = store
        .read(|c| hydrus_store::media::load_basic(c, &[id]))
        .unwrap()
        .remove(0)
        .info
        .unwrap();
    let want = fixture["cases"][1]["decode"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["file"] == "embedded-linear.jpg")
        .unwrap();
    assert_eq!(
        info.pixel_hash.unwrap().to_hex(),
        want["pixel_sha256"],
        "future actual import must consume the saved disabled policy"
    );
}
