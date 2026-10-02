//! The ratings the reference draws over a thumbnail's top right
//! (`_PaintThumbnailContent`): those of its rating services shown in
//! thumbnails, in rows from the top, each row on a box of the window's
//! colour (if the options say so) against the right border. The
//! like/dislike ratings share a row, each numerical rating has its own,
//! and the inc/dec ratings share the last. The icons at the top right go
//! under them. Plain Rust, tested against the reference's recording.

use hydrus_core::thumbnail::ThumbnailRatingSettings;
use hydrus_store::services::{PenBrush, ServiceKind, ServiceRegistry};

use crate::ratings::{Control, Kind};

/// The pixels between two shapes (`ClientGUIPainterShapes.PAD_PX`).
const PAD: i32 = 4;
/// Between a box's edge and what is in it.
const MARGIN: i32 = 1;

/// `show_fraction_beside_stars`'s "on the left" and "on the right".
const FRACTION_LEFT: u8 = 1;
const FRACTION_RIGHT: u8 = 2;

/// A rating drawn at (`x`, `y`) (where the reference's `DrawLike`,
/// `DrawNumerical` or `DrawIncDec` is told to draw it).
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub control: Control,
    pub x: i32,
    pub y: i32,
    pub look: Look,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Look {
    /// Shapes `size` square, `step` apart from `first` to the right, in
    /// these colours left to right; a numerical rating's "stars/of" `text`
    /// before or after them.
    Shapes {
        path: &'static str,
        first: i32,
        size: i32,
        step: i32,
        shapes: Vec<PenBrush>,
        text: Option<Text>,
    },
    /// An inc/dec rating: a box this big, its number in `text`, right
    /// aligned and centred in a box a pixel smaller each way.
    Counter {
        width: i32,
        height: i32,
        colours: PenBrush,
        text: Text,
    },
}

/// Text drawn from (`x`, `y`), in a box `width` wide, at a pixel size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub pixel_size: i32,
}

/// A box drawn behind a row of ratings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backing {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// What is drawn over a thumbnail, and where the icons at the top right go
/// under it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub drawn: Vec<Drawn>,
    pub boxes: Vec<Backing>,
    pub top_right_y: i32,
}

/// Whether a file's `control` is drawn over its thumbnail
/// (`ShouldShowRatingInThumbnail`): its service shows in thumbnails, and
/// either even when unrated or it is rated (an inc/dec rating of 0 isn't).
pub fn shown(services: &ServiceRegistry, control: &Control) -> bool {
    let Ok(service) = services.get(control.service) else {
        return false;
    };
    let display = match &service.kind {
        ServiceKind::RatingLike(config) => &config.display,
        ServiceKind::RatingNumerical(config) => &config.display,
        ServiceKind::RatingIncDec(display) => display,
        _ => return false,
    };
    if !display.show_in_thumbnail {
        return false;
    }
    display.show_in_thumbnail_even_when_null
        || match &control.kind {
            Kind::Like { state, .. } => state.is_some(),
            Kind::Numerical { stars, .. } => stars.is_some(),
            Kind::IncDec { value } => *value != 0,
        }
}

