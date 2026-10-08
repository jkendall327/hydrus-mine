//! The preview pane's sound and moving pictures: the file it shows plays in
//! mpv (the media viewer's player) when the reference's preview canvas
//! would play it, at the preview's volume (`GetCorrectCurrentVolume` and
//! `GetCorrectCurrentMute` for `CANVAS_PREVIEW`), with the volume control
//! the reference puts on the preview (`ClientGUIMediaControls`).
//!
//! The preview plays what it has accepted, if its kind is shown in mpv
//! (`preview_show_action`) and not covered by an embed button; it starts
//! paused if the view says so (`preview_start_paused`). Where libmpv is
//! absent the decisions are still made (the player's `target` and `audio`
//! show them) and the pane keeps its still.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hydrus_core::HashId;
use hydrus_core::media_viewer::{MediaViewerSettings, ShowAction};
use hydrus_gui_model::audio;
use hydrus_store::Store;
use slint::ComponentHandle as _;

use crate::MainWindow;
use crate::playback::Playback;

/// How often the volume and mutes are looked up again while playing (ctrl+g
/// and the Options window change them from elsewhere).
const AUDIO_CHECK: Duration = Duration::from_millis(250);

pub(crate) struct PreviewAudio {
    store: Arc<Store>,
    playback: Rc<Playback>,
    playing: Cell<Option<HashId>>,
    /// Whether a frame of the video has come to replace the still.
    has_frame: Rc<Cell<bool>>,
    checked: Cell<Option<Instant>>,
}

/// Whether the preview plays `file`: its kind is shown in mpv and not
/// behind an embed button; with where it is.
pub(crate) fn playable(store: &Store, file: HashId) -> Option<(std::path::PathBuf, bool)> {
    let (mime, _) = crate::viewer::shape(store, file)?;
    let settings: MediaViewerSettings = store.read(hydrus_store::settings::get).ok()?;
    let view = settings.view(mime);
    if view.preview_show_action != ShowAction::Mpv || view.preview_start_with_embed {
        return None;
    }
    Some((
        crate::viewer::playable(store, file)?,
        view.preview_start_paused,
    ))
}

fn has_audio(store: &Store, file: HashId) -> bool {
    store
        .read(|conn| hydrus_store::media::load_basic(conn, &[file]))
        .ok()
        .and_then(|results| results.into_iter().next()?.info)
        .is_some_and(|info| info.has_audio)
}

impl PreviewAudio {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            playback: Playback::for_store(store.clone()),
            store,
            playing: Cell::new(None),
            has_frame: Rc::default(),
            checked: Cell::new(None),
        }
    }

    pub fn playback(&self) -> &Rc<Playback> {
        &self.playback
    }

    /// Whether video frames, not the still, are what the pane shows.
    pub fn showing_frames(&self) -> bool {
        self.has_frame.get()
    }

    /// Play `file` (the preview's accepted file), or stop on `None`.
    pub fn sync(&self, window: &MainWindow, file: Option<HashId>) {
        let file = file.filter(|&file| playable(&self.store, file).is_some());
        if self.playing.get() != file {
            self.playing.set(file);
            self.has_frame.set(false);
            self.checked.set(None);
            match file.and_then(|file| Some((file, playable(&self.store, file)?))) {
                Some((_, (path, paused))) => {
                    let (volume, mute) = audio::preview_sound(&self.store);
                    self.playback.set_audio(volume, mute);
                    let (size, frame, has_frame) =
                        (window.as_weak(), window.as_weak(), self.has_frame.clone());
                    self.playback.play(
                        Some(&path),
                        move || {
                            let window = size.upgrade()?;
                            let (w, h) = (
                                window.get_preview_media_width(),
                                window.get_preview_media_height(),
                            );
                            let scale = window.window().scale_factor();
                            (w >= 1.0 && h >= 1.0)
                                .then_some(((w * scale) as u32, (h * scale) as u32))
                        },
                        move |image| {
                            if let Some(window) = frame.upgrade() {
                                has_frame.set(true);
                                window.set_preview_media(image);
                            }
                        },
                    );
                    self.playback.set_paused(paused);
                }
                None => self.playback.stop(),
            }
        }
        let now = Instant::now();
        if self
            .checked
            .get()
            .is_none_or(|at| now.duration_since(at) >= AUDIO_CHECK)
        {
            self.checked.set(Some(now));
            self.show_audio(window);
        }
    }

    /// The volume and mutes, as kept, on the player and the control.
    fn show_audio(&self, window: &MainWindow) {
        let settings = audio::settings(&self.store);
        let own = audio::preview_uses_its_own_volume(&self.store);
        let (volume, muted) = audio::preview_sound(&self.store);
        self.playback.set_audio(volume, muted);
        window.set_preview_volume_shown(
            self.playing
                .get()
                .is_some_and(|file| has_audio(&self.store, file)),
        );
        window.set_preview_volume(i32::from(settings.current_preview_volume(own)));
        window.set_preview_global_muted(settings.global_mute);
        window.set_preview_muted(settings.preview_mute);
    }

    /// The control was used: keep the change and show it.
    pub fn changed(&self, window: &MainWindow, change: impl FnOnce(&Store)) {
        change(&self.store);
        self.show_audio(window);
    }

    pub fn close(&self) {
        self.playing.set(None);
        self.has_frame.set(false);
        self.playback.close();
    }
}
