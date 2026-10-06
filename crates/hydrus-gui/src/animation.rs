//! Playing the animations the reference plays with its own player rather
//! than mpv (ugoiras and animated WebP): frames are decoded on a thread of
//! their own, a few ahead of the one shown, and each is shown for its
//! duration, looping. The player says which frame it is on and when, and
//! goes to a frame when asked, as the reference's scanbar has it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TryRecvError};
use hydrus_media::animation::Frames;

use crate::thumbnails::Pixels;

/// How many frames are decoded ahead of the one shown.
const AHEAD: usize = 3;

/// The least time a frame is shown, so a zero duration doesn't spin.
const SHORTEST_FRAME_MS: u32 = 10;

/// A frame decoded: its pixels, how long it shows, its index and place in
/// time (ms), and which seek it follows.
struct Decoded {
    pixels: Pixels,
    ms: u32,
    index: usize,
    at_ms: u64,
    generation: u64,
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
    frames: Receiver<Decoded>,
    seeks: Sender<(usize, u64)>,
    show: Box<dyn Fn(slint::Image)>,
    paused: bool,
    /// The seek frames must follow to be shown; and whether to show the
    /// next one even though paused (just seeked).
    generation: u64,
    show_one: bool,
    status: Status,
    durations: Vec<u32>,
    /// How many times it has come round from its last frame to its first
    /// (`_playthrough_count`).
    playthroughs: u32,
    times_to_play: u32,
    /// Whether it stops on its last frame rather than come round
    /// (`StopForSlideshow`), and the first frame, held there until it
    /// plays on.
    stop_at_end: bool,
    held: Option<Decoded>,
    /// The frame shown last, if any.
    last_shown: Option<usize>,
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
        let (sender, receiver) = crossbeam_channel::bounded(AHEAD);
        let (seeks, seeking) = crossbeam_channel::unbounded::<(usize, u64)>();
        let decoding = std::thread::Builder::new()
            .name("animation".into())
            .spawn(move || {
                // An impossible reused-widget index must not silently clamp to
                // a different first frame. A later explicit seek can recover it.
                let (to, mut generation) =
                    if let Some(index) = initial.filter(|index| *index < count) {
                        (index, 0)
                    } else {
                        let Ok(request) = seeking.recv() else {
                            return;
                        };
                        request
                    };
                let Ok(mut at_ms) = frames.seek(to) else {
                    return;
                };
                let mut index = to.min(count.saturating_sub(1));
                // (ends when a frame can't be read, or nothing is watching)
                loop {
                    if let Some((to, new)) = seeking.try_iter().last() {
                        match frames.seek(to) {
                            Ok(before) => {
                                (generation, index, at_ms) =
                                    (new, to.min(count.saturating_sub(1)), before);
                            }
                            Err(_) => break,
                        }
                    }
                    let Ok((raster, ms)) = frames.next_frame() else {
                        break;
                    };
                    let decoded = Decoded {
                        pixels: Pixels::new(&raster),
                        ms,
                        index,
                        at_ms,
                        generation,
                    };
                    if sender.send(decoded).is_err() {
                        break;
                    }
                    index += 1;
                    at_ms += u64::from(ms);
                    if index >= count {
                        (index, at_ms) = (0, 0);
                    }
                }
            });
        if let Err(e) = decoding {
            eprintln!("could not start playing the animation: {e}");
            return;
        }
        *self.running.borrow_mut() = Some(Running {
            frames: receiver,
            seeks,
            show: Box::new(show),
            paused,
            generation: 0,
            show_one: true,
            status: Status {
                index: initial.unwrap_or(0),
                at_ms: 0,
                frames: count,
                total_ms,
                paused,
            },
            durations,
            playthroughs: 0,
            times_to_play,
            stop_at_end: false,
            held: None,
            last_shown: None,
        });
        self.tick();
    }

    /// Show the next frame if it is ready, and wait its duration (or a
    /// moment, for it to be decoded).
    fn tick(self: &Rc<Self>) {
        let wait = {
            let mut running = self.running.borrow_mut();
            let Some(running) = running.as_mut().filter(|r| !r.paused || r.show_one) else {
                return;
            };
            let next = match running.held.take() {
                Some(frame) => Ok(frame),
                None => running.frames.try_recv(),
            };
            match next {
                // (decoded before the latest seek)
                Ok(frame) if frame.generation != running.generation => 0,
                Ok(frame) => {
                    // round from the last frame to the first: played through
                    // (told to stop there, it stays on the last)
                    let frames = running.status.frames;
                    if !running.show_one
                        && frame.index == 0
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
                            running.held = Some(frame);
                            return;
                        }
                    }
                    running.last_shown = Some(frame.index);
                    (running.show)(frame.pixels.image());
                    running.status.index = frame.index;
                    running.status.at_ms = frame.at_ms;
                    if running.show_one {
                        running.show_one = false;
                        if running.paused {
                            return;
                        }
                    }
                    frame.ms.max(SHORTEST_FRAME_MS)
                }
                Err(TryRecvError::Empty) => 5,
                // (a frame couldn't be read: the last one stays)
                Err(TryRecvError::Disconnected) => return,
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
            running.generation += 1;
            running.show_one = true;
            // (frames decoded before it are passed over as they come, which
            // frees the decoder to seek; they aren't emptied out here, as
            // that could take the frame sought too, the decoder running on
            // meanwhile)
            let _ = running.seeks.send((index, running.generation));
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
    /// the first and back (`GotoPreviousOrNextFrame`).
    pub fn step(self: &Rc<Self>, direction: i32) {
        let index = {
            let running = self.running.borrow();
            let Some(running) = running.as_ref() else {
                return;
            };
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
        // A decoded blue frame from generation0 cannot flash after a paused seek.
        let old = animator
            .running
            .borrow()
            .as_ref()
            .unwrap()
            .frames
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert_eq!(old.index, 2);
        animator.running.borrow_mut().as_mut().unwrap().held = Some(old);
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
        // a new file plays on, not yet played through
        animator.play(Some(frames()), |_| {});
        assert!(!animator.played_through());
        assert!(until(&animator, Animator::played_through));
        assert!(!animator.status().unwrap().paused);
    }
}
