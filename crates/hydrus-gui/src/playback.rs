//! Playing a file in a window through mpv (video, audio and most
//! animations, as the reference's defaults have it): the player is made when
//! first needed, and its frames are shown as they come.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::mpv;

pub(crate) struct Playback {
    /// The store's `mpv.conf` (else hydrus's default options apply).
    conf: PathBuf,
    store: Option<Arc<hydrus_store::Store>>,
    times_to_play: Cell<u32>,
    player: RefCell<Option<mpv::Player>>,
    frames: slint::Timer,
    /// The volume and mute to play at.
    audio: Cell<(u8, bool)>,
    /// Whether the file stops at its end rather than play again
    /// (`StopForSlideshow`).
    stop_at_end: Cell<bool>,
    /// How many times it has gone back to its start, as it loops (or is
    /// sought there), and where it was last seen, in milliseconds.
    restarts: Cell<u32>,
    last_position: Cell<Option<f64>>,
}

/// Going back to within this many milliseconds of the start is a restart
/// (the reference counts seeks to under a tenth of a second; seen every
/// 10 milliseconds or so, a little more).
const RESTARTED_MS: f64 = 250.0;

impl Playback {
    pub fn new(conf: PathBuf) -> Rc<Self> {
        Rc::new(Self {
            conf,
            store: None,
            times_to_play: Cell::new(0),
            player: RefCell::new(None),
            frames: slint::Timer::default(),
            audio: Cell::new((100, false)),
            stop_at_end: Cell::new(false),
            restarts: Cell::new(0),
            last_position: Cell::new(None),
        })
    }

    pub fn for_store(store: Arc<hydrus_store::Store>) -> Rc<Self> {
        let mut playback = Self::new(store.dir().join("mpv.conf"));
        Rc::get_mut(&mut playback)
            .expect("new player is exclusively owned")
            .store = Some(store);
        playback
    }

