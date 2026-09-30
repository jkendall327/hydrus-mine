//! Reading files' content for the comparators that need it (jpeg quality,
//! visual duplicates), with caches like the reference's.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_media::jpeg::{self, JpegQuality};
use hydrus_media::visual::{self, VisualData, VisualDataTiled};
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
        Self {
            store,
            snapshot: store.snapshot(),
            tools: MediaTools::new(),
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

    fn visual_confidence(&mut self, a: HashId, b: HashId) -> Option<u8> {
        let (va, vb) = (self.visual_data(a)?, self.visual_data(b)?);
        if !visual::similar_simple(&va, &vb).0 {
            return None;
        }
        let (ta, tb) = (self.tiled(a)?, self.tiled(b)?);
        Some(visual::similar_regional(&ta, &tb).1)
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
