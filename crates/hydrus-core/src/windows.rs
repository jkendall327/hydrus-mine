//! Where the client's windows open, and how big (the reference's
//! `frame_locations`, as `ClientGUITopLevelWindows` applies and saves them).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// Remaining reference frame keys, including imported unknown keys.
    pub other_frames: BTreeMap<String, FrameLocation>,
    pub save_media_viewer_on_close: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            main_gui: FrameLocation::main_gui(),
            media_viewer: FrameLocation::media_viewer(),
            other_frames: default_other_frames(),
            save_media_viewer_on_close: false,
        }
    }
}

impl WindowSettings {
    /// The complete editable reference table, preserving unrecognised frame names.
    pub fn frames(&self) -> BTreeMap<String, FrameLocation> {
        let mut frames = self.other_frames.clone();
        frames.insert("main_gui".into(), self.main_gui.clone());
        frames.insert("media_viewer".into(), self.media_viewer.clone());
        frames
    }

    pub fn set_frame(&mut self, name: &str, frame: FrameLocation) {
        match name {
            "main_gui" => {
                self.main_gui = frame;
            }
            "media_viewer" => {
                self.media_viewer = frame;
            }
            _ => {
                self.other_frames.insert(name.into(), frame);
            }
        }
    }

    pub fn frame(&self, name: &str) -> Option<&FrameLocation> {
        match name {
            "main_gui" => Some(&self.main_gui),
            "media_viewer" => Some(&self.media_viewer),
            _ => self.other_frames.get(name),
        }
    }
}

fn default_other_frames() -> BTreeMap<String, FrameLocation> {
    let ordinary = FrameLocation {
        remember_size: false,
        remember_position: false,
        last_size: None,
        last_position: None,
        default_gravity: (-1, -1),
        default_position: "topleft".into(),
        maximised: false,
        fullscreen: false,
    };
    let mut frames = BTreeMap::new();
    for name in [
        "file_import_status",
        "gallery_import_log",
        "local_import_filename_tagging",
        "manage_options_dialog",
        "manage_subscriptions_dialog",
        "edit_subscription_dialog",
        "manage_tags_dialog",
        "manage_tags_frame",
        "regular_dialog",
        "review_services",
        "deeply_nested_dialog",
        "file_history_chart",
        "mr_bones",
        "manage_urls_dialog",
        "manage_times_dialog",
        "manage_notes_dialog",
        "export_files_frame",
        "quick_select_dialog",
        "quick_yesno_dialog",
        "quick_entry_dialog",
    ] {
        let mut frame = ordinary.clone();
        if matches!(
            name,
            "file_import_status"
                | "gallery_import_log"
                | "manage_subscriptions_dialog"
                | "edit_subscription_dialog"
                | "file_history_chart"
                | "mr_bones"
                | "manage_urls_dialog"
                | "manage_times_dialog"
                | "manage_notes_dialog"
                | "export_files_frame"
        ) {
            frame.remember_size = true;
            frame.remember_position = true;
        }
        match name {
            "local_import_filename_tagging" => {
                frame.remember_size = true;
            }
            "review_services" => {
                frame.remember_position = true;
            }
            "manage_subscriptions_dialog" | "edit_subscription_dialog" => {
                frame.default_gravity = (1, -1);
            }
            "manage_tags_dialog" | "manage_tags_frame" => {
                frame.default_gravity = (-1, 1);
            }
            "file_history_chart" => {
                frame.last_size = Some((960, 720));
            }
            "quick_select_dialog" | "quick_yesno_dialog" | "quick_entry_dialog" => {
                "center".clone_into(&mut frame.default_position);
            }
            _ => {}
        }
        frames.insert(name.into(), frame);
    }
    frames
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
