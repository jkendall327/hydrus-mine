//! The media viewer: a page's files one at a time, in its own window.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_store::Store;

pub struct MediaViewer {
    store: Arc<Store>,
    files: Vec<HashId>,
    index: usize,
    /// The file domains of the page it was opened from.
    location: hydrus_search::LocationContext,
    /// Where each random file was gone to from, latest last.
    random_history: Vec<usize>,
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
            location: hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
            )),
            random_history: Vec::new(),
        })
    }

    /// Viewing a page searching `location` (where deletions take its files
    /// from).
    #[must_use]
    pub fn with_location(mut self, location: hydrus_search::LocationContext) -> Self {
        self.location = location;
        self
    }

    pub fn location(&self) -> &hydrus_search::LocationContext {
        &self.location
    }

    /// Take the current file out (deleted, say), showing the next; whether
    /// any are left.
    pub fn remove_current(&mut self) -> bool {
        self.remove(self.current())
    }

    /// Take `file` out: if it is the one shown, the next is shown, else the
    /// one shown stays; whether any are left.
    pub fn remove(&mut self, file: HashId) -> bool {
        if let Some(at) = self.files.iter().position(|&f| f == file) {
            self.files.remove(at);
            if at < self.index {
                self.index -= 1;
            }
            if self.index >= self.files.len() {
                self.index = 0;
            }
        }
        !self.files.is_empty()
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// The exit file, or none after the final file was removed.
    pub fn exit_media(&self) -> Option<HashId> {
        self.files.get(self.index).copied()
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

    /// A random other file, noting where it came from
    /// (`MediaList.GetRandom`); with one file, that one.
    pub fn random(&mut self) {
        let count = self.files.len();
        if count < 2 {
            return;
        }
        self.random_history.push(self.index);
        loop {
            let index = rand::random_range(0..count);
            if index != self.index {
                self.index = index;
                return;
            }
        }
    }

    /// Back to where the last random file came from, if anywhere
    /// (`MediaList.UndoRandom`).
    pub fn undo_random(&mut self) {
        if let Some(index) = self.random_history.pop() {
            self.index = index.min(self.files.len() - 1);
        }
    }

    /// The first file (`SIMPLE_VIEW_FIRST`).
    pub fn first(&mut self) {
        self.index = 0;
    }

    /// The last file (`SIMPLE_VIEW_LAST`).
    pub fn last(&mut self) {
        self.index = self.files.len() - 1;
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

    /// Whether the current file has sound (for which the viewer shows its
    /// volume control, as the reference's `ShouldHaveVolumeControl`).
    pub fn has_audio(&self) -> bool {
        self.store
            .read(|conn| hydrus_store::media::load_basic(conn, &[self.current()]))
            .ok()
            .and_then(|results| results.into_iter().next()?.info)
            .is_some_and(|info| info.has_audio)
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
        hover_tags(&self.store, self.current())
    }

    /// The current file's type and resolution, for zooming it.
    pub fn shape(&self) -> Option<(hydrus_core::Mime, Option<(u32, u32)>)> {
        shape(&self.store, self.current())
    }

    /// The current file's frames, if the reference plays its kind with its
    /// own player (ugoiras and animated WebP).
    pub fn animation(&self) -> Option<hydrus_media::animation::Frames> {
        animation(&self.store, self.current())
    }
}

/// A file's type and resolution, for zooming it.
pub(crate) fn shape(store: &Store, id: HashId) -> Option<(hydrus_core::Mime, Option<(u32, u32)>)> {
    let info = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .ok()?
        .into_iter()
        .next()?
        .info?;
    Some((info.mime, info.width.zip(info.height)))
}

/// What the top hover frame shows of a file: its info line, now, and
/// which of its buttons apply, as the reference's `_ResetButtons` decides;
/// and its notes, as (name, text), by name.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Shown {
    pub line: String,
    pub locations: Vec<String>,
    pub notes: Vec<(String, String)>,
    /// In the inbox: its archive button archives (else re-inboxes).
    pub inbox: bool,
    /// In the trash: its delete button deletes it completely.
    pub trashed: bool,
    /// Here, not in the trash: its delete button sends it to the trash.
    pub local: bool,
    /// Deleted from one of your local file domains: it can be undeleted.
    pub undeletable: bool,
}

pub(crate) fn shown(store: &Store, id: HashId) -> Shown {
    use hydrus_core::ServiceType;
    let snapshot = store.snapshot();
    let settings: hydrus_core::media_viewer::InfoLineSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let of_type = |kind: ServiceType| {
        snapshot
            .services
            .all()
            .filter(move |s| s.service_type() == kind)
            .map(|s| s.id)
    };
    store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, &[id]))
        .ok()
        .and_then(|batch| batch.results.into_iter().next())
        .map(|media| {
            let line = crate::info_lines::top_line(
                &media,
                &snapshot.services,
                &settings,
                hydrus_core::TimestampMs::now().0,
            );
            let trashed =
                of_type(ServiceType::LocalFileTrashDomain).any(|t| media.is_current_in(t));
            let here = of_type(ServiceType::HydrusLocalFileStorage).any(|l| media.is_current_in(l));
            let mut local: Vec<_> = snapshot
                .services
                .all()
                .filter(|service| service.service_type() == ServiceType::LocalFileDomain)
                .filter(|service| media.is_current_in(service.id))
                .map(|service| service.name.clone())
                .collect();
            local.sort();
            let mut remote: Vec<_> = snapshot
                .services
                .all()
                .filter(|service| {
                    matches!(
                        service.service_type(),
                        ServiceType::FileRepository | ServiceType::Ipfs
                    )
                })
                .filter_map(|service| {
                    if media.pending.contains(&service.id) {
                        Some((service.name.clone(), format!("{} (+)", service.name)))
                    } else if media.is_current_in(service.id) {
                        Some((
                            service.name.clone(),
                            if media.petitioned.contains(&service.id) {
                                format!("{} (-)", service.name)
                            } else {
                                service.name.clone()
                            },
                        ))
                    } else {
                        None
                    }
                })
                .collect();
            remote.sort_by(|left, right| left.0.cmp(&right.0));
            local.extend(remote.into_iter().map(|(_, display)| display));
            Shown {
                line,
                locations: local,
                inbox: media.inbox,
                trashed,
                local: here && !trashed,
                undeletable: of_type(ServiceType::LocalFileDomain)
                    .any(|d| media.is_deleted_from(d)),
                notes: media.notes,
            }
        })
        .unwrap_or_default()
}

