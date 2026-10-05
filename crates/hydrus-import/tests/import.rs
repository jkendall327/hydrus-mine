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
fn saved_precision_reaches_actual_import_rejection_without_importing_or_changing_rules() {
    let fixture = hydrus_testkit::fixture_json("gui_format_backend.json");
    let w = world();
    let path = media_dir().join("png_rgba.png");
    let info = MediaTools::new().inspect(&path).unwrap();
    let rules = FileImportOptions {
        min_size: Some(243_200),
        ..Default::default()
    };
    for event in fixture["events"].as_array().unwrap() {
        let formatting: hydrus_store::settings::GuiFormatting =
            serde_json::from_value(event["saved"].clone()).unwrap();
        w.store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &formatting))
            .unwrap();
        let reopened = Store::open(w.store.dir()).unwrap();
        let saved: hydrus_store::settings::GuiFormatting =
            reopened.read(hydrus_store::settings::get).unwrap();
        let expected = rules.check_with_figures(&info, saved.figures).unwrap_err();
        let result = w.importer.import_path(&path, &rules).unwrap();
        assert_eq!(result.status, ImportStatus::Vetoed);
        assert_eq!(result.note, expected);
        assert_eq!(rules.min_size, Some(243_200));
        assert!(
            w.store
                .read(|conn| hydrus_store::master::hash_id(conn, &result.hash.unwrap()))
                .unwrap()
                .is_none()
        );
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
        // (a video's frames come from ffmpeg, which decodes differently
        // between versions)
        let from_ffmpeg = hydrus_media::mimes::is_video(info.mime);
        if let Some(thumbnail) = want["thumbnail"].as_object()
            && (!from_ffmpeg || hydrus_testkit::recording_ffmpeg())
        {
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

#[test]
fn a_missing_thumbnail_is_made_again_from_its_file() {
    let w = world();
    let options = FileImportOptions::default();
    let hash = w
        .importer
        .import_path(&media_dir().join("png_rgba.png"), &options)
        .unwrap()
        .hash
        .unwrap();
    let path = w.store.snapshot().storage.thumbnail_path(&hash).unwrap();
    let original = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();

    let made = w.importer.regenerate_thumbnail(&w.load(&hash)).unwrap();
    assert_eq!(made.as_deref(), Some(path.as_path()));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "the same thumbnail"
    );

    // a file that is no longer stored can't give one
    std::fs::remove_file(&path).unwrap();
    let id = w.id(&hash);
    w.store
        .write_content(move |c| {
            let storage = c.roles().local_file_storage;
            c.delete_files(storage, &[id], None)
        })
        .unwrap();
    assert!(w.importer.regenerate_thumbnail(&w.load(&hash)).is_err());
    assert!(!path.exists());
}

#[test]
fn imports_record_xmp_iptc_and_software_flags() {
    let w = world();
    let options = FileImportOptions::default();
    let dir = hydrus_testkit::fixture_path("metadata");
    for (file, flag) in [
        ("jpeg_xmp.jpg", FileFlags::XMP),
        ("jpeg_iptc_keywords.jpg", FileFlags::IPTC),
        ("png_creator.png", FileFlags::SOFTWARE_SOURCE),
    ] {
        let hash = w
            .importer
            .import_path(&dir.join(file), &options)
            .unwrap()
            .hash
            .unwrap();
        let flags = w.load(&hash).info.unwrap().flags;
        assert!(flags.has(flag), "{file}: {flags:?}");
    }
}

#[test]
fn a_failed_copy_into_storage_pauses_the_importers() {
    use hydrus_store::settings::{FolderSettings, Pauses};
    let w = world();
    let path = media_dir().join("png_rgb.png");
    let hash = hydrus_media::hash_file(&path).unwrap().sha256;
    let dest = w
        .store
        .snapshot()
        .storage
        .file_path(&hash, hydrus_core::Mime::ImagePng)
        .unwrap();
    // a file where its folder should be
    let folder = dest.parent().unwrap();
    std::fs::create_dir_all(folder.parent().unwrap()).unwrap();
    std::fs::write(folder, b"in the way").unwrap();

    let result = w.importer.import_path(&path, &FileImportOptions::default());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("failed"), "{error}");
    let (pauses, folders): (Pauses, FolderSettings) = w
        .store
        .read(|c| {
            Ok((
                hydrus_store::settings::get(c)?,
                hydrus_store::settings::get(c)?,
            ))
        })
        .unwrap();
    assert!(pauses.subscriptions && pauses.file_queues, "{pauses:?}");
    assert!(folders.pause_import_folders);
    assert!(!pauses.network_traffic, "only the importers");
}

#[test]
fn decompression_bombs_are_vetoed_when_the_options_say_so() {
    // a PNG claiming 20000x20000 pixels, past Pillow's limit
    let chunk = |kind: &[u8], body: &[u8]| {
        let mut out = (body.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(body);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
        out
    };
    let mut ihdr = 20000u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&20000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(
        b"IDAT",
        &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
    ));
    png.extend(chunk(b"IEND", b""));
    let w = world();
    let options = FileImportOptions {
        allow_decompression_bombs: false,
        ..FileImportOptions::default()
    };
    let result = w.importer.import_bytes(&png, &options).unwrap();
    assert_eq!(result.status, ImportStatus::Vetoed);
    assert_eq!(result.note, "Image seems to be a Decompression Bomb!");
}

/// CRC-32 (IEEE), as PNG chunks carry.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn a_new_file_with_no_import_destination_is_vetoed() {
    use hydrus_core::import_options::{CallerType, ImportOptionsManager, NO_IMPORT_DESTINATION};
    let w = world();
    let path = media_dir().join("png_rgb.png");
    // options whose only destination has since been deleted
    let mut full = ImportOptionsManager::default().full(CallerType::ClientApi, None, &[]);
    full.locations.destinations = vec!["ab".repeat(32)];
    let options = FileImportOptions::from_full(&full, &w.store.snapshot().services);
    let result = w.importer.import_path(&path, &options).unwrap();
    assert_eq!(result.status, ImportStatus::Vetoed);
    assert_eq!(result.note, NO_IMPORT_DESTINATION);
    assert_eq!(result.raised.as_deref(), Some(NO_IMPORT_DESTINATION));
    let hash = result.hash.unwrap();
    let stored = w
        .store
        .snapshot()
        .storage
        .file_path(&hash, result.mime.unwrap());
    assert!(!stored.unwrap().exists(), "nothing reaches storage");

    // with somewhere to go, it imports
    let result = w
        .importer
        .import_path(&path, &FileImportOptions::default())
        .unwrap();
    assert_eq!(result.status, ImportStatus::SuccessfulAndNew);
    // and a file the client has is only "already in db", as in the reference
    let result = w.importer.import_path(&path, &options).unwrap();
    assert_eq!(result.status, ImportStatus::SuccessfulButRedundant);
}
