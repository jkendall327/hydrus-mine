//! Playing the animations the reference plays with its own player rather
//! than mpv (ugoiras and animated WebP): frames are decoded on a thread of
//! their own, a few ahead of the one shown, and each is shown for its
//! duration, looping.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crossbeam_channel::{Receiver, TryRecvError};
use hydrus_media::animation::Frames;

use crate::thumbnails::Pixels;

/// How many frames are decoded ahead of the one shown.
const AHEAD: usize = 3;

/// The least time a frame is shown, so a zero duration doesn't spin.
const SHORTEST_FRAME_MS: u32 = 10;

pub(crate) struct Animator {
    timer: slint::Timer,
    running: RefCell<Option<Running>>,
}

struct Running {
    frames: Receiver<(Pixels, u32)>,
    show: Box<dyn Fn(slint::Image)>,
    paused: bool,
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
        let (sender, receiver) = crossbeam_channel::bounded(AHEAD);
        let decoding = std::thread::Builder::new()
            .name("animation".into())
            .spawn(move || {
                // (ends when a frame can't be read, or nothing is watching)
                while let Ok((raster, ms)) = frames.next_frame() {
                    if sender.send((Pixels::new(&raster), ms)).is_err() {
                        break;
                    }
                }
            });
        if let Err(e) = decoding {
            eprintln!("could not start playing the animation: {e}");
            return;
        }
        *self.running.borrow_mut() = Some(Running {
            frames: receiver,
            show: Box::new(show),
            paused: false,
        });
        self.tick();
    }

    /// Show the next frame if it is ready, and wait its duration (or a
    /// moment, for it to be decoded).
    fn tick(self: &Rc<Self>) {
        let wait = {
            let running = self.running.borrow();
            let Some(running) = running.as_ref().filter(|r| !r.paused) else {
                return;
            };
            match running.frames.try_recv() {
                Ok((pixels, ms)) => {
                    (running.show)(pixels.image());
                    ms.max(SHORTEST_FRAME_MS)
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

    pub fn toggle_pause(self: &Rc<Self>) {
        let resumed = {
            let mut running = self.running.borrow_mut();
            let Some(running) = running.as_mut() else {
                return;
            };
            running.paused = !running.paused;
            !running.paused
        };
        if resumed {
            self.tick();
        } else {
            self.timer.stop();
        }
    }

    /// Stop playing; the decoding thread ends at its next frame.
    pub fn stop(&self) {
        self.timer.stop();
        self.running.borrow_mut().take();
    }
}