/// The ratings `controls` (a file's, as [`crate::ratings::controls`] has
/// them) drawn over its thumbnail `width` wide with a border `border`
/// wide, as `settings` say. `text_width` is how wide some text is at a
/// pixel size.
pub fn layout(
    services: &ServiceRegistry,
    controls: &[Control],
    settings: &ThumbnailRatingSettings,
    width: i32,
    border: i32,
    text_width: &dyn Fn(&str, i32) -> i32,
) -> Layout {
    let size = settings.icon_size.round_ties_even() as i32;
    let counter_height = settings.incdec_height.round_ties_even() as i32;
    let mut out = Layout {
        top_right_y: border,
        ..Layout::default()
    };
    let shown: Vec<&Control> = controls.iter().filter(|c| shown(services, c)).collect();
    let add_box = |out: &mut Layout, rect_width: i32, rect_height: i32| -> (i32, i32) {
        let x = width - border - rect_width;
        let y = out.top_right_y;
        if settings.background {
            out.boxes.push(Backing {
                x,
                y,
                width: rect_width,
                height: rect_height,
            });
        }
        out.top_right_y += rect_height;
        (x, y)
    };
    // the likes, one row
    let likes: Vec<&Control> = shown
        .iter()
        .copied()
        .filter(|c| matches!(c.kind, Kind::Like { .. }))
        .collect();
    if !likes.is_empty() {
        let n = likes.len() as i32;
        let (x, y) = add_box(
            &mut out,
            size * n + PAD * (n - 1) + 2 * MARGIN,
            size + PAD + 2 * MARGIN,
        );
        for (i, control) in likes.into_iter().enumerate() {
            let Kind::Like { shape, .. } = &control.kind else {
                continue;
            };
            out.drawn.push(Drawn {
                control: control.clone(),
                x: x + PAD / 2 + i as i32 * (size + PAD),
                y: y + PAD / 2,
                look: Look::Shapes {
                    path: shape,
                    first: 0,
                    size,
                    step: size + PAD,
                    shapes: control.shapes(),
                    text: None,
                },
            });
        }
    }
    // each numerical rating, a row each
    for control in shown.iter().copied() {
        let Kind::Numerical {
            stars,
            config,
            shape,
        } = &control.kind
        else {
            continue;
        };
        let collapsed = settings.numerical_collapsed;
        let beside = config.show_fraction_beside_stars;
        let pad = config.custom_pad;
        let pixel_size = size - 1;
        let text = (collapsed || beside != 0).then(|| fraction_text(*stars, config.num_stars));
        let text_size = text.as_deref().map_or(0, |t| text_width(t, pixel_size));
        let n = if collapsed {
            1
        } else {
            config.num_stars as i32
        };
        let numerical_width = text_size + size * n + pad * (n - 1) + PAD;
        let (x, y) = add_box(
            &mut out,
            numerical_width + 2 * MARGIN,
            size + PAD + 2 * MARGIN,
        );
        let (x, y) = (x + PAD / 2, y + PAD / 2);
        let text_at = |at: i32| Text {
            text: text.clone().unwrap_or_default(),
            x: at,
            y: y - PAD,
            width: text_size,
            pixel_size,
        };
        let (shapes, start, text) = if collapsed {
            let colours = control.colours;
            let one = if stars.is_some() {
                colours.like
            } else {
                colours.null
            };
            (vec![one], text_size + 1, Some(text_at(x - PAD / 2)))
        } else if beside == FRACTION_LEFT {
            (control.shapes(), text_size + 1, Some(text_at(x - PAD / 2)))
        } else if beside == FRACTION_RIGHT {
            let after = n * (size + pad) - pad + PAD / 2;
            (control.shapes(), 0, Some(text_at(x + after)))
        } else {
            (control.shapes(), 0, None)
        };
        out.drawn.push(Drawn {
            control: control.clone(),
            x,
            y,
            look: Look::Shapes {
                path: shape,
                first: start,
                size,
                step: size + pad,
                shapes,
                text,
            },
        });
    }
    // the inc/decs, one row
    let counters: Vec<(&Control, i64)> = shown
        .iter()
        .filter_map(|c| match c.kind {
            Kind::IncDec { value } => Some((*c, value)),
            _ => None,
        })
        .collect();
    if !counters.is_empty() {
        let n = counters.len() as i32;
        let widths: Vec<i32> = counters
            .iter()
            .map(|(_, value)| counter_width(counter_height, *value))
            .collect();
        let rect_width = 2 * MARGIN + MARGIN * (n - 1) + widths.iter().sum::<i32>();
        let (x, y) = add_box(&mut out, rect_width, counter_height + 2 * MARGIN);
        let mut x = x + MARGIN;
        let y = y + MARGIN;
        // (a small number is drawn a pixel lower)
        let (pixel_size, text_y) = if counter_height > 8 {
            (counter_height - 1, y)
        } else {
            (counter_height, y + 1)
        };
        for ((control, value), w) in counters.into_iter().zip(widths) {
            out.drawn.push(Drawn {
                control: control.clone(),
                x,
                y,
                look: Look::Counter {
                    width: w,
                    height: counter_height,
                    colours: control.colours.like,
                    text: Text {
                        text: hydrus_core::numbers::human_int(value.unsigned_abs()),
                        x: x - 1,
                        y: text_y,
                        width: w - 1,
                        pixel_size,
                    },
                },
            });
            x += w + MARGIN;
        }
    }
    out
}

/// A numerical rating's "stars/of" (`GetNumericalFractionText`): the stars
/// set, or "-" for none, as wide as the number of stars.
pub fn fraction_text(stars: Option<u32>, num_stars: u32) -> String {
    let of = num_stars.to_string();
    let set = stars.map_or_else(|| "-".to_owned(), |s| s.to_string());
    format!("{set:>width$}/{of}", width = of.len())
}

/// How wide an inc/dec rating `height` tall is (`GetIncDecSize`): twice
/// its height, and wider for numbers of over three digits.
pub fn counter_width(height: i32, value: i64) -> i32 {
    let mut width = f64::from(height * 2);
    if value > 0 {
        let digits = value.to_string().len() as f64;
        if digits > 3.0 {
            width += f64::from(height - 1) * (digits - (2.0 + digits / 3.0));
        }
    }
    width as i32
}
