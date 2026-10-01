//! Playing a file in a window through mpv (video, audio and most
//! animations, as the reference's defaults have it): the player is made when
//! first needed, and its frames are shown as they come.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate::mpv;

pub(crate) struct Playback {
    /// The store's `mpv.conf` (else hydrus's default options apply).
    conf: PathBuf,
    player: RefCell<Option<mpv::Player>>,
    frames: slint::Timer,
}

impl Playback {
    pub fn new(conf: PathBuf) -> Rc<Self> {
        Rc::new(Self {
            conf,
            player: RefCell::new(None),
            frames: slint::Timer::default(),
        })
    }

    /// Play `path`, its frames at the size `size` gives shown by `show`; or,
    /// with none (or no libmpv), stop.
    pub fn play(
        self: &Rc<Self>,
        path: Option<&Path>,
        size: impl Fn() -> Option<(u32, u32)> + 'static,
        show: impl Fn(slint::Image) + 'static,
    ) {
        let Some(path) = path.filter(|_| mpv::available()) else {
            self.stop();
            return;
        };
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
        if let Some(player) = player.as_ref()
            && let Err(e) = player.load(path)
        {
            eprintln!("mpv could not play {}: {e}", path.display());
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
            },
        );
    }

    /// Stop playing (the player is kept for the next file).
    pub fn stop(&self) {
        self.frames.stop();
        if let Some(player) = self.player.borrow().as_ref() {
            let _ = player.stop();
        }
    }

    pub fn toggle_pause(&self) {
        if let Some(player) = self.player.borrow().as_ref() {
            let _ = player.toggle_pause();
        }
    }

    /// Stop at once and let the player go.
    pub fn close(&self) {
        self.frames.stop();
        self.player.borrow_mut().take();
    }
}
