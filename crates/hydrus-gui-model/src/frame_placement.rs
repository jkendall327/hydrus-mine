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
    /// `QRect::center` (the rectangle's last pixel is `x + width - 1`).
    pub fn center(self) -> (i32, i32) {
        (
            self.x + (self.width - 1) / 2,
            self.y + (self.height - 1) / 2,
        )
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
/// window's own `OpeningRescue` does that).
pub fn initial(frame: &FrameLocation, hint: (i32, i32), around: &Surroundings) -> Placement {
    let size = match frame.last_size.filter(|_| frame.remember_size) {
        Some(size) => size,
        None => safe_size(hint, frame.default_gravity, around),
    };
    let padding = (CHILD_POSITION_PADDING, CHILD_POSITION_PADDING);
    let mut position = Some(padding);
    let mut slide = false;
    if let Some(last) = frame.last_position.filter(|_| frame.remember_position) {
        position = Some(last);
    } else {
        let me = Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        };
        match frame.default_position.as_str() {
            "topleft" => {
                position = match around.parent {
                    Some(parent) => Some((
                        parent.frame.x + CHILD_POSITION_PADDING,
                        parent.frame.y + CHILD_POSITION_PADDING,
                    )),
                    None => around
                        .display
                        .map(|d| (d.x + CHILD_POSITION_PADDING, d.y + CHILD_POSITION_PADDING)),
                };
                slide = true;
            }
            "center" => {
                position = match around.parent {
                    Some(parent) => {
                        let (cx, cy) = parent.frame.center();
                        let (mx, my) = me.center();
                        Some((cx - mx, cy - my))
                    }
                    // (the reference puts the display's centre at the top-left)
                    None => around.display.map(Rect::center),
                };
            }
            "mouse" => {
                // (a desktop without a readable pointer opens it over the
                // parent, where the click that opened it was)
                let at = around
                    .mouse
                    .or_else(|| around.parent.map(|parent| parent.frame.center()));
                if let Some((x, y)) = at {
                    let (mx, my) = me.center();
                    position = Some((x - mx, y - my));
                }
            }
            _ => {}
        }
    }
    if slide && let (Some(at), Some(display)) = (position, around.display) {
        position = Some(slide_up_and_left(at, size, display));
    }
    Placement { size, position }
}
