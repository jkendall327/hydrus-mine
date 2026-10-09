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
    /// The file it was last asked to play, whether or not libmpv is there
    /// to play it (what tests without libmpv look at).
    target: RefCell<Option<PathBuf>>,
    /// Whether the file stops at its end rather than play again
    /// (`StopForSlideshow`).
    stop_at_end: Cell<bool>,
    /// How many times it has gone back to its start, as it loops (or is
    /// sought there), and where it was last seen, in milliseconds.
    restarts: Cell<u32>,
    last_position: Cell<Option<f64>>,
    /// The mpv options the player was last given, and when they were last
    /// checked against the saved options (an open player takes the
    /// options' changes, as the reference's do on Options OK).
    plan: RefCell<Option<hydrus_gui_model::mpv_options::Plan>>,
    plan_checked: Cell<Option<std::time::Instant>>,
    /// The last positions seen, with when (ms since the first), for a test
    /// that says why it timed out.
    #[cfg(test)]
    samples: RefCell<std::collections::VecDeque<(u128, f64)>>,
}

thread_local! {
    /// The players made for a store on this thread, for [`live_property`].
    static LIVE: RefCell<Vec<std::rc::Weak<Playback>>> = const { RefCell::new(Vec::new()) };
}

/// An mpv property (`loop-file`, `audio-device`, ...) of each player made
/// for a store on this thread and still playing, oldest first: what tests
/// see of the players inside windows.
#[doc(hidden)]
pub fn live_property(name: &str) -> Vec<Option<String>> {
    LIVE.with(|live| {
        live.borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
            .filter_map(|playback| {
                let player = playback.player.try_borrow().ok()?;
                Some(player.as_ref()?.string_property(name))
            })
            .collect()
    })
}

/// Going back to within this many milliseconds of the start is a restart
/// (the reference counts seeks to under a tenth of a second; seen every
/// 10 milliseconds or so, a little more).
const RESTARTED_MS: f64 = 250.0;

/// Whether the file at `path` (named by its hash) has sound; one that isn't
/// known counts as having some.
fn has_audio(store: &hydrus_store::Store, path: &Path) -> bool {
    let Some(hash) = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.parse::<hydrus_core::Sha256>().ok())
    else {
        return true;
    };
    store
        .read(|conn| {
            let Some(id) = hydrus_store::master::hash_id(conn, &hash)? else {
                return Ok(true);
            };
            Ok(hydrus_store::media::load_basic(conn, &[id])?
                .into_iter()
                .next()
                .and_then(|media| media.info)
                .is_none_or(|info| info.has_audio))
        })
        .unwrap_or(true)
}

impl Playback {
    pub fn new(conf: PathBuf) -> Rc<Self> {
        Rc::new(Self {
            conf,
            store: None,
            times_to_play: Cell::new(0),
            player: RefCell::new(None),
            frames: slint::Timer::default(),
            audio: Cell::new((100, false)),
            target: RefCell::new(None),
            stop_at_end: Cell::new(false),
            restarts: Cell::new(0),
            last_position: Cell::new(None),
            plan: RefCell::new(None),
            plan_checked: Cell::new(None),
            #[cfg(test)]
            samples: RefCell::default(),
        })
    }

    pub fn for_store(store: Arc<hydrus_store::Store>) -> Rc<Self> {
        let mut playback = Self::new(store.dir().join("mpv.conf"));
        Rc::get_mut(&mut playback)
            .expect("new player is exclusively owned")
            .store = Some(store);
        LIVE.with(|live| {
            let mut live = live.borrow_mut();
            live.retain(|p| p.strong_count() > 0);
            live.push(Rc::downgrade(&playback));
        });
        playback
    }

    /// The mpv options for `path` from the saved options.
    fn plan_for(&self, path: &Path) -> hydrus_gui_model::mpv_options::Plan {
        let options = self.store.as_ref().map_or_else(Default::default, |store| {
            store
                .read(
                    hydrus_store::settings::get::<
                        hydrus_store::reference_options::ReferenceOptions,
                    >,
                )
                .unwrap_or_default()
        });
        hydrus_gui_model::mpv_options::Plan::for_file(
            &options,
            self.store
                .as_ref()
                .is_none_or(|store| has_audio(store, path)),
        )
    }

