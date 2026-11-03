//! Where a window with no remembered size or place opens, as the reference's
//! `GetSafeSize` and `SetInitialTLWSizeAndPosition` decide it: its default
//! gravity grows it toward its parent, its default position puts it at its
//! parent's top-left, centre, or the mouse. All in logical pixels.

use hydrus_core::windows::FrameLocation;

/// `CHILD_POSITION_PADDING`.
pub const CHILD_POSITION_PADDING: i32 = 24;

/// A rectangle, as `QRect` holds one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    /// `QRect::center`: halfway between its first pixel and its last
    /// (`x + width - 1`), rounded toward zero as C++ divides (so a
    /// rectangle left of or above the origin rounds the other way).
    pub fn center(self) -> (i32, i32) {
        let half = |first: i32, length: i32| {
            i32::try_from((i64::from(first) * 2 + i64::from(length) - 1) / 2).unwrap_or(first)
        };
        (half(self.x, self.width), half(self.y, self.height))
    }
}

/// The window an opening one is a child of: the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent {
    /// Its frame geometry.
    pub frame: Rect,
    pub fullscreen: bool,
}

/// What the placement can see of its surroundings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Surroundings {
    /// The main window, unless the opening one is the main window.
    pub parent: Option<Parent>,
    /// The display the window opens on.
    pub display: Option<Rect>,
    /// The mouse, in desktop coordinates.
    pub mouse: Option<(i32, i32)>,
    /// What a window's frame (its title bar and borders) adds to its size,
    /// as `frameGeometry().size() - size()`.
    pub frame_padding: (i32, i32),
}

/// The size and place a window opens at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub size: (i32, i32),
    /// None when the window should stay where its toolkit puts it.
    pub position: Option<(i32, i32)>,
}

/// `GetSafeSize`: the size hint, or the parent's available size (its frame less
/// the frame padding, and the child padding on both sides) where the gravity is not -1, kept inside the
/// display.
pub fn safe_size(hint: (i32, i32), gravity: (i32, i32), around: &Surroundings) -> (i32, i32) {
    let (mut width, mut height) = hint;
    let (pad_width, pad_height) = around.frame_padding;
    if let Some(parent) = around.parent {
        // (a fullscreen parent has no frame to take from its size)
        let (taken_width, taken_height) = if parent.fullscreen {
            (0, 0)
        } else {
            (pad_width, pad_height)
        };
        let (parent_width, parent_height) = (
            parent.frame.width - taken_width,
            parent.frame.height - taken_height,
        );
        if gravity.0 != -1 {
            let max = parent_width - 2 * CHILD_POSITION_PADDING;
            width = (f64::from(gravity.0) * f64::from(max)) as i32;
        }
        if gravity.1 != -1 {
            let max = parent_height - 2 * CHILD_POSITION_PADDING;
            height = (f64::from(gravity.1) * f64::from(max)) as i32;
        }
    }
    if let Some(display) = around.display {
        width = width.min(display.width - pad_width - 2 * CHILD_POSITION_PADDING);
        height = height.min(display.height - pad_height - 2 * CHILD_POSITION_PADDING);
    }
    (width, height)
}

/// `SlideOffScreenTLWUpAndLeft`: back inside the display's right and bottom
/// edges (less the child padding), never moving it right or down.
fn slide_up_and_left(position: (i32, i32), size: (i32, i32), display: Rect) -> (i32, i32) {
    let right = position.0 + size.0 - 1;
    let bottom = position.1 + size.1 - 1;
    let display_right = display.x + display.width - CHILD_POSITION_PADDING;
    let display_bottom = display.y + display.height - CHILD_POSITION_PADDING;
    let mut position = position;
    if right > display_right {
        position.0 += (display_right - right).min(0);
    }
    if bottom > display_bottom {
        position.1 += (display_bottom - bottom).min(0);
    }
    position
}

/// `SetInitialTLWSizeAndPosition`, less the off-screen rescue (the opening
/// window's own `OpeningRescue` does that, or [`fitted`]).
pub fn initial(frame: &FrameLocation, hint: (i32, i32), around: &Surroundings) -> Placement {
    let size = initial_size(frame, hint, around);
    let Some(opening) = opening(frame, size, around) else {
        return Placement {
            size,
            position: None,
        };
    };
    let mut position = opening.desired;
    if opening.slide
        && let Some(display) = around.display
    {
        position = slide_up_and_left(position, size, display);
    }
    Placement {
        size,
        position: Some(position),
    }
}

/// The size `SetInitialTLWSizeAndPosition` gives an opening window: its
/// remembered size, or its size hint grown by its default gravity.
pub fn initial_size(frame: &FrameLocation, hint: (i32, i32), around: &Surroundings) -> (i32, i32) {
    match frame.last_size.filter(|_| frame.remember_size) {
        Some(size) => size,
        None => safe_size(hint, frame.default_gravity, around),
    }
}

