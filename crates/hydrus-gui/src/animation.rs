//! Playing the animations the reference plays with its own player rather
//! than mpv (ugoiras and animated WebP): frames are decoded on a thread of
//! their own into a buffer sized from "Memory for video buffer", two thirds
//! of it behind the frame shown and a third ahead, as the reference's
//! `RasterContainerVideo` keeps them (so a short loop is decoded once); and
//! each is shown for its duration, looping. The player says which frame it
//! is on and when, and goes to a frame when asked, as the reference's
//! scanbar has it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use hydrus_gui_model::video_buffer::{self, Buffer};
use hydrus_media::animation::Frames;

use crate::thumbnails::Pixels;

/// The least time a frame is shown, so a zero duration doesn't spin.
const SHORTEST_FRAME_MS: u32 = 10;

thread_local! {
    /// How many frames the players started on this thread have decoded.
    static DECODED: Arc<AtomicU64> = Arc::default();
    /// How many decoding threads the players started on this thread have
    /// running.
    static LIVE: Arc<AtomicU64> = Arc::default();
}

/// Counts a decoding thread while it lives.
struct Live(Arc<AtomicU64>);

impl Live {
    fn start() -> Self {
        let live = LIVE.with(Arc::clone);
        live.fetch_add(1, Ordering::SeqCst);
        Self(live)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How many decoding threads the animation players started on this thread
/// have running: a player that is stopped lets its thread go after the
/// frame it is on.
pub fn decoders_running() -> u64 {
    LIVE.with(|live| live.load(Ordering::SeqCst))
}

/// How many frames the animation players started on this thread (the
/// event loop's) have decoded, all told: a frame kept in the buffer and
/// shown again is not decoded again.
pub fn frames_decoded() -> u64 {
    DECODED.with(|decoded| decoded.load(Ordering::Relaxed))
}

/// A frame decoded: its pixels and how long it shows.
struct Decoded {
    pixels: Pixels,
    ms: u32,
}

impl Decoded {
    fn image(&self) -> slint::Image {
        match &self.pixels {
            Pixels::Rgba(pixels) => slint::Image::from_rgba8(pixels.clone()),
            Pixels::Rgb(pixels) => slint::Image::from_rgb8(pixels.clone()),
        }
    }
}

/// The buffer the decoding thread fills and the player shows from.
struct Decoding {
    buffer: Buffer<Decoded>,
    /// Whether the player has gone (the thread ends), and whether a frame
    /// couldn't be read (the last one shown stays).
    stop: bool,
    failed: bool,
}

struct Shared {
    decoding: Mutex<Decoding>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Decoding> {
        self.decoding
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Keep frame `index` and those round it (`GetReadyForFrame`).
    fn get_ready_for(&self, index: usize) {
        self.lock().buffer.get_ready_for(index);
        self.wake.notify_all();
    }
}

pub(crate) struct Animator {
    timer: slint::Timer,
    running: RefCell<Option<Running>>,
    store: Option<Arc<hydrus_store::Store>>,
    widget_frames: Cell<usize>,
}

/// Where playing is: the frame shown, its place in time (ms), how many
/// frames there are and how long they all take, and whether it is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Status {
    pub index: usize,
    pub at_ms: u64,
    pub frames: usize,
    pub total_ms: u64,
    pub paused: bool,
}

struct Running {
    shared: Arc<Shared>,
    show: Box<dyn Fn(slint::Image)>,
    paused: bool,
    /// The frame to show next; and whether to show it even though paused
    /// (just gone to).
    want: usize,
    show_one: bool,
    status: Status,
    durations: Vec<u32>,
    /// How many times it has come round from its last frame to its first
    /// (`_playthrough_count`).
    playthroughs: u32,
    times_to_play: u32,
    /// Whether it stops on its last frame rather than come round
    /// (`StopForSlideshow`).
    stop_at_end: bool,
    /// The frame shown last, if any.
    last_shown: Option<usize>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
    }
}

impl Running {
    /// The place in time (ms) of frame `index`.
    fn at_ms(&self, index: usize) -> u64 {
        self.durations[..index.min(self.durations.len())]
            .iter()
            .map(|&ms| u64::from(ms))
            .sum()
    }
}

/// Decode the frames `shared`'s buffer asks for until the player goes or a
/// frame can't be read (`THREADRender`).
fn decode(mut frames: Frames, shared: &Shared, decoded: &AtomicU64) {
    let count = frames.len();
    // (where the decoder is: the reference's render thread keeps its own
    // count, which for a one-frame buffer starts a frame behind it; frames
    // are kept as the count says, but decoded where they are)
    let mut position = 0;
    let mut decoding = shared.lock();
    loop {
        if decoding.stop {
            return;
        }
        let Some(job) = decoding.buffer.next_job() else {
            decoding = shared
                .wake
                .wait_timeout(decoding, Duration::from_millis(100))
                .map_or_else(|e| e.into_inner().0, |(guard, _)| guard);
            continue;
        };
        drop(decoding);
        let mut read = || -> hydrus_media::error::Result<_> {
            if let Some(to) = job.seek {
                frames.seek(to)?;
                position = to;
            }
            if let Ok(index) = usize::try_from(job.index)
                && index != position
            {
                frames.seek(index)?;
                position = index;
            }
            let frame = frames.next_frame()?;
            position = (position + 1) % count.max(1);
            Ok(frame)
        };
        let read = read();
        decoding = shared.lock();
        let Ok((raster, ms)) = read else {
            decoding.failed = true;
            return;
        };
        decoded.fetch_add(1, Ordering::Relaxed);
        let frame = Decoded {
            pixels: Pixels::new(&raster),
            ms,
        };
        if decoding.buffer.rendered(job.index, frame) {
            drop(decoding);
            if frames.seek(0).is_err() {
                shared.lock().failed = true;
                return;
            }
            position = 0;
            decoding = shared.lock();
        }
    }
}

impl Animator {
    #[cfg(test)]
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            timer: slint::Timer::default(),
            running: RefCell::new(None),
            store: None,
            widget_frames: Cell::new(1),
        })
    }

    pub fn for_store(store: Arc<hydrus_store::Store>) -> Rc<Self> {
        Rc::new(Self {
            timer: slint::Timer::default(),
            running: RefCell::new(None),
            store: Some(store),
            widget_frames: Cell::new(1),
        })
    }

    /// Play `frames`, each shown by `show`; or, with none, stop.
    #[cfg(test)]
    pub fn play(self: &Rc<Self>, frames: Option<Frames>, show: impl Fn(slint::Image) + 'static) {
        let metadata = frames.as_ref().map(|frames| frames.len() as u64);
        self.play_with_metadata(frames, metadata, false, show);
    }

    /// "Memory for video buffer", as saved when the file is opened.
    fn buffer_bytes(&self) -> u64 {
        self.store
            .as_ref()
            .and_then(|store| {
                store
                    .read(
                        hydrus_store::settings::get::<
                            hydrus_store::reference_options::ReferenceOptions,
                        >,
                    )
                    .ok()
            })
            .map_or(video_buffer::DEFAULT_BYTES, |options| {
                u64::try_from(options.integer("video_buffer_size")).unwrap_or(0)
            })
    }

    /// SetMedia samples the previous widget's count, then installs this one's.
    pub fn play_with_metadata(
        self: &Rc<Self>,
        frames: Option<Frames>,
        metadata_frames: Option<u64>,
        paused: bool,
        show: impl Fn(slint::Image) + 'static,
    ) {
        let previous = self.widget_frames.get();
        self.stop();
        let Some(mut frames) = frames else {
            return;
        };
        self.widget_frames.set(
            metadata_frames
                .and_then(|count| usize::try_from(count).ok())
                .filter(|count| *count != 0)
                .unwrap_or(1),
        );
        let initial = self.store.as_ref().map_or_else(
            || Some(0),
            |store| {
                store
                    .read(hydrus_store::animation_start::load)
                    .ok()
                    .and_then(|settings| settings.frame(previous))
            },
        );
        let count = frames.len();
        if count == 0 {
            return;
        }
        let times_to_play = frames.times_to_play();
        let total_ms = frames.total_ms();
        let durations = frames.durations().to_vec();
        // (sized at the frames' own size, which is the size they are
        // decoded at)
        let kept = video_buffer::frames_kept(
            self.buffer_bytes(),
            frames.dimensions().unwrap_or((0, 0)),
            Some(total_ms),
            Some(count as u64),
        );
        let shared = Arc::new(Shared {
            decoding: Mutex::new(Decoding {
                buffer: Buffer::new(count, kept),
                stop: false,
                failed: false,
            }),
            wake: Condvar::new(),
        });
        // An impossible reused-widget index must not silently clamp to a
        // different first frame; nothing is decoded until a later explicit
        // seek asks for a frame.
        if let Some(index) = initial.filter(|index| *index < count) {
            shared.get_ready_for(index);
        }
        let decoded = DECODED.with(Arc::clone);
        let live = Live::start();
        let decoding = std::thread::Builder::new().name("animation".into()).spawn({
            let shared = shared.clone();
            move || {
                let _live = live;
                decode(frames, &shared, &decoded);
            }
        });
        if let Err(e) = decoding {
            eprintln!("could not start playing the animation: {e}");
            return;
        }
        let want = initial.unwrap_or(0);
        *self.running.borrow_mut() = Some(Running {
            shared,
            show: Box::new(show),
            paused,
            want,
            show_one: initial.is_some_and(|index| index < count),
            status: Status {
                index: want,
                at_ms: 0,
                frames: count,
                total_ms,
                paused,
            },
            durations,
            playthroughs: 0,
            times_to_play,
            stop_at_end: false,
            last_shown: None,
        });
        self.tick();
    }

    /// Show the frame wanted if it is decoded (`GetFrame`, which makes the
    /// buffer ready for the one after), and wait its duration (or a
    /// moment, for it to be decoded).
    fn tick(self: &Rc<Self>) {
        let wait = {
            let mut running = self.running.borrow_mut();
            let Some(running) = running.as_mut().filter(|r| !r.paused || r.show_one) else {
                return;
            };
            let frames = running.status.frames;
            let index = running.want;
            let ready = {
                let decoding = running.shared.lock();
                match decoding.buffer.get(index) {
                    Some(frame) => Ok((frame.image(), frame.ms)),
                    None => Err(decoding.failed),
                }
            };
            match ready {
                Ok((image, ms)) => {
                    // round from the last frame to the first: played through
                    // (told to stop there, it stays on the last)
                    if !running.show_one
                        && index == 0
                        && running.last_shown == Some(frames.saturating_sub(1))
                    {
                        running.playthroughs += 1;
                        let always_loop = self.store.as_ref().is_none_or(|store| {
                            store
                                .read(
                                    hydrus_store::settings::get::<
                                        hydrus_store::settings::ViewerPlaybackSettings,
                                    >,
                                )
                                .unwrap_or_default()
                                .always_loop
                        });
                        if running.stop_at_end
                            || (!always_loop
                                && running.times_to_play != 0
                                && running.playthroughs >= running.times_to_play)
                        {
                            running.paused = true;
                            return;
                        }
                    }
                    running.shared.get_ready_for((index + 1) % frames.max(1));
                    running.last_shown = Some(index);
                    (running.show)(image);
                    running.status.index = index;
                    running.status.at_ms = running.at_ms(index);
                    running.want = (index + 1) % frames.max(1);
                    if running.show_one {
                        running.show_one = false;
                        if running.paused {
                            return;
                        }
                    }
                    ms.max(SHORTEST_FRAME_MS)
                }
                Err(false) => 5,
                // (a frame couldn't be read: the last one stays)
                Err(true) => return,
            }
        };
        let this = Rc::downgrade(self);
        self.timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(u64::from(wait)),
            move || {
                if let Some(this) = this.upgrade() {
                    this.tick();
                }
            },
        );
    }

    /// Where playing is, if anything plays.
    pub fn status(&self) -> Option<Status> {
        self.running.borrow().as_ref().map(|r| Status {
            paused: r.paused,
            ..r.status
        })
    }

    /// Go to frame `index` and show it (`GotoFrame`).
    pub fn goto(self: &Rc<Self>, index: usize) {
        {
            let mut running = self.running.borrow_mut();
            let Some(running) = running.as_mut() else {
                return;
            };
            let index = index.min(running.status.frames.saturating_sub(1));
            running.want = index;
            running.show_one = true;
            running.shared.get_ready_for(index);
        }
        self.timer.stop();
        self.tick();
    }
    /// Go `step_ms` forwards (`direction` 1) or back (-1) from the frame
    /// shown (`SeekDelta`): to the frame showing then, or if that is this
    /// one, the next (or last) frame; never before the first, and past the
    /// end, round to the first.
    pub fn seek_delta(self: &Rc<Self>, direction: i32, step_ms: u64) {
        let index = {
            let running = self.running.borrow();
            let Some(running) = running.as_ref() else {
                return;
            };
            let Status { index, at_ms, .. } = running.status;
            let to = if direction < 0 {
                at_ms.saturating_sub(step_ms)
            } else {
                at_ms + step_ms
            };
            let mut to_index = hydrus_media::animation::frame_index(&running.durations, to);
            if to_index == index {
                to_index = if direction < 0 {
                    to_index.saturating_sub(1)
                } else {
                    to_index + 1
                };
            }
            if to_index >= running.status.frames {
                0
            } else {
                to_index
            }
        };
        self.goto(index);
    }

    /// Go a frame on (`direction` 1) or back (-1), round from the last to
    /// the first and back, and stay there paused (`GotoPreviousOrNextFrame`,
    /// which is `GotoFrame` with its `pause_afterwards`).
    pub fn step(self: &Rc<Self>, direction: i32) {
        let index = {
            let mut running = self.running.borrow_mut();
            let Some(running) = running.as_mut() else {
                return;
            };
            running.paused = true;
            let Status { index, frames, .. } = running.status;
            if frames == 0 {
                return;
            }
            if direction < 0 {
                index.checked_sub(1).unwrap_or(frames - 1)
            } else if index + 1 >= frames {
                0
            } else {
                index + 1
            }
        };
        self.goto(index);
    }

    pub fn set_paused(self: &Rc<Self>, paused: bool) {
        let resumed = {
            let mut running = self.running.borrow_mut();
            let Some(running) = running.as_mut() else {
                return;
            };
            let resumed = running.paused && !paused;
            running.paused = paused;
            resumed
        };
        if resumed {
            self.tick();
        } else if paused {
            self.timer.stop();
        }
    }

    /// Whether it has played through (`HasPlayedOnceThrough`).
    pub fn played_through(&self) -> bool {
        self.running
            .borrow()
            .as_ref()
            .is_some_and(|r| r.playthroughs > 0)
    }

    /// Stop on the last frame rather than come round, or not
    /// (`StopForSlideshow`): until the next file.
    pub fn set_stop_at_end(&self, stop: bool) {
        if let Some(running) = self.running.borrow_mut().as_mut() {
            running.stop_at_end = stop;
        }
    }

    pub fn toggle_pause(self: &Rc<Self>) {
        let paused = self.status().is_some_and(|s| s.paused);
        self.set_paused(!paused);
    }

    /// Stop playing; the decoding thread ends at its next frame.
    pub fn stop(&self) {
        self.timer.stop();
        self.running.borrow_mut().take();
        self.widget_frames.set(1);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn frames() -> Frames {
        let path = hydrus_testkit::fixture_path("media/webp_anim.webp");
        Frames::open(&path, hydrus_core::Mime::AnimationWebp, &[], None).unwrap()
    }

    /// Run the timers until `done`, or ten seconds have passed; whether done.
    fn until(animator: &Animator, done: impl Fn(&Animator) -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            slint::platform::update_timers_and_animations();
            if done(animator) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    #[test]
    fn saved_initial_seek_uses_previous_metadata_and_rejects_held_pre_seek_pixels() {
        let _windows = crate::headless::init();
        let directory = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(directory.path()).unwrap();
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::animation_start::Preferences { fraction: 0.5 },
                )
            })
            .unwrap();
        let qt = hydrus_testkit::fixture_json("animation_start.json");
        let path = directory.path().join("three-frames.webp");
        std::fs::write(
            &path,
            hex::decode(qt["rendered"]["bytes"].as_str().unwrap()).unwrap(),
        )
        .unwrap();
        let open = || Frames::open(&path, hydrus_core::Mime::AnimationWebp, &[], None).unwrap();
        let animator = Animator::for_store(store.clone());
        let seen = Rc::new(RefCell::new(Vec::<[u8; 4]>::new()));
        let show = || {
            let seen = seen.clone();
            move |image: slint::Image| {
                seen.borrow_mut().push(
                    image.to_rgba8().unwrap().as_bytes()[..4]
                        .try_into()
                        .unwrap(),
                );
            }
        };
        // A fresh widget starts at zero even at50%; its next SetMedia uses3.
        animator.play_with_metadata(Some(open()), Some(3), true, show());
        assert!(until(&animator, |_| !seen.borrow().is_empty()));
        assert_eq!(&*seen.borrow(), &[[240, 10, 20, 255]]);
        assert_eq!(
            animator.status().unwrap(),
            Status {
                index: 0,
                at_ms: 0,
                frames: 3,
                total_ms: 720,
                paused: true
            }
        );
        seen.borrow_mut().clear();
        animator.play_with_metadata(Some(open()), Some(3), true, show());
        assert!(until(&animator, |_| !seen.borrow().is_empty()));
        let expected: [u8; 4] = serde_json::from_value(qt["rendered"]["pixel"].clone()).unwrap();
        assert_eq!(&*seen.borrow(), &[expected]);
        assert_eq!(
            animator.status().unwrap().index,
            qt["rendered"]["index"].as_u64().unwrap() as usize
        );
        assert_eq!(
            animator.status().unwrap().at_ms,
            qt["rendered"]["status"][1].as_u64().unwrap()
        );
        // Frames are kept by index, so a frame decoded before a paused seek
        // (the blue one after the green) cannot show in place of the one
        // gone to.
        assert!(until(&animator, |a| a
            .running
            .borrow()
            .as_ref()
            .unwrap()
            .shared
            .lock()
            .buffer
            .get(2)
            .is_some()));
        seen.borrow_mut().clear();
        animator.goto(0);
        assert!(until(&animator, |_| !seen.borrow().is_empty()));
        assert_eq!(&*seen.borrow(), &[[240, 10, 20, 255]]);
        assert!(animator.status().unwrap().paused);
        animator.stop();
        assert!(animator.status().is_none());
        animator.play_with_metadata(Some(open()), None, true, show());
        assert_eq!(animator.widget_frames.get(), 1);
        assert!(until(&animator, |a| a
            .running
            .borrow()
            .as_ref()
            .unwrap()
            .last_shown
            == Some(0)));
        animator.play_with_metadata(Some(open()), Some(0), true, show());
        assert_eq!(animator.widget_frames.get(), 1);
        assert!(until(&animator, |a| a
            .running
            .borrow()
            .as_ref()
            .unwrap()
            .last_shown
            == Some(0)));
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::animation_start::Preferences { fraction: 1.0 },
                )
            })
            .unwrap();
        animator.play_with_metadata(Some(open()), Some(5), true, show());
        assert!(until(&animator, |a| a
            .running
            .borrow()
            .as_ref()
            .unwrap()
            .last_shown
            == Some(0)));
        seen.borrow_mut().clear();
        animator.play_with_metadata(Some(open()), Some(3), true, show());
        assert_eq!(animator.status().unwrap().index, 4);
        assert!(
            seen.borrow().is_empty(),
            "impossible initial request publishes no clamped pixels"
        );
        animator.goto(1);
        assert!(until(&animator, |_| !seen.borrow().is_empty()));
        assert_eq!(&*seen.borrow(), &[expected]);
        assert_eq!(animator.status().unwrap().at_ms, 120);
        assert!(animator.status().unwrap().paused);
        animator.stop();
    }

    #[test]
    fn ugoira_initial_timestamp_and_impossible_reader_request_release_on_stop() {
        let _windows = crate::headless::init();
        let directory = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(directory.path()).unwrap();
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::animation_start::Preferences { fraction: 0.5 },
                )
            })
            .unwrap();
        let path = hydrus_testkit::fixture_path("media/ugoira_json.zip");
        let open =
            || Frames::open(&path, hydrus_core::Mime::AnimationUgoira, &[], Some(5)).unwrap();
        let animator = Animator::for_store(store.clone());
        animator.play_with_metadata(Some(open()), Some(5), true, |_| {});
        assert!(until(&animator, |a| a
            .running
            .borrow()
            .as_ref()
            .unwrap()
            .last_shown
            == Some(0)));
        let mut expected = open();
        assert_eq!(expected.seek(2).unwrap(), 130);
        let expected = Pixels::new(&expected.next_frame().unwrap().0)
            .image()
            .to_rgba8()
            .unwrap();
        let seen = Rc::new(RefCell::new(None));
        animator.play_with_metadata(Some(open()), Some(5), true, {
            let seen = seen.clone();
            move |image| *seen.borrow_mut() = image.to_rgba8()
        });
        assert!(until(&animator, |_| seen.borrow().is_some()));
        let qt = hydrus_testkit::fixture_json("animation_start.json");
        let status = animator.status().unwrap();
        assert_eq!(
            status.index,
            qt["ugoira"]["index"].as_u64().unwrap() as usize
        );
        assert_eq!(status.at_ms, qt["ugoira"]["status"][1].as_u64().unwrap());
        assert!(status.paused);
        assert_eq!(
            seen.borrow().as_ref().unwrap().as_bytes(),
            expected.as_bytes()
        );
        // Previous5 at100% requests4, outside this actual three-frame reader.
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::animation_start::Preferences { fraction: 1.0 },
                )
            })
            .unwrap();
        let token = Arc::new(());
        let weak = Arc::downgrade(&token);
        let blocked = frames().with_icc_reader(Arc::new(move || {
            std::hint::black_box(&token);
            true
        }));
        animator.play_with_metadata(Some(blocked), Some(3), true, |_| {
            panic!("impossible initial frame must not clamp")
        });
        assert_eq!(animator.status().unwrap().index, 4);
        assert!(
            animator
                .running
                .borrow()
                .as_ref()
                .unwrap()
                .last_shown
                .is_none()
        );
        assert!(weak.upgrade().is_some());
        animator.stop();
        assert!(
            until(&animator, |_| weak.upgrade().is_none()),
            "blocked initial-seek worker releases its reader and policy owner"
        );
        assert_eq!(animator.widget_frames.get(), 1);
        animator.play_with_metadata(Some(frames()), Some(3), true, |_| {});
        assert!(until(&animator, |a| a
            .running
            .borrow()
            .as_ref()
            .unwrap()
            .last_shown
            == Some(0)));
        animator.stop();
    }

    #[test]
    fn animation_loop_preference_reaches_decoded_frames_and_live_playthrough_limits() {
        let _windows = crate::headless::init();
        let directory = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(directory.path()).unwrap();
        let fixture = hydrus_testkit::fixture_json("viewer_zoom_loop_options.json");
        let original = hex::decode(fixture["animation_bytes"].as_str().unwrap()).unwrap();
        let offset = original
            .windows(4)
            .position(|bytes| bytes == b"ANIM")
            .unwrap()
            + 12;
        let animator = Animator::for_store(store.clone());
        for case in fixture["loops"].as_array().unwrap() {
            let count = case["count"].as_u64().unwrap() as u16;
            let always_loop = case["always"].as_bool().unwrap();
            store
                .write(move |ctx| {
                    hydrus_store::settings::set(
                        ctx.conn(),
                        &hydrus_store::settings::ViewerPlaybackSettings {
                            always_loop,
                            ..Default::default()
                        },
                    )
                })
                .unwrap();
            let mut data = original.clone();
            data[offset..offset + 2].copy_from_slice(&count.to_le_bytes());
            let path = directory.path().join("animation.webp");
            std::fs::write(&path, data).unwrap();
            let frames = Frames::open(&path, hydrus_core::Mime::AnimationWebp, &[], None).unwrap();
            let frames_count = frames.len();
            animator.play(Some(frames), |_| {});
            if !always_loop && count != 0 {
                assert!(until(&animator, |a| a.status().is_some_and(|s| s.paused)));
                let status = animator.status().unwrap();
                let last = case["states"].as_array().unwrap().last().unwrap();
                assert_eq!(status.index as u64, last["frame"].as_u64().unwrap());
                assert_eq!(status.index, frames_count - 1);
                assert_eq!(
                    u64::from(animator.running.borrow().as_ref().unwrap().playthroughs),
                    last["playthroughs"].as_u64().unwrap()
                );
            } else {
                assert!(until(&animator, |a| a
                    .running
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .playthroughs
                    >= 3));
                assert!(!animator.status().unwrap().paused, "{case:?}");
            }
        }
        // Native Qt checks the preference at the end of each play, so changing
        // it while an existing finite animation plays takes effect next wrap.
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
        assert!(until(&animator, |a| a.status().is_some_and(|s| s.paused)));
        animator.stop();
        assert!(animator.status().is_none());
    }

    #[test]
    fn an_animation_plays_through_and_may_stop_at_its_end() {
        let _windows = crate::headless::init();
        let animator = Animator::new();
        animator.play(Some(frames()), |_| {});
        let count = animator.status().unwrap().frames;
        assert!(count > 1);
        assert!(!animator.played_through());
        // round once: played through, and playing on
        assert!(until(&animator, Animator::played_through));
        assert!(!animator.status().unwrap().paused);
        // told to stop at its end: it stops there, on its last frame
        animator.set_stop_at_end(true);
        assert!(until(&animator, |a| a.status().is_some_and(|s| s.paused)));
        assert_eq!(animator.status().unwrap().index, count - 1);
        // (and stays there)
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(300) {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(2));
        }
        let status = animator.status().unwrap();
        assert!(status.paused && status.index == count - 1, "{status:?}");
        // (as the reference's Animation does on coming round with
        // `StopForSlideshow`: paused on its last frame, played through)
        let recorded = hydrus_testkit::fixture_json("animation_playback.json");
        let last = recorded["webp"]["stop_for_slideshow"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        assert_eq!(status.index as u64, last["index"].as_u64().unwrap());
        assert_eq!(status.paused, last["paused"].as_bool().unwrap());
        assert_eq!(animator.played_through(), last["played_through"].as_bool().unwrap());
        // a new file plays on, not yet played through
        animator.play(Some(frames()), |_| {});
        assert!(!animator.played_through());
        assert!(until(&animator, Animator::played_through));
        assert!(!animator.status().unwrap().paused);
    }
}
