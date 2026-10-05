//! Actual Qt's file/block prefixes, popup timing, copy metadata and queue delay.
use hydrus_core::{Mime, Sha256};
use hydrus_store::{
    Store,
    metadata_jobs::{self, Effects, File, Request},
    popups,
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{
    fs, io,
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

struct Script<'a> {
    store: &'a Arc<Store>,
    now: Cell<i64>,
    speed: i64,
    cancel_at: Option<usize>,
    shutdown: &'a AtomicBool,
    fallback: bool,
    progress: RefCell<Vec<usize>>,
    published: RefCell<Vec<Value>>,
}
impl Effects for Script<'_> {
    fn now(&self) -> i64 {
        self.now.get()
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fallback {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "scripted rename unavailable",
            ))
        } else {
            fs::rename(from, to)
        }
    }
    fn before_step(&self, index: usize, key: &[u8; 32]) {
        self.progress.borrow_mut().push(index);
        self.now
            .set(1000 + i64::try_from(index).unwrap() * self.speed);
        if self.cancel_at.is_some_and(|at| index >= at) {
            let cancelled_existing = self
                .store
                .write({
                    let key = *key;
                    let now = self.now();
                    move |ctx| popups::update(ctx.conn(), &key, now, |job| job.cancel())
                })
                .unwrap()
                .is_some();
            self.shutdown.store(!cancelled_existing, Ordering::Release);
        }
    }
    fn published(&self, job: &popups::Job) {
        if self.shutdown.swap(false, Ordering::AcqRel) {
            let key = job.key;
            let now = self.now();
            self.store
                .write(move |ctx| {
                    popups::update(ctx.conn(), &key, now, |job| job.cancel())?;
                    Ok(())
                })
                .unwrap();
        }
        self.published.borrow_mut().push(json!({"title":job.status_title,"at":self.now(),"gauge":job.popup_gauge_1.map(|(a,b)|vec![a,b])}));
    }
}
fn files(store: &Arc<Store>, count: usize) -> Vec<File> {
    let info = hydrus_store::media::FileInfo {
        size: 22,
        mime: Mime::ImageJpeg,
        original_mime: None,
        width: None,
        height: None,
        duration_ms: None,
        num_frames: None,
        has_audio: false,
        num_words: None,
        file_modified: None,
        pixel_hash: None,
        blurhash: None,
        flags: Default::default(),
    };
    let files = store
        .write_content(move |writer| {
            (0..count)
                .map(|index| {
                    let hash = Sha256([u8::try_from(index).unwrap(); 32]);
                    let id = hydrus_store::master::intern_hash(writer.conn(), &hash)?;
                    writer.add_file_info(id, &info, false)?;
                    Ok(File {
                        id,
                        hash,
                        mime: Mime::ImageJpeg,
                        original_mime: Mime::ImageJpeg,
                    })
                })
                .collect::<hydrus_store::Result<Vec<_>>>()
        })
        .unwrap();
    for file in &files {
        let path = store
            .snapshot()
            .storage
            .file_path(&file.hash, file.mime)
            .unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"owned metadata fixture").unwrap();
        let time = UNIX_EPOCH + Duration::from_secs(1234567890);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(time).set_accessed(time))
            .unwrap();
    }
    files
}
#[test]
fn actual_qt_worker_inputs_replay_files_cancellation_progress_and_copy_cleanup() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../oracle/fixtures/metadata_file_jobs.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let files = files(
            &store,
            usize::try_from(case["count"].as_u64().unwrap()).unwrap(),
        );
        let shutdown = AtomicBool::new(false);
        let effects = Script {
            store: &store,
            now: Cell::new(1000),
            speed: case["seconds_per_file"].as_i64().unwrap(),
            cancel_at: case["cancel_at"]
                .as_u64()
                .map(|n| usize::try_from(n).unwrap()),
            shutdown: &shutdown,
            fallback: case["fallback"].as_bool().unwrap(),
            progress: RefCell::default(),
            published: RefCell::default(),
        };
        let force = case["kind"] == "force";
        let request = if force {
            Request::Force {
                files: files.clone(),
                mime: Some(Mime::ImagePng),
            }
        } else {
            Request::Modified {
                files: files.clone(),
                milliseconds: 1700000000123,
                step: 250,
            }
        };
        if !force {
            let ids: Vec<_> = files.iter().map(|file| file.id).collect();
            store
                .write_content(move |writer| {
                    for (index, id) in ids.into_iter().enumerate() {
                        writer.set_file_time(
                            &[id],
                            &hydrus_store::content::FileTime::FileModified,
                            1700000000123 + i64::try_from(index).unwrap() * 250,
                        )?;
                    }
                    Ok(())
                })
                .unwrap();
        }
        metadata_jobs::run(&store, &request, &shutdown, &effects).unwrap();
        assert_eq!(
            *effects.published.borrow(),
            *case["published"].as_array().unwrap(),
            "{case}"
        );
        let expected: Vec<usize> = case["progress"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| usize::try_from(p["gauge"][0].as_u64().unwrap()).unwrap())
            .collect();
        assert_eq!(*effects.progress.borrow(), expected);
        for (index, file) in files.iter().enumerate() {
            let source = store
                .snapshot()
                .storage
                .file_path(&file.hash, file.mime)
                .unwrap();
            let destination = store
                .snapshot()
                .storage
                .file_path(&file.hash, Mime::ImagePng)
                .unwrap();
            let modified = source.metadata().ok().map(|m| {
                i64::try_from(
                    m.modified()
                        .unwrap()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis(),
                )
                .unwrap()
            });
            assert_eq!(json!(modified), case["files"][index]["modified_ms"]);
            assert_eq!(
                source.exists(),
                case["files"][index]["source"].as_bool().unwrap()
            );
            assert_eq!(
                destination.exists(),
                case["files"][index]["destination"].as_bool().unwrap()
            );
            if destination.exists() {
                assert_eq!(fs::read(&destination).unwrap(), b"owned metadata fixture");
                assert_eq!(
                    destination.metadata().unwrap().modified().unwrap(),
                    UNIX_EPOCH + Duration::from_secs(1234567890)
                );
            }
            if source.exists() && !effects.fallback {
                assert_eq!(
                    source.metadata().unwrap().accessed().unwrap(),
                    UNIX_EPOCH + Duration::from_secs(1234567890)
                );
            }
            if !force {
                let database_ms: i64 = store
                    .read(|c| {
                        Ok(c.query_row(
                            "SELECT file_modified_ms FROM files WHERE hash_id=?1",
                            [file.id],
                            |row| row.get(0),
                        )?)
                    })
                    .unwrap();
                assert_eq!(
                    database_ms,
                    1700000000123 + i64::try_from(index).unwrap() * 250,
                    "disk cancellation never rolls back DB changes"
                );
            }
            let forced: Option<u8> = store
                .read(|c| {
                    Ok(c.query_row(
                        "SELECT forced_mime FROM files WHERE hash_id=?1",
                        [file.id],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
            assert_eq!(forced.is_some(), force && destination.exists());
            let scheduled: Option<i64> = store.read(|c| { use rusqlite::OptionalExtension as _; Ok(c.query_row("SELECT time_can_start FROM file_maintenance_jobs WHERE hash_id=?1 AND job_type=4", [file.id], |r| r.get(0)).optional()?) }).unwrap();
            assert_eq!(scheduled, if effects.fallback { Some(4600) } else { None });
        }
        assert!(
            store
                .read(|c| popups::all(c, effects.now()))
                .unwrap()
                .is_empty()
        );
        if effects.fallback {
            assert!(
                store
                    .read(|c| hydrus_store::file_maintenance::due_jobs_of(c, 4600, &|_| true))
                    .unwrap()
                    .is_empty()
            );
            let due = store
                .read(|c| hydrus_store::file_maintenance::due_jobs_of(c, 4601, &|_| true))
                .unwrap();
            assert_eq!(
                due,
                vec![(
                    files[0].id,
                    vec![hydrus_store::file_maintenance::JobType::DeleteNeighbourDupes]
                )]
            );
        }
        assert_eq!(case["finished"], true);
        assert_eq!(case["dismissed"], true);
    }
}