/// Where `SetInitialTLWSizeAndPosition` wants an opening window, before it
/// is fitted to the screens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    /// Its remembered place, or its default one.
    pub desired: (i32, i32),
    /// Whether it is slid back inside its display afterward (a default
    /// top-left place).
    pub slide: bool,
    /// Whether the user is told when it is rescued from off the screens
    /// (`we_care_about_off_screen_messages`): not for a place by the mouse,
    /// nor a default place with no main window to be relative to.
    pub report: bool,
}

/// Where a window of `size` wants to open by its frame, before it is fitted
/// to the screens; None where it stays where its toolkit puts it (a default
/// top-left or centre place with neither a main window nor a display).
pub fn opening(frame: &FrameLocation, size: (i32, i32), around: &Surroundings) -> Option<Opening> {
    let padding = (CHILD_POSITION_PADDING, CHILD_POSITION_PADDING);
    let mut opening = Opening {
        desired: padding,
        slide: false,
        report: true,
    };
    if let Some(last) = frame.last_position.filter(|_| frame.remember_position) {
        opening.desired = last;
        return Some(opening);
    }
    let me = Rect {
        x: 0,
        y: 0,
        width: size.0,
        height: size.1,
    };
    match frame.default_position.as_str() {
        "topleft" => {
            opening.slide = true;
            if let Some(parent) = around.parent {
                opening.desired = (
                    parent.frame.x + CHILD_POSITION_PADDING,
                    parent.frame.y + CHILD_POSITION_PADDING,
                );
            } else {
                opening.report = false;
                let d = around.display?;
                opening.desired = (d.x + CHILD_POSITION_PADDING, d.y + CHILD_POSITION_PADDING);
            }
        }
        "center" => {
            if let Some(parent) = around.parent {
                let (cx, cy) = parent.frame.center();
                let (mx, my) = me.center();
                opening.desired = (cx - mx, cy - my);
            } else {
                // (the reference puts the display's centre at the top-left)
                opening.report = false;
                opening.desired = around.display.map(Rect::center)?;
            }
        }
        "mouse" => {
            // (a desktop without a readable pointer opens it over the
            // parent, where the click that opened it was; with neither, at
            // the padding, as the reference with no mouse screen)
            if let Some((x, y)) = around
                .mouse
                .or_else(|| around.parent.map(|parent| parent.frame.center()))
            {
                let (mx, my) = me.center();
                opening.desired = (x - mx, y - my);
                opening.report = false;
            }
        }
        _ => {}
    }
    Some(opening)
}

/// Where a window of `size` opens once the screens are known
/// (`GetSafePosition` on its wanted place, then `SlideOffScreenTLWUpAndLeft`
/// for a default top-left place, on the display it then is on), and the
/// message the user is shown if it was rescued from off the screens. None
/// where no screen can take it.
pub fn fitted(
    opening: Opening,
    size: (i32, i32),
    settings: &hydrus_store::settings::WindowRescueSettings,
    screens: &[crate::window_rescue::Screen],
    frame_key: &str,
) -> Option<((i32, i32), Option<String>)> {
    let desired = (i64::from(opening.desired.0), i64::from(opening.desired.1));
    let safe = crate::window_rescue::safe_position(
        desired,
        Some((i64::from(size.0), i64::from(size.1))),
        settings,
        screens,
    )?;
    let mut position = (i32::try_from(safe.0).ok()?, i32::try_from(safe.1).ok()?);
    let message = (opening.report && safe != desired)
        .then(|| rescue_message(frame_key, opening.desired, position));
    if opening.slide {
        // (the display the window is on: the one its top-left is on)
        let display = screens
            .iter()
            .find(|screen| screen.geometry.contains(safe))
            .or(screens.first())
            .and_then(|screen| {
                Some(Rect {
                    x: i32::try_from(screen.geometry.x).ok()?,
                    y: i32::try_from(screen.geometry.y).ok()?,
                    width: i32::try_from(screen.geometry.width).ok()?,
                    height: i32::try_from(screen.geometry.height).ok()?,
                })
            });
        if let Some(display) = display {
            position = slide_up_and_left(position, size, display);
        }
    }
    Some((position, message))
}

/// `GetSafePosition`'s message, its points written as pairs rather than as
/// Qt's `QPoint` reprs.
pub fn rescue_message(frame_key: &str, wanted: (i32, i32), rescued: (i32, i32)) -> String {
    format!(
        "A window with frame key \"{frame_key}\" that wanted to display at \"({}, {})\" was rescued from apparent off-screen to the new location at \"({}, {})\".",
        wanted.0, wanted.1, rescued.0, rescued.1
    )
}
