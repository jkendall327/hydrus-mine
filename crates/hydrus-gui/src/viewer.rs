//! The media viewer: a page's files one at a time, in its own window.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_store::Store;

pub struct MediaViewer {
    store: Arc<Store>,
    files: Vec<HashId>,
    index: usize,
}

impl std::fmt::Debug for MediaViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaViewer")
            .field("files", &self.files.len())
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl MediaViewer {
    /// View `files` from the one at `index`; `None` if there is none.
    pub fn new(store: Arc<Store>, files: Vec<HashId>, index: usize) -> Option<Self> {
        (index < files.len()).then_some(Self {
            store,
            files,
            index,
        })
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn current(&self) -> HashId {
        self.files[self.index]
    }

    /// The next file, from the last back to the first, as in the reference.
    pub fn next(&mut self) {
        self.index = (self.index + 1) % self.files.len();
    }

    pub fn previous(&mut self) {
        self.index = self.index.checked_sub(1).unwrap_or(self.files.len() - 1);
    }

    /// The window's title, e.g. `3/31`.
    pub fn caption(&self) -> String {
        format!("{}/{}", self.index + 1, self.files.len())
    }

    /// The current file as shown: an image decoded whole; anything the
    /// viewer can't show yet, by its thumbnail.
    pub fn media(&self) -> Option<hydrus_media::Raster> {
        let id = self.current();
        let result = self
            .store
            .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
            .ok()?
            .into_iter()
            .next()?;
        let snapshot = self.store.snapshot();
        let full = result.info.as_ref().and_then(|info| {
            let path = snapshot.storage.file_path(&result.hash, info.mime)?;
            let bytes = std::fs::read(path).ok()?;
            hydrus_media::decode_image(&bytes).ok()
        });
        full.or_else(|| {
            let path = snapshot.storage.thumbnail_path(&result.hash)?;
            hydrus_media::decode_image(&std::fs::read(path).ok()?).ok()
        })
    }
}
