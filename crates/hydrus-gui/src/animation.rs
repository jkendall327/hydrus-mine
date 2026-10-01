//! Playing the animations the reference plays with its own player rather
//! than mpv (ugoiras and animated WebP): frames are decoded on a thread of
//! their own, a few ahead of the one shown, and each is shown for its
//! duration, looping. The player says which frame it is on and when, and
//! goes to a frame when asked, as the reference's scanbar has it.

use std::cell::RefCell;
use std::rc::Rc;
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
}

impl Animator {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            timer: slint::Timer::default(),
            running: RefCell::new(None),
        })
    }

    /// Play `frames`, each shown by `show`; or, with none, stop.
    pub fn play(self: &Rc<Self>, frames: Option<Frames>, show: impl Fn(slint::Image) + 'static) {
        self.stop();
        let Some(mut frames) = frames else {
            return;
        };
        let count = frames.len();
        let total_ms = frames.total_ms();
        let durations = frames.durations().to_vec();
        let (sender, receiver) = crossbeam_channel::bounded(AHEAD);
        let (seeks, seeking) = crossbeam_channel::unbounded::<(usize, u64)>();
        let decoding = std::thread::Builder::new()
            .name("animation".into())
            .spawn(move || {
                let (mut generation, mut index, mut at_ms) = (0, 0, 0);
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
            paused: false,
            generation: 0,
            show_one: false,
            status: Status {
                index: 0,
                at_ms: 0,
                frames: count,
                total_ms,
                paused: false,
            },
            durations,
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
            match running.frames.try_recv() {
                // (decoded before the latest seek)
                Ok(frame) if frame.generation != running.generation => 0,
                Ok(frame) => {
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

    pub fn toggle_pause(self: &Rc<Self>) {
        let paused = self.status().is_some_and(|s| s.paused);
        self.set_paused(!paused);
    }

    /// Stop playing; the decoding thread ends at its next frame.
    pub fn stop(&self) {
        self.timer.stop();
        self.running.borrow_mut().take();
    }
}
