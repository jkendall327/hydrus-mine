//! Reading files' content for the comparators that need it (jpeg quality,
//! visual duplicates), with caches like the reference's.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_media::jpeg::{self, JpegQuality};
use hydrus_media::visual::{self, Verdict, VisualData, VisualDataTiled};
use hydrus_media::{MediaTools, Raster};
use hydrus_store::{Snapshot, Store};

use crate::selector::FileContent;

/// How many detailed image summaries to keep (each is over a megabyte).
const TILED_CACHE: usize = 24;

/// File content from the store's media files.
pub struct StoreContent<'s> {
    store: &'s Store,
    snapshot: Arc<Snapshot>,
    tools: MediaTools,
    jpegs: HashMap<HashId, JpegQuality>,
    visual: HashMap<HashId, Option<VisualData>>,
    tiled: VecDeque<(HashId, Option<Arc<VisualDataTiled>>)>,
}

impl<'s> StoreContent<'s> {
    pub fn new(store: &'s Store) -> Self {
        Self::with_tools(store, MediaTools::new())
    }

    /// Supply executable/decoder tools while retaining this Store's live policy.
    pub fn with_tools(store: &'s Store, tools: MediaTools) -> Self {
        Self {
            store,
            snapshot: store.snapshot(),
            tools: tools.with_ffmpeg_timeout_reader(hydrus_store::ffmpeg_policy::reader(store)),
            jpegs: HashMap::new(),
            visual: HashMap::new(),
            tiled: VecDeque::new(),
        }
    }

    fn path(&self, file: HashId) -> Option<(std::path::PathBuf, hydrus_core::Mime)> {
        let (hash, mime) = self
            .store
            .read(|conn| {
                let hashes = hydrus_store::master::hashes(conn, &[file])?;
                let mime: Option<u8> = rusqlite::OptionalExtension::optional(conn.query_row(
                    "SELECT mime FROM files WHERE hash_id = ?",
                    [file],
                    |r| r.get(0),
                ))?;
                Ok((hashes.get(&file).copied(), mime))
            })
            .ok()?;
        let mime = hydrus_core::Mime::from_code(mime?)?;
        Some((self.snapshot.storage.file_path(&hash?, mime)?, mime))
    }

    fn image(&self, file: HashId) -> Option<Raster> {
        let (path, mime) = self.path(file)?;
        self.tools.load_image(&path, mime).ok()
    }

    fn visual_data(&mut self, file: HashId) -> Option<VisualData> {
        if let Some(v) = self.visual.get(&file) {
            return v.clone();
        }
        let v = self.image(file).map(|i| visual::visual_data(&i));
        if self.visual.len() > 10_000 {
            self.visual.clear();
        }
        self.visual.insert(file, v.clone());
        v
    }

    fn tiled(&mut self, file: HashId) -> Option<Arc<VisualDataTiled>> {
        if let Some((_, t)) = self.tiled.iter().find(|(f, _)| *f == file) {
            return t.clone();
        }
        let t = self
            .image(file)
            .map(|i| Arc::new(visual::visual_data_tiled(&i)));
        if self.tiled.len() >= TILED_CACHE {
            self.tiled.pop_front();
        }
        self.tiled.push_back((file, t.clone()));
        t
    }
}

impl FileContent for StoreContent<'_> {
    fn jpeg_quality(&mut self, file: HashId) -> JpegQuality {
        if let Some(q) = self.jpegs.get(&file) {
            return *q;
        }
        let data = self
            .path(file)
            .and_then(|(path, _)| std::fs::read(path).ok())
            .unwrap_or_default();
        let q = jpeg::jpeg_quality(&data);
        self.jpegs.insert(file, q);
        q
    }

    fn visual_comparison(&mut self, a: HashId, b: HashId) -> Option<(Verdict, Option<Verdict>)> {
        let (va, vb) = (self.visual_data(a)?, self.visual_data(b)?);
        let simple = visual::similar_simple(&va, &vb);
        if !simple.similar {
            return Some((simple, None));
        }
        let regional = match (self.tiled(a), self.tiled(b)) {
            (Some(ta), Some(tb)) => Some(visual::similar_regional(&ta, &tb)),
            _ => None,
        };
        Some((simple, regional))
    }
}

impl std::fmt::Debug for StoreContent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreContent")
            .field("jpegs", &self.jpegs.len())
            .field("visual", &self.visual.len())
            .field("tiled", &self.tiled.len())
            .finish_non_exhaustive()
    }
}

#[cfg(all(test, unix))]
mod timeout_tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt as _, process::Command, sync::mpsc, time::Duration};
    #[test]
    fn retained_store_content_reads_new_policy_before_real_psd_decode() {
        use hydrus_store::{ffmpeg_policy::FfmpegPolicy, settings};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let importer = hydrus_import::FileImporter::new(store.clone(), MediaTools::new());
        let imported = importer
            .import_path(
                &hydrus_testkit::fixture_path("image_decoder_policies/plain.png"),
                &hydrus_import::FileImportOptions::default(),
            )
            .unwrap();
        let hash = imported.hash.unwrap();
        let id = store
            .read(|conn| hydrus_store::master::hash_id(conn, &hash))
            .unwrap()
            .unwrap();
        let path = store
            .snapshot()
            .storage
            .file_path(&hash, hydrus_core::Mime::ApplicationPsd)
            .unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"authored local PSD subprocess transport").unwrap();
        store
            .write(move |c| {
                c.conn().execute(
                    "UPDATE files SET mime=? WHERE hash_id=?",
                    rusqlite::params![hydrus_core::Mime::ApplicationPsd as u8, id],
                )?;
                Ok(())
            })
            .unwrap();
        let fifo = dir.path().join("gate");
        assert!(
            Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let exe = dir.path().join("ffmpeg");
        std::fs::write(
            &exe,
            format!("#!/bin/sh\nread -r reply < '{}'\n", fifo.display()),
        )
        .unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let tools = MediaTools::with_ffmpeg(hydrus_media::Ffmpeg::with_executable(exe));
        let (ready, opened) = mpsc::channel();
        let (start, go) = mpsc::channel();
        let (sender, result) = mpsc::channel();
        let current = store.clone();
        let worker = std::thread::spawn(move || {
            let content = StoreContent::with_tools(&current, tools);
            ready.send(()).unwrap();
            go.recv().unwrap();
            sender.send(content.image(id)).unwrap();
        });
        opened.recv_timeout(Duration::from_secs(2)).unwrap();
        store
            .write(|c| settings::set(c.conn(), &FfmpegPolicy { seconds: 1 }))
            .unwrap();
        start.send(()).unwrap();
        assert!(
            result
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .is_none(),
            "real render process must time out at the newly saved policy, not old15 seconds"
        );
        worker.join().unwrap();
    }
}
