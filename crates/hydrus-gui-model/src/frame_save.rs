//! What a window remembers of itself when it closes, as the reference's
//! `SaveTLWSizeAndPosition` decides it over the displays it can see: nothing
//! while minimised or hidden; its place only once checked by the off-screen
//! rescue; and, while maximised or fullscreen, its earlier size and place
//! kept, moved to the same spot on a new display if the window was carried
//! to one.

use hydrus_core::windows::{FrameLocation, WindowState};
use hydrus_store::settings::WindowRescueSettings;

use crate::window_rescue::{self, Point, Rect, Screen};

/// `SaveTLWSizeAndPosition`'s nudge for a maximised window's new place.
const JUST_OFF_THE_CORNER: i64 = 20;

/// What the window and the displays look like as it is saved.
#[derive(Debug, Clone, Copy)]
pub struct Surroundings<'a> {
    pub minimised: bool,
    pub visible: bool,
    /// The displays, the first being the primary one.
    pub screens: &'a [Screen],
    /// The display the window is on (`tlw.screen()`), by place in `screens`.
    pub window_screen: Option<usize>,
    pub rescue: &'a WindowRescueSettings,
}

fn screen_at(screens: &[Screen], point: Point) -> Option<usize> {
    screens.iter().position(|s| s.geometry.contains(point))
}

fn fits(rect: Rect, top_left: Point, size: (i32, i32)) -> bool {
    rect.contains(top_left)
        && rect.contains((
            top_left.0 + i64::from(size.0),
            top_left.1 + i64::from(size.1),
        ))
}

/// The frame after saving a window in `state` (its frame's top-left being
/// `state.position`).
#[must_use]
pub fn saved(
    frame: &FrameLocation,
    state: WindowState,
    around: &Surroundings<'_>,
) -> FrameLocation {
    if around.minimised || !around.visible {
        return frame.clone();
    }
    let mut out = frame.clone();
    if frame.remember_size {
        out.maximised = state.maximised;
        out.fullscreen = state.fullscreen;
    }
    let position = (i64::from(state.position.0), i64::from(state.position.1));
    let Some(mut safe) =
        window_rescue::safe_position(position, None, around.rescue, around.screens)
    else {
        return out;
    };
    let mut worth_saving_size = true;
    let mut worth_saving_position = true;
    if (out.maximised || out.fullscreen)
        && let Some(last) = frame.last_position
    {
        worth_saving_size = false;
        let last = (i64::from(last.0), i64::from(last.1));
        let saved_screen = screen_at(around.screens, last);
        if around.window_screen.is_none() || saved_screen == around.window_screen {
            worth_saving_position = false;
        }
        if worth_saving_position {
            // (just off the corner on the screen it was carried to)
            safe = (safe.0 + JUST_OFF_THE_CORNER, safe.1 + JUST_OFF_THE_CORNER);
            // or the same place on it as it had on the last one, if that fits
            if let (Some(now), Some(then), Some(size)) =
                (around.window_screen, saved_screen, frame.last_size)
            {
                let local = (
                    last.0 - around.screens[then].geometry.x,
                    last.1 - around.screens[then].geometry.y,
                );
                let target = (
                    local.0 + around.screens[now].geometry.x,
                    local.1 + around.screens[now].geometry.y,
                );
                if fits(around.screens[now].geometry, target, size) {
                    safe = target;
                }
            }
        }
    }
    if frame.remember_size && worth_saving_size {
        out.last_size = Some(state.size);
    }
    if frame.remember_position && worth_saving_position {
        #[allow(clippy::cast_possible_truncation)] // (screen coordinates)
        {
            out.last_position = Some((safe.0 as i32, safe.1 as i32));
        }
    }
    out
}
