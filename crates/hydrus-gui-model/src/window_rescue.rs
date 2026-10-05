//! GetSafePosition decisions over explicit monitor geometry, without GUI state.
use hydrus_store::settings::WindowRescueSettings;

pub type Point = (i64, i64);

/// Logical monitor rectangle; bounds are half open, matching QPoint/QRect hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}
impl Rect {
    pub fn contains(self, (x, y): Point) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
    fn intersection_top_left(self, other: Self) -> Option<Point> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        (x < (self.x + self.width).min(other.x + other.width)
            && y < (self.y + self.height).min(other.y + other.height))
        .then_some((x, y))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Screen {
    pub geometry: Rect,
    /// Qt uses the first display's available top left as its last resort.
    pub available_top_left: Point,
}

/// Preserve a leniently visible top left, then try the three remaining corners
/// in Qt order, then use the first display's available origin. Padding used to
/// test visibility is independent from the setting that pads rescued positions.
pub fn safe_position(
    position: Point,
    size: Option<(i64, i64)>,
    settings: &WindowRescueSettings,
    screens: &[Screen],
) -> Option<Point> {
    let padding = i64::from(settings.padding);
    let screen_at = |point| {
        screens
            .iter()
            .position(|screen| screen.geometry.contains(point))
    };
    if settings.disabled || screen_at((position.0 + padding, position.1 + padding)).is_some() {
        return Some(position);
    }
    if let Some((width, height)) = size {
        let original = Rect {
            x: position.0,
            y: position.1,
            width,
            height,
        };
        for (test, fuzz) in [
            ((position.0, position.1 + height), (0, padding)),
            ((position.0 + width, position.1), (padding, 0)),
            (
                (position.0 + width, position.1 + height),
                (padding, padding),
            ),
        ] {
            let Some(screen_index) = screen_at(test) else {
                continue;
            };
            let Some(mut rescue) = screens[screen_index]
                .geometry
                .intersection_top_left(original)
            else {
                continue;
            };
            if settings.add_padding {
                rescue.0 += fuzz.0;
                rescue.1 += fuzz.1;
            }
            if screen_at(rescue) == Some(screen_index) {
                return Some(rescue);
            }
        }
    }
    screens.first().map(|screen| {
        let (x, y) = screen.available_top_left;
        if settings.add_padding {
            (x + padding, y + padding)
        } else {
            (x, y)
        }
    })
}