    /// Play `path`, its frames at the size `size` gives shown by `show`; or,
    /// with none (or no libmpv), stop.
    pub fn play(
        self: &Rc<Self>,
        path: Option<&Path>,
        size: impl Fn() -> Option<(u32, u32)> + 'static,
        show: impl Fn(slint::Image) + 'static,
    ) {
        // (a new file plays on, and hasn't played through)
        self.stop_at_end.set(false);
        self.restarts.set(0);
        self.last_position.set(None);
        let Some(path) = path.filter(|_| mpv::available()) else {
            self.stop();
            return;
        };
        let always_loop = self.store.as_ref().is_none_or(|store| {
            store
                .read(hydrus_store::settings::get::<hydrus_store::settings::ViewerPlaybackSettings>)
                .unwrap_or_default()
                .always_loop
        });
        self.times_to_play.set(if always_loop {
            0
        } else {
            hydrus_media::animation::times_to_play(path)
        });
        let mut player = self.player.borrow_mut();
        if player.is_none() {
            match mpv::Player::new(Some(&self.conf)) {
                Ok(made) => *player = Some(made),
                Err(e) => {
                    eprintln!("could not start mpv: {e}");
                    return;
                }
            }
        }
        if let Some(player) = player.as_ref() {
            if let Err(e) = player.load(path) {
                eprintln!("mpv could not play {}: {e}", path.display());
            }
            // (each file plays from the start, though the last was paused,
            // as the reference's `SetMedia` has it)
            if let Err(e) = player.set_paused(false) {
                eprintln!("could not play: {e}");
            }
            // (as the reference sets them on each file it loads)
            let (volume, mute) = self.audio.get();
            if let Err(e) = player.set_audio(volume, mute) {
                eprintln!("could not set mpv's volume: {e}");
            }
        }
        let this = Rc::downgrade(self);
        self.frames.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(10),
            move || {
                let Some(this) = this.upgrade() else { return };
                let Ok(player) = this.player.try_borrow() else {
                    return;
                };
                let (Some(player), Some((width, height))) = (player.as_ref(), size()) else {
                    return;
                };
                player.set_size(width, height);
                if let Some(frame) = player.frame() {
                    show(frame);
                }
                this.watch_restarts(player);
            },
        );
    }

    /// Note the file going back to its start; told to stop at its end, it
    /// pauses there, at the start (as the reference's player pauses on
    /// seeking to it).
    fn watch_restarts(&self, player: &mpv::Player) {
        let Some(position) = player.position_ms() else {
            return;
        };
        let before = self.last_position.replace(Some(position));
        if before.is_some_and(|before| position < before) && position < RESTARTED_MS {
            self.restarts.set(self.restarts.get() + 1);
            if self.stop_at_end.get()
                || (self.times_to_play.get() != 0
                    && self.restarts.get() >= self.times_to_play.get())
            {
                let _ = player.set_paused(true);
                let _ = player.seek_ms(0.0);
                self.last_position.set(Some(0.0));
            }
        }
    }

    /// Whether the file has played through (`HasPlayedOnceThrough`).
    pub fn played_through(&self) -> bool {
        self.restarts.get() > 0
    }

    /// Stop at the file's end rather than play it again, or not
    /// (`StopForSlideshow`): until the next file.
    pub fn set_stop_at_end(&self, stop: bool) {
        self.stop_at_end.set(stop);
    }

    /// Stop playing (the player is kept for the next file).
    pub fn stop(&self) {
        self.frames.stop();
        if let Some(player) = self.player.borrow().as_ref() {
            let _ = player.stop();
        }
    }

    /// Where playing is, in milliseconds, once the file is loaded.
    pub fn position_ms(&self) -> Option<f64> {
        if !self.frames.running() {
            return None;
        }
        self.player.borrow().as_ref()?.position_ms()
    }

    /// Go to `ms` into the file.
    pub fn seek_ms(&self, ms: f64) {
        if let Some(player) = self.player.borrow().as_ref()
            && let Err(e) = player.seek_ms(ms)
        {
            eprintln!("could not seek: {e}");
        }
    }

    pub fn paused(&self) -> bool {
        self.player
            .borrow()
            .as_ref()
            .is_some_and(mpv::Player::paused)
    }

    pub fn set_paused(&self, paused: bool) {
        if let Some(player) = self.player.borrow().as_ref() {
            let _ = player.set_paused(paused);
        }
    }

    pub fn toggle_pause(&self) {
        if let Some(player) = self.player.borrow().as_ref() {
            let _ = player.toggle_pause();
        }
    }

    /// A frame on (1) or back (-1).
    pub fn frame_step(&self, direction: i32) {
        if let Some(player) = self.player.borrow().as_ref()
            && let Err(e) = player.frame_step(direction)
        {
            eprintln!("could not step a frame: {e}");
        }
    }

    /// Play at `volume` (0 to 100), muted or not, from now on.
    pub fn set_audio(&self, volume: u8, mute: bool) {
        self.audio.set((volume, mute));
        if let Some(player) = self.player.borrow().as_ref()
            && let Err(e) = player.set_audio(volume, mute)
        {
            eprintln!("could not set mpv's volume: {e}");
        }
    }

    /// Stop at once and let the player go.
    pub fn close(&self) {
        self.frames.stop();
        self.player.borrow_mut().take();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    /// Run the timers until `done`, or fifteen seconds have passed; whether
    /// done.
    fn until(done: impl Fn() -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(15) {
            slint::platform::update_timers_and_animations();
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    #[test]
    fn finite_gif_loop_metadata_reaches_the_existing_mpv_player() {
        if !mpv::available() {
            eprintln!("libmpv is not installed here; skipped");
            return;
        }
        let _windows = crate::headless::init();
        let directory = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(directory.path()).unwrap();
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::ViewerPlaybackSettings {
                        always_loop: false,
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        let fixture = hydrus_testkit::fixture_json("viewer_zoom_loop_options.json");
        let gif = fixture["metadata"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["format"] == "GIF" && case["stored_count"] == 1)
            .unwrap();
        let path = directory.path().join("finite.gif");
        std::fs::write(&path, hex::decode(gif["bytes"].as_str().unwrap()).unwrap()).unwrap();
        let playback = Playback::for_store(store.clone());
        playback.play(Some(&path), || Some((20, 16)), |_| {});
        assert_eq!(playback.times_to_play.get(), 1);
        assert!(until(|| playback.paused()));
        assert_eq!(playback.restarts.get(), 1);
        assert!(until(|| playback
            .position_ms()
            .is_some_and(|at| at < RESTARTED_MS)));
        // MPV captures the count at load, like the reference. Enabling forced
        // looping affects its next file load, and does not resume this one.
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::settings::ViewerPlaybackSettings::default(),
                )
            })
            .unwrap();
        assert!(playback.paused());
        playback.play(Some(&path), || Some((20, 16)), |_| {});
        assert_eq!(playback.times_to_play.get(), 0);
        assert!(until(|| playback.restarts.get() >= 2));
        assert!(!playback.paused());
        playback.close();
    }

    #[test]
    fn a_file_plays_through_and_may_stop_at_its_end() {
        if !mpv::available() {
            eprintln!("libmpv is not installed here; skipped");
            return;
        }
        let _windows = crate::headless::init();
        let playback = Playback::new(PathBuf::from("no mpv.conf"));
        // (a second long)
        let video = hydrus_testkit::fixture_path("media/mp4_h264.mp4");
        playback.play(Some(&video), || Some((64, 48)), |_| {});
        assert!(!playback.played_through());
        // round once: played through, and playing on
        assert!(until(|| playback.played_through()));
        assert!(!playback.paused());
        // told to stop at its end: it pauses back at its start
        playback.set_stop_at_end(true);
        assert!(until(|| playback.paused()));
        let at = || playback.position_ms().unwrap_or(f64::MAX);
        assert!(until(|| at() < RESTARTED_MS), "{}", at());
        // a new file plays on, not yet played through
        playback.play(Some(&video), || Some((64, 48)), |_| {});
        assert!(!playback.played_through());
        assert!(until(|| playback.played_through()));
        assert!(!playback.paused());
        playback.close();
    }
}