/// A file's duration and frame count, for its scanbar.
pub(crate) fn timing(store: &Store, id: HashId) -> (Option<u64>, Option<u64>) {
    let info = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .ok()
        .and_then(|basic| basic.into_iter().next()?.info);
    info.map_or((None, None), |info| (info.duration_ms, info.num_frames))
}

/// A file's still, to draw sharply at its zoom: the file decoded whole
/// (not a thumbnail standing in for it), if it is a still.
pub(crate) fn still_of(
    media: Option<std::sync::Arc<hydrus_media::Raster>>,
    shape: Option<(hydrus_core::Mime, Option<(u32, u32)>)>,
    still: bool,
) -> Option<std::sync::Arc<hydrus_media::Raster>> {
    let resolution = shape?.1?;
    media.filter(|m| still && (m.width(), m.height()) == resolution)
}

/// A file's tags as the tags hover frame lists them, each with its colour.
pub(crate) fn hover_tags(store: &Store, id: HashId) -> Vec<(String, [u8; 3])> {
    let colours: hydrus_core::tag_presentation::NamespaceColours =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    crate::page::tag_rows(store, &[id], None, crate::page::TagList::MediaViewer)
        .into_iter()
        .map(|(tag, row)| (row, colours.tag(&tag)))
        .collect()
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