    /// Give the player the saved mpv options again if they changed since it
    /// was given them (checked twice a second).
    fn follow_options(&self, player: &mpv::Player) {
        let due = self
            .plan_checked
            .get()
            .is_none_or(|at| at.elapsed() >= Duration::from_millis(500));
        let Some(path) = self.target.borrow().clone().filter(|_| due) else {
            return;
        };
        self.plan_checked.set(Some(std::time::Instant::now()));
        let plan = self.plan_for(&path);
        if self.plan.borrow().as_ref() == Some(&plan) {
            return;
        }
        if let Err(e) = player.set_playback_options(&plan) {
            eprintln!("could not set mpv's options: {e}");
        }
        *self.plan.borrow_mut() = Some(plan);
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
        *self.target.borrow_mut() = path.map(Path::to_path_buf);
        let Some(path) = path.filter(|_| mpv::available()) else {
            // (without libmpv nothing plays, but what it was asked to play
            // stays known)
            let target = self.target.borrow().clone();
            self.stop();
            *self.target.borrow_mut() = target;
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
            let plan = self.plan_for(path);
            if let Err(e) = player.set_playback_options(&plan) {
                eprintln!("could not set mpv's options: {e}");
            }
            *self.plan.borrow_mut() = Some(plan);
            self.plan_checked.set(Some(std::time::Instant::now()));
            if let Err(e) = player.load(path) {
                eprintln!("mpv could not play {}: {e}", path.display());
            }
            // (the load is asynchronous: a position read before it finishes
            // is the last file's, and is no part of this one's)
            self.last_position.set(None);
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
                this.follow_options(player);
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
        #[cfg(test)]
        {
            static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
            let mut samples = self.samples.borrow_mut();
            samples.push_back((
                START
                    .get_or_init(std::time::Instant::now)
                    .elapsed()
                    .as_millis(),
                position,
            ));
            if samples.len() > 80 {
                samples.pop_front();
            }
        }
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
        *self.target.borrow_mut() = None;
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

    /// The file it was last asked to play, if it was not stopped since.
    pub fn target(&self) -> Option<PathBuf> {
        self.target.borrow().clone()
    }

    /// The volume and mute it plays at.
    pub fn audio(&self) -> (u8, bool) {
        self.audio.get()
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

    /// Run the timers until `done`, or `limit` has passed; whether done.
    fn until_within(limit: Duration, done: impl Fn() -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < limit {
            slint::platform::update_timers_and_animations();
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    /// How long to wait for libmpv to loop a file. A two-frame, 80 ms GIF
    /// takes mpv anything from 0.15 s to several seconds (nine, once) to come
    /// round, whatever the timing options, with the position sampled every
    /// 10 ms the whole while: the file sits on its last frame. So loops are
    /// waited for patiently; a pass costs only what mpv takes.
    const LOOP_PATIENCE: Duration = Duration::from_secs(90);

    /// `mpv_null_audio_on_silent_media` asks the store whether the file at a
    /// path (named by its hash) has sound: it does if the store says so, and
    /// where the store or the name cannot say.
    #[test]
    fn a_file_has_sound_as_the_store_says_and_by_default_where_it_cannot() {
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let directory = tempfile::tempdir().unwrap();
        hydrus_store::import::import_legacy(
            legacy.path(),
            &directory.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = hydrus_store::Store::open(directory.path()).unwrap();
        let (silent, loud) = store
            .write(|ctx| {
                let ids: Vec<u32> = ctx
                    .conn()
                    .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 2")?
                    .query_map([], |row| row.get(0))?
                    .collect::<Result<_, _>>()?;
                ctx.conn()
                    .execute("UPDATE files SET has_audio = 0 WHERE hash_id = ?", [ids[0]])?;
                ctx.conn()
                    .execute("UPDATE files SET has_audio = 1 WHERE hash_id = ?", [ids[1]])?;
                let name = |id: u32| -> hydrus_store::Result<String> {
                    let hash = hydrus_store::master::hash(ctx.conn(), hydrus_core::HashId(id))?;
                    Ok(format!("{}.mp4", hash.expect("a hash")))
                };
                Ok((name(ids[0])?, name(ids[1])?))
            })
            .unwrap();
        assert!(!has_audio(&store, Path::new(&silent)));
        assert!(has_audio(&store, Path::new(&loud)));
        // no such hash in the store, and no hash in the name: sound
        let unknown = format!("{}.mp4", "ab".repeat(32));
        assert!(has_audio(&store, Path::new(&unknown)));
        assert!(has_audio(&store, Path::new("not-a-hash.mp4")));
    }

    #[test]
    fn finite_gif_loop_metadata_reaches_the_existing_mpv_player() {
        if mpv::skip_without_libmpv() {
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
        assert!(until_within(LOOP_PATIENCE, || playback.paused()));
        assert_eq!(playback.restarts.get(), 1);
        assert!(until_within(LOOP_PATIENCE, || playback
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
        // (and the next file is played by a player of its own, so nothing
        // of the last one's position or pause is left to read as this one's)
        playback.close();
        let playback = Playback::for_store(store.clone());
        playback.play(Some(&path), || Some((20, 16)), |_| {});
        assert_eq!(playback.times_to_play.get(), 0);
        assert!(
            until_within(LOOP_PATIENCE, || playback.restarts.get() >= 2),
            "restarts {}, at {:?}ms, paused {}, times to play {}, last samples (ms since start, position) {:?}",
            playback.restarts.get(),
            playback.position_ms(),
            playback.paused(),
            playback.times_to_play.get(),
            playback.samples.borrow(),
        );
        assert!(!playback.paused());
        playback.close();
    }

    #[test]
    fn a_file_plays_through_and_may_stop_at_its_end() {
        if mpv::skip_without_libmpv() {
            return;
        }
        let _windows = crate::headless::init();
        let playback = Playback::new(PathBuf::from("no mpv.conf"));
        // (a second long)
        let video = hydrus_testkit::fixture_path("media/mp4_h264.mp4");
        playback.play(Some(&video), || Some((64, 48)), |_| {});
        assert!(!playback.played_through());
        // round once: played through, and playing on
        assert!(until_within(LOOP_PATIENCE, || playback.played_through()));
        assert!(!playback.paused());
        // told to stop at its end: it pauses back at its start
        playback.set_stop_at_end(true);
        assert!(until_within(LOOP_PATIENCE, || playback.paused()));
        let at = || playback.position_ms().unwrap_or(f64::MAX);
        assert!(
            until_within(LOOP_PATIENCE, || at() < RESTARTED_MS),
            "{}",
            at()
        );
        // a new file plays on, not yet played through
        playback.play(Some(&video), || Some((64, 48)), |_| {});
        assert!(!playback.played_through());
        assert!(until_within(LOOP_PATIENCE, || playback.played_through()));
        assert!(!playback.paused());
        playback.close();
    }
}
