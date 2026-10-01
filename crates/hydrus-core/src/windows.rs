//! Where the client's windows open, and how big (the reference's
//! `frame_locations`, as `ClientGUITopLevelWindows` applies and saves them).

use serde::{Deserialize, Serialize};

/// A window's remembered frame: whether it keeps its size and place, the
/// last it had, and whether it was maximised or fullscreen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameLocation {
    pub remember_size: bool,
    pub remember_position: bool,
    pub last_size: Option<(i32, i32)>,
    pub last_position: Option<(i32, i32)>,
    /// How it grows to fit its screen when it has no size of its own.
    pub default_gravity: (i32, i32),
    /// Where it opens with no position of its own: `topleft`, `center` or
    /// `mouse`.
    pub default_position: String,
    pub maximised: bool,
    pub fullscreen: bool,
}

impl FrameLocation {
    /// The main window's, in a new client: 800x600 at (20, 20), maximised.
    pub fn main_gui() -> Self {
        Self {
            remember_size: true,
            remember_position: true,
            last_size: Some((800, 600)),
            last_position: Some((20, 20)),
            default_gravity: (-1, -1),
            default_position: "topleft".into(),
            maximised: true,
            fullscreen: false,
        }
    }

    /// The media viewer's, in a new client: maximised and fullscreen.
    pub fn media_viewer() -> Self {
        Self {
            remember_size: true,
            remember_position: true,
            last_size: Some((640, 480)),
            last_position: Some((70, 70)),
            default_gravity: (-1, -1),
            default_position: "topleft".into(),
            maximised: true,
            fullscreen: true,
        }
    }
}

/// The main window's and the media viewer's frames, and whether a media
/// viewer's is saved when it closes
/// (`save_media_viewer_window_size_and_position_on_close`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    pub main_gui: FrameLocation,
    pub media_viewer: FrameLocation,
    pub save_media_viewer_on_close: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            main_gui: FrameLocation::main_gui(),
            media_viewer: FrameLocation::media_viewer(),
            save_media_viewer_on_close: false,
        }
    }
}

/// A window's state when it is saved: its size and place, and whether it
/// is maximised or fullscreen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowState {
    pub size: (i32, i32),
    pub position: (i32, i32),
    pub maximised: bool,
    pub fullscreen: bool,
}

impl FrameLocation {
    /// The frame after saving a window in `state` (`SaveTLWSizeAndPosition`,
    /// less its screen checks): whether it is maximised or fullscreen only
    /// if it remembers its size; its size and place only if neither (once
    /// it has a place to go back to), and only those it remembers.
    #[must_use]
    pub fn saved(&self, state: WindowState) -> Self {
        let mut out = self.clone();
        if self.remember_size {
            out.maximised = state.maximised;
            out.fullscreen = state.fullscreen;
        }
        let worth_saving = !((out.maximised || out.fullscreen) && self.last_position.is_some());
        if worth_saving {
            if self.remember_size {
                out.last_size = Some(state.size);
            }
            if self.remember_position {
                out.last_position = Some(state.position);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameLocation, WindowState};

    #[test]
    fn a_saved_frame_keeps_what_it_remembers() {
        let windowed = WindowState {
            size: (1000, 700),
            position: (40, 50),
            maximised: false,
            fullscreen: false,
        };
        // a window it remembers: its size and place, no longer maximised
        let frame = FrameLocation::main_gui().saved(windowed);
        assert_eq!(frame.last_size, Some((1000, 700)));
        assert_eq!(frame.last_position, Some((40, 50)));
        assert!(!frame.maximised && !frame.fullscreen);
        // maximised or fullscreen: so, but the size and place to go back
        // to stay as they were
        let full = FrameLocation::main_gui().saved(WindowState {
            fullscreen: true,
            ..windowed
        });
        assert!(full.fullscreen && !full.maximised);
        assert_eq!(full.last_size, Some((800, 600)));
        assert_eq!(full.last_position, Some((20, 20)));
        // one that remembers neither keeps nothing, not even maximised
        let forgetful = FrameLocation {
            remember_size: false,
            remember_position: false,
            ..FrameLocation::main_gui()
        };
        assert_eq!(forgetful.saved(windowed), forgetful);
    }
}
