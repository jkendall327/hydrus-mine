//! Importing into a fresh store, checked against what the reference records
//! for the same files (`oracle/fixtures/media.json`).

use std::path::PathBuf;
use std::sync::Arc;

use hydrus_core::{HashId, Sha256};
use hydrus_import::{FileImportOptions, FileImporter, ImportStatus};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::media::{self, FileFlags};
use serde_json::Value;

fn media_dir() -> PathBuf {
    hydrus_testkit::fixture_path("media")
}

fn expected(file: &str) -> Value {
    let all = hydrus_testkit::fixture_json("media.json");
    all["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["file"] == file)
        .unwrap_or_else(|| panic!("{file} not in media.json"))
        .clone()
}

struct World {
    _dir: tempfile::TempDir,
    store: Arc<Store>,
    importer: FileImporter,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    World {
        _dir: dir,
        store,
        importer,
    }
}

impl World {
    fn id(&self, hash: &Sha256) -> HashId {
        self.store
            .read(|c| hydrus_store::master::hash_id(c, hash))
            .unwrap()
            .unwrap()
    }

    fn load(&self, hash: &Sha256) -> media::MediaResult {
        let snap = self.store.snapshot();
        let id = self.id(hash);
        self.store
            .read(|c| media::load(c, &snap.services, Some(&snap.display), &[id]))
            .unwrap()
            .results
            .remove(0)
    }
}

#[test]
fn imports_match_what_the_reference_records() {
    let w = world();
    for file in [
        "png_rgba.png",
        "png_rgb.png",
        "jpeg_exif_software.jpg",
        "gif_anim.gif",
        "mp4_h264_aac.mp4",
    ] {
        let want = expected(file);
        let result = w
            .importer
            .import_path(&media_dir().join(file), &FileImportOptions::default())
            .unwrap();
        assert_eq!(
            result.status,
            ImportStatus::SuccessfulAndNew,
            "{file}: {}",
            result.note
        );
        let hash = result.hash.unwrap();
        assert_eq!(hash.to_hex(), want["hashes"]["sha256"], "{file}");

        let m = w.load(&hash);
        let info = m.info.as_ref().unwrap();
        let i = &want["info"];
        assert_eq!(
            u64::from(info.mime.code()),
            want["mime"].as_u64().unwrap(),
            "{file}"
        );
        assert_eq!(info.size, i["size"].as_u64().unwrap(), "{file}");
        assert_eq!(info.width.map(u64::from), i["width"].as_u64(), "{file}");
        assert_eq!(info.height.map(u64::from), i["height"].as_u64(), "{file}");
        assert_eq!(info.duration_ms, i["duration_ms"].as_u64(), "{file}");
        assert_eq!(info.num_frames, i["num_frames"].as_u64(), "{file}");
        assert_eq!(
            info.flags.has(FileFlags::TRANSPARENCY),
            want["has_transparency"].as_bool().unwrap(),
            "{file}"
        );
        assert_eq!(
            info.pixel_hash.map(|h| h.to_hex()),
            want["pixel_hash"].as_str().map(str::to_owned),
            "{file}"
        );
        if let Some(thumbnail) = want["thumbnail"].as_object() {
            assert_eq!(
                info.blurhash.as_deref(),
                thumbnail["blurhash"].as_str(),
                "{file}"
            );
        }
        // stored, with a thumbnail, in the inbox, in my files and the umbrellas
        let snap = w.store.snapshot();
        assert!(
            snap.storage.file_path(&hash, info.mime).unwrap().is_file(),
            "{file}"
        );
        assert!(
            snap.storage.thumbnail_path(&hash).unwrap().is_file(),
            "{file}"
        );
        assert!(m.inbox, "{file}");
        assert_eq!(
            m.current.len(),
            3,
            "{file}: my files, combined local media, local storage"
        );
        let phashes: Vec<String> = w
            .store
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT hex(p.phash) FROM file_perceptual_hashes f JOIN perceptual_hashes p USING (phash_id)
                     WHERE f.hash_id = ?1",
                )?;
                Ok(stmt
                    .query_map([m.hash_id], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .unwrap();
        let want_phashes: Vec<String> = want["perceptual_hashes"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|p| p.as_str().unwrap().to_uppercase())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(phashes, want_phashes, "{file}");
    }
}

#[test]
fn reimports_report_what_the_client_knows() {
    let w = world();
    let path = media_dir().join("png_rgb.png");
    let options = FileImportOptions::default();
    let first = w.importer.import_path(&path, &options).unwrap();
    assert_eq!(first.status, ImportStatus::SuccessfulAndNew);

    let again = w.importer.import_path(&path, &options).unwrap();
    assert_eq!(again.status, ImportStatus::SuccessfulButRedundant);
    assert!(
        again.note.starts_with("file recognised: Imported at "),
        "{}",
        again.note
    );

    // deleted for good: not re-imported by default
    let id = w.id(&first.hash.unwrap());
    w.store
        .write_content(move |c| {
            let storage = c.roles().local_file_storage;
            c.delete_files(storage, &[id], Some("testing"))
        })
        .unwrap();
    let deleted = w.importer.import_path(&path, &options).unwrap();
    assert_eq!(deleted.status, ImportStatus::Deleted);
    assert!(
        deleted
            .note
            .starts_with("file recognised: Deleted from the client "),
        "{}",
        deleted.note
    );
    assert!(deleted.note.contains("(testing)"), "{}", deleted.note);

    // unless the options allow it
    let options = FileImportOptions {
        exclude_deleted: false,
        ..FileImportOptions::default()
    };
    let back = w.importer.import_path(&path, &options).unwrap();
    assert_eq!(back.status, ImportStatus::SuccessfulAndNew);
}

#[test]
fn rules_veto_and_bad_files_error() {
    let w = world();
    let small = FileImportOptions {
        max_size: Some(1024),
        ..FileImportOptions::default()
    };
    let vetoed = w
        .importer
        .import_path(&media_dir().join("png_rgb.png"), &small)
        .unwrap();
    assert_eq!(vetoed.status, ImportStatus::Vetoed);
    assert_eq!(
        vetoed.note,
        "File was 104 KB but the upper limit in the File Filtering Import Options is 1 KB."
    );

    let junk = w
        .importer
        .import_bytes(b"this is not a picture", &FileImportOptions::default())
        .unwrap();
    assert_eq!(junk.status, ImportStatus::Error, "{}", junk.note);
    assert!(!junk.note.is_empty());
}
