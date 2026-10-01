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

    pub fn store(&self) -> &Arc<Store> {
        &self.store
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

    /// Where the current file is, if the reference plays its kind in mpv by
    /// default.
    pub fn playable(&self) -> Option<std::path::PathBuf> {
        playable(&self.store, self.current())
    }

    /// The current file as a still: an image decoded whole; anything else by
    /// its thumbnail.
    pub fn media(&self) -> Option<hydrus_media::Raster> {
        still(&self.store, self.current())
    }

    /// The current file's tags as the tags hover frame lists them, each
    /// with its colour: display tags, less those the single media filters
    /// hide, in the media viewer's tag sort, pending ones marked `(+)`.
    pub fn tag_rows(&self) -> Vec<(String, [u8; 3])> {
        let colours: hydrus_core::tag_presentation::NamespaceColours = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        crate::page::tag_rows(
            &self.store,
            &[self.current()],
            None,
            crate::page::TagList::MediaViewer,
        )
        .into_iter()
        .map(|(tag, row)| (row, colours.tag(&tag)))
        .collect()
    }

    /// The current file's frames, if the reference plays its kind with its
    /// own player (ugoiras and animated WebP).
    pub fn animation(&self) -> Option<hydrus_media::animation::Frames> {
        animation(&self.store, self.current())
    }
}

/// A file's frames, if the reference plays its kind with its own player:
/// a ugoira (timed by its animation.json, else its notes) or an animated
/// WebP.
pub fn animation(store: &Store, id: HashId) -> Option<hydrus_media::animation::Frames> {
    use hydrus_media::animation::Frames;
    let snapshot = store.snapshot();
    let result = store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, &[id]))
        .ok()?
        .results
        .into_iter()
        .next()?;
    let info = result.info.as_ref()?;
    if !Frames::plays(info.mime) {
        return None;
    }
    let path = snapshot.storage.file_path(&result.hash, info.mime)?;
    Frames::open(&path, info.mime, &result.notes, info.num_frames)
        .map_err(|e| eprintln!("could not play {}: {e}", path.display()))
        .ok()
}

/// A file as a still: an image decoded whole; anything else by its
/// thumbnail.
pub fn still(store: &Store, id: HashId) -> Option<hydrus_media::Raster> {
    let result = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .ok()?
        .into_iter()
        .next()?;
    let snapshot = store.snapshot();
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

/// Where a file is, if the reference plays its kind in mpv by default:
/// video, audio and animations (but animated WebP and JPEG XL, and ugoiras,
/// which it shows natively).
pub fn playable(store: &Store, id: HashId) -> Option<std::path::PathBuf> {
    use hydrus_core::Mime;
    let result = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .ok()?
        .into_iter()
        .next()?;
    let mime = result.info?.mime;
    let plays = match mime {
        Mime::AnimationWebp | Mime::AnimationJxl | Mime::AnimationUgoira => false,
        other => matches!(
            other.general_class(),
            Some(Mime::GeneralVideo | Mime::GeneralAudio | Mime::GeneralAnimation)
        ),
    };
    if !plays {
        return None;
    }
    store.snapshot().storage.file_path(&result.hash, mime)
}
