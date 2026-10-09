//! The archive/delete window's scanbar and volume control, as the media
//! viewer's: a bar under a file mpv or the animator plays, seeking on a
//! click or drag (playing pauses while dragging), and the global and viewer
//! mutes and volume at its right.
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use hydrus_store::Store;

use crate::{ArchiveDeleteWindow, animation, audio, playback, scanbar};

/// Apply the scanbar's configured sizes to the window.
pub(crate) fn seek_options(window: &ArchiveDeleteWindow, store: &Store) {
    let options: hydrus_store::settings::ViewerCanvasSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    window.set_seek_height(f32::from(options.seek_height.clamp(1, 255) as u8));
    window.set_seek_hidden_height(
        f32::from(options.seek_hidden_height.map_or(0, |h| h.clamp(1, 255)) as u8),
    );
    window.set_seek_nub_width(f32::from(options.seek_nub_width.clamp(1, 63) as u8));
}

struct State {
    /// The scanbar of the file playing, and whether the animator (not mpv)
    /// plays it.
    bar: Cell<Option<(scanbar::Scanbar, bool)>>,
    playing_before_scan: Cell<bool>,
    store: Arc<Store>,
    playback: Rc<playback::Playback>,
    animator: Rc<animation::Animator>,
    weak: slint::Weak<ArchiveDeleteWindow>,
    timer: slint::Timer,
}

/// The window's scanbar and volume wiring.
#[derive(Clone)]
pub(crate) struct Controls(Rc<State>);

impl Controls {
    pub(crate) fn new(
        window: &ArchiveDeleteWindow,
        store: Arc<Store>,
        playback: Rc<playback::Playback>,
        animator: Rc<animation::Animator>,
    ) -> Self {
        let state = Rc::new(State {
            bar: Cell::new(None),
            playing_before_scan: Cell::new(false),
            store,
            playback,
            animator,
            weak: window.as_weak(),
            timer: slint::Timer::default(),
        });
        let controls = Controls(state);
        controls.show_audio();
        controls.bind(window);
        controls
    }

    fn show_audio(&self) {
        let s = &self.0;
        let settings = audio::settings(&s.store);
        s.playback
            .set_audio(settings.current_viewer_volume(), settings.viewer_muted());
        if let Some(window) = s.weak.upgrade() {
            window.set_global_muted(settings.global_mute);
            window.set_viewer_muted(settings.viewer_mute);
            window.set_volume(i32::from(settings.current_viewer_volume()));
        }
    }

    /// A file is shown: its scanbar, if it plays, and the volume control if
    /// mpv plays it with sound.
    pub(crate) fn file_shown(
        &self,
        mpv: bool,
        animation: Option<(usize, u64)>,
        duration_ms: Option<u64>,
        num_frames: Option<u64>,
        has_audio: bool,
    ) {
        let s = &self.0;
        let bar = match (mpv, animation) {
            (true, _) => scanbar::Scanbar::new(duration_ms, num_frames).map(|b| (b, false)),
            (false, Some((frames, total_ms))) => scanbar::Scanbar::new(
                duration_ms.or(Some(total_ms)),
                num_frames.or(Some(frames as u64)),
            )
            .map(|b| (b, true)),
            (false, None) => None,
        };
        s.bar.set(bar);
        s.playing_before_scan.set(false);
        if let Some(window) = s.weak.upgrade() {
            window.set_scanbar_shown(bar.is_some());
            window.set_volume_shown(mpv && has_audio);
            window.set_volume_open(false);
        }
        self.show_position(0.0);
        self.show_audio();
    }

    fn show_position(&self, position_ms: f64) {
        let s = &self.0;
        if let (Some(window), Some((bar, _))) = (s.weak.upgrade(), s.bar.get()) {
            let (progress, text) = bar.at(position_ms);
            window.set_scanbar_progress(progress);
            window.set_scanbar_text(text.into());
        }
    }

    fn follow(&self) {
        let s = &self.0;
        match s.bar.get() {
            Some((_, false)) => {
                if let Some(position) = s.playback.position_ms() {
                    self.show_position(position);
                }
            }
            Some((bar, true)) => {
                if let (Some(status), Some(window)) = (s.animator.status(), s.weak.upgrade()) {
                    let (progress, text) = bar.at_frame(status.index, status.at_ms);
                    window.set_scanbar_progress(progress);
                    window.set_scanbar_text(text.into());
                }
            }
            None => {}
        }
    }

    fn playing(&self) -> bool {
        let s = &self.0;
        match s.bar.get() {
            Some((_, false)) => !s.playback.paused(),
            Some((_, true)) => s.animator.status().is_some_and(|st| !st.paused),
            None => false,
        }
    }

    fn bind(&self, window: &ArchiveDeleteWindow) {
        let this = self.clone();
        // (the bar follows playing; weak, as the state owns the timer)
        let follower = Rc::downgrade(&self.0);
        self.0
            .timer
            .start(slint::TimerMode::Repeated, Duration::from_millis(50), move || {
                if let Some(state) = follower.upgrade() {
                    Controls(state).follow();
                }
            });
        window.on_scan({
            let this = this.clone();
            move |x, width| {
                let s = &this.0;
                let nub = s
                    .weak
                    .upgrade()
                    .map_or(scanbar::NUB_WIDTH, |w| w.get_seek_nub_width());
                match s.bar.get() {
                    Some((bar, false)) => {
                        let to = bar.seek_to_with_nub(x, width, nub);
                        s.playback.seek_ms(to);
                        this.show_position(to);
                    }
                    Some((bar, true)) => {
                        let index = bar.frame_at_with_nub(x, width, nub);
                        s.animator.goto(index);
                        if let Some(window) = s.weak.upgrade() {
                            window.set_scanbar_progress(bar.at_frame(index, 0).0);
                        }
                    }
                    None => {}
                }
            }
        });
        window.on_scan_started({
            let this = this.clone();
            move || {
                let s = &this.0;
                let playing = this.playing();
                s.playing_before_scan.set(playing);
                if playing {
                    s.playback.set_paused(true);
                    s.animator.set_paused(true);
                }
            }
        });
        window.on_scan_ended({
            let this = this.clone();
            move || {
                let s = &this.0;
                if s.playing_before_scan.replace(false) {
                    match s.bar.get() {
                        Some((_, false)) => s.playback.set_paused(false),
                        Some((_, true)) => s.animator.set_paused(false),
                        None => {}
                    }
                }
            }
        });
        window.on_flip_global_mute({
            let this = this.clone();
            move || {
                audio::flip_global_mute(&this.0.store);
                this.show_audio();
            }
        });
        window.on_flip_viewer_mute({
            let this = this.clone();
            move || {
                audio::change(&this.0.store, |a| a.viewer_mute = !a.viewer_mute);
                this.show_audio();
            }
        });
        window.on_volume_changed(move |volume| {
            let volume = u8::try_from(volume.clamp(0, 100)).unwrap_or(0);
            if audio::settings(&this.0.store).current_viewer_volume() != volume {
                audio::change(&this.0.store, |a| a.set_viewer_volume(volume));
                this.show_audio();
            }
        });
    }
}
