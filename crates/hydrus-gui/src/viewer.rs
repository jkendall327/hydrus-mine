//! The media viewer: a page's files one at a time, in its own window.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_store::Store;

pub struct MediaViewer {
    store: Arc<Store>,
    image_cache: Option<crate::image_cache::Handle>,
    owns_cache: bool,
    files: Vec<HashId>,
    index: usize,
    /// The file domains of the page it was opened from.
    location: hydrus_search::LocationContext,
    /// Where each random file was gone to from, latest last.
    random_history: Vec<usize>,
    /// Display mode captured when this viewer's tag list opens.
    tag_display_type: hydrus_core::tag_presentation::TagDisplayType,
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
        let presentation: hydrus_core::tag_presentation::TagPresentation =
            store.read(hydrus_store::settings::get).unwrap_or_default();
        (index < files.len()).then_some(Self {
            store,
            image_cache: None,
            owns_cache: false,
            files,
            index,
            location: hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
            )),
            random_history: Vec::new(),
            tag_display_type: presentation.viewer_display_type,
        })
    }

    pub(crate) fn with_image_cache(mut self, cache: crate::image_cache::Handle) -> Self {
        self.image_cache = Some(cache);
        self
    }
    pub(crate) fn shared_media(&self) -> Option<Arc<hydrus_media::Raster>> {
        self.image_cache.as_ref().map_or_else(
            || self.media().map(Arc::new),
            |cache| cache.load_current_saved(&self.store, self.current()),
        )
    }

    pub(crate) fn prefetch_cache(&mut self) -> crate::image_cache::Handle {
        if self.image_cache.is_none() {
            self.image_cache = Some(crate::image_cache::Handle::standalone(&self.store));
            self.owns_cache = true;
        }
        self.image_cache.as_ref().unwrap().clone()
    }
    pub(crate) fn retire_prefetch_cache(&self) {
        if self.owns_cache
            && let Some(cache) = &self.image_cache
        {
            cache.retire();
        }
    }

    pub(crate) fn prefetch_files(
        &self,
        preferences: hydrus_store::viewer_prefetch::Preferences,
    ) -> Vec<HashId> {
        let mut files = vec![self.current()];
        files.extend(hydrus_gui_model::viewer_prefetch::neighbours(
            &self.files,
            self.index,
            preferences.previous,
            preferences.next,
        ));
        files
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

    /// The tag display mode captured when this viewer's list opened.
    pub fn tag_display_type(&self) -> hydrus_core::tag_presentation::TagDisplayType {
        self.tag_display_type
    }

    /// The current file's tag rows in the captured display mode, with their
    /// namespace colours and pending/petitioned markers.
    pub fn tag_rows(&self) -> Vec<(String, [u8; 3])> {
        hover_tags(&self.store, self.current(), self.tag_display_type)
    }

    /// Canonical identity alongside each rendered hover row; never parse its label.
    pub(crate) fn tag_entries(&self) -> Vec<(String, String, [u8; 3])> {
        let colours: hydrus_core::tag_presentation::NamespaceColours = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        crate::page::tag_rows(
            &self.store,
            &[self.current()],
            None,
            crate::page::TagList::MediaViewer,
            self.tag_display_type,
        )
        .into_iter()
        .map(|(tag, text)| {
            let colour = colours.tag(&tag);
            (tag, text, colour)
        })
        .collect()
    }

    /// The current file's type and resolution, for zooming it.
    pub fn shape(&self) -> Option<(hydrus_core::Mime, Option<(u32, u32)>)> {
        shape(&self.store, self.current())
    }

    /// The current file's frames, if the reference plays its kind with its
    /// own player (ugoiras and animated WebP).
    pub fn animation(&self) -> Option<hydrus_media::animation::Frames> {
        animation_owned(&self.store, self.current())
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
            let line = crate::info_lines::top_line_with_format(
                &media,
                &snapshot.services,
                &settings,
                hydrus_core::TimestampMs::now().0,
                &hydrus_gui_model::gui_format::preferences(store),
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

/// Whether a file has an audio track.
pub(crate) fn has_audio_of(store: &Store, id: HashId) -> bool {
    store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .ok()
        .and_then(|results| results.into_iter().next()?.info)
        .is_some_and(|info| info.has_audio)
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
pub(crate) fn hover_tags(
    store: &Store,
    id: HashId,
    display_type: hydrus_core::tag_presentation::TagDisplayType,
) -> Vec<(String, [u8; 3])> {
    let colours: hydrus_core::tag_presentation::NamespaceColours =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    crate::page::tag_rows(
        store,
        &[id],
        None,
        crate::page::TagList::MediaViewer,
        display_type,
    )
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
    let policy = store.read(hydrus_store::image_colour::load).ok()?;
    Frames::open_with_icc(
        &path,
        info.mime,
        &result.notes,
        info.num_frames,
        policy.normalise_icc,
    )
    .map_err(|e| eprintln!("could not play {}: {e}", path.display()))
    .ok()
}

/// Existing native players read their owned Store policy before each future
/// frame, preserving the animation timeline and media viewing interval.
pub(crate) fn animation_owned(
    store: &std::sync::Arc<Store>,
    id: HashId,
) -> Option<hydrus_media::animation::Frames> {
    let frames = animation(store, id)?;
    let weak = std::sync::Arc::downgrade(store);
    Some(frames.with_icc_reader(std::sync::Arc::new(move || {
        match weak
            .upgrade()
            .map(|store| store.read(hydrus_store::image_colour::load))
        {
            Some(Ok(policy)) => policy.normalise_icc,
            Some(Err(error)) => {
                eprintln!("could not read frame ICC policy: {error}");
                true
            }
            None => true,
        }
    })))
}

/// A file as a still: an image decoded whole; anything else by its
/// thumbnail.
pub fn still(store: &Store, id: HashId) -> Option<hydrus_media::Raster> {
    let policy = store.read(hydrus_store::image_colour::load).ok()?;
    still_with_icc(store, id, policy.normalise_icc)
}

/// A worker captures policy at request admission so a later settings change
/// cannot change the meaning of its request generation.
pub(crate) fn still_with_icc(
    store: &Store,
    id: HashId,
    normalise_icc: bool,
) -> Option<hydrus_media::Raster> {
    let result = store
        .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
        .ok()?
        .into_iter()
        .next()?;
    full_still_with_icc(store, &result, normalise_icc)
        .or_else(|| thumbnail_still_with_icc(store, &result, normalise_icc))
}
pub(crate) fn full_still_with_icc(
    store: &Store,
    result: &hydrus_store::media::MediaResult,
    normalise_icc: bool,
) -> Option<hydrus_media::Raster> {
    let info = result.info.as_ref()?;
    let path = store
        .snapshot()
        .storage
        .file_path(&result.hash, info.mime)?;
    hydrus_media::decode_image_with_icc(&std::fs::read(path).ok()?, normalise_icc).ok()
}
pub(crate) fn thumbnail_still_with_icc(
    store: &Store,
    result: &hydrus_store::media::MediaResult,
    normalise_icc: bool,
) -> Option<hydrus_media::Raster> {
    let path = store.snapshot().storage.thumbnail_path(&result.hash)?;
    hydrus_media::decode_image_with_icc(&std::fs::read(path).ok()?, normalise_icc).ok()
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

impl Drop for MediaViewer {
    fn drop(&mut self) {
        self.retire_prefetch_cache();
    }
}
