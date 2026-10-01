//! A file's ratings in the media viewer, as the reference's top-right hover
//! frame has them: each local like/dislike service, then each numerical
//! one, then each inc/dec one, drawn in its service's shape and colours,
//! and set by clicking as the reference's controls are. Plain Rust, tested
//! directly.

use hydrus_core::{HashId, ServiceId, ServiceType};
use hydrus_store::Store;
use hydrus_store::media::Rating;
use hydrus_store::services::{
    NumericalRatingConfig, PenBrush, RatingColours, ServiceKind, StarAppearance,
};

/// A rating service's control for one file.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    pub service: ServiceId,
    pub name: String,
    pub kind: Kind,
    pub colours: RatingColours,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Liked (`Some(true)`), disliked, or not rated; drawn in this shape.
    Like {
        state: Option<bool>,
        shape: &'static str,
    },
    /// The stars set, if rated, of the service's.
    Numerical {
        stars: Option<u32>,
        config: NumericalRatingConfig,
        shape: &'static str,
    },
    IncDec {
        value: i64,
    },
}

impl Control {
    /// How each shape is drawn, left to right: like/dislike's one, a
    /// numerical rating's stars (those set in the like colours, the rest
    /// in the dislike colours, or all in the null colours if unrated).
    pub fn shapes(&self) -> Vec<PenBrush> {
        let c = &self.colours;
        match &self.kind {
            Kind::Like { state, .. } => vec![match state {
                Some(true) => c.like,
                Some(false) => c.dislike,
                None => c.null,
            }],
            Kind::Numerical { stars, config, .. } => {
                let n = config.num_stars as usize;
                match stars {
                    Some(on) => (0..n)
                        .map(|i| if i < *on as usize { c.like } else { c.dislike })
                        .collect(),
                    None => vec![c.null; n],
                }
            }
            Kind::IncDec { .. } => Vec::new(),
        }
    }
}

/// A shape's outline, as SVG path commands in a 12x12 box
/// (`ClientGUIPainterShapes`); a named SVG is drawn as the fat star, as the
/// reference draws one it doesn't have.
pub fn shape_path(appearance: &StarAppearance) -> &'static str {
    let StarAppearance::Shape(shape) = appearance else {
        return FAT_STAR;
    };
    match shape.0 {
        0 => "M 0 6 A 6 6 0 1 1 12 6 A 6 6 0 1 1 0 6 Z",
        1 => "M 0 0 L 12 0 L 12 12 L 0 12 Z",
        3 => {
            "M 6 0 L 7.5 4.5 L 12 4.5 L 8.3 7.2 L 9.8 12 L 6 9 L 2.2 12 L 3.7 7.2 L 0 4.5 L 4.5 4.5 Z"
        }
        4 => "M 6 0 L 8 3 L 11 3 L 9 6 L 11 9 L 8 9 L 6 12 L 4 9 L 1 9 L 3 6 L 1 3 L 4 3 Z",
        5 => {
            "M 6 0.5 L 6.9 3.9 L 10 2 L 8 5.1 L 11.5 6 L 8 6.9 L 10 10 L 7 8.1 L 6 11.5 L 5 8.1 L 2 10 L 4 6.9 L 0.5 6 L 4 5.1 L 2 2 L 5 3.9 Z"
        }
        6 => "M 2 0 L 6 4 L 10 0 L 12 2 L 8 6 L 12 10 L 10 12 L 6 8 L 2 12 L 0 10 L 4 6 L 0 2 Z",
        7 => {
            "M 4.5 0 L 7.5 0 L 7.5 4.5 L 12 4.5 L 12 7.5 L 7.5 7.5 L 7.5 12 L 4.5 12 L 4.5 7.5 L 0 7.5 L 0 4.5 L 4.5 4.5 Z"
        }
        30 => "M 6 0 L 12 12 L 0 12 Z",
        31 => "M 0 0 L 12 0 L 6 12 Z",
        32 => "M 0 0 L 12 6 L 0 12 Z",
        33 => "M 12 0 L 0 6 L 12 12 Z",
        40 => "M 6 0 L 12 6 L 6 12 L 0 6 Z",
        42 => "M 12 0 L 1 4 L 0 12 L 11 8 Z",
        43 => "M 0 0 L 12 5 L 12 12 L 0 7 Z",
        44 => "M 2 0 L 10 0 L 2 12 L 10 12 Z",
        50 => "M 6 0 L 12 5.5 L 9 12 L 3 12 L 0 5.5 Z",
        60 => "M 6 0 L 10.3 2.6 L 10.3 7.8 L 6 10.4 L 1.7 7.8 L 1.7 2.6 Z",
        61 => "M 6 2.8 L 8.7 4.9 L 8.7 7.3 L 6 9.4 L 3.3 7.3 L 3.3 4.9 Z",
        101 => "M 6 11 L 1.5 5 C 0 3 3.5 -1 6 3 C 8.5 -1 12 3 10.5 5 L 6 11 Z",
        102 => {
            "M 6 0 C 6.8 5.2 7.5 5.5 9 7.3 C 11 10 9 12 6 12 C 3 12 1 10 3 7.3 C 4.5 5.5 5.2 5.2 6 0 Z"
        }
        103 => {
            "M 8 1.1 C 0.25 2 0.25 11 8 11.5 C 10.25 11.5 11.75 10.8 11.75 10.5 C 6 9 6 3 11.75 3 C 11.75 1.8 10.25 1.2 8 1.1 Z"
        }
        _ => FAT_STAR,
    }
}

const FAT_STAR: &str =
    "M 6 0 L 7.8 4.1 L 12 4.6 L 8.9 7.6 L 9.8 12 L 6 9.8 L 2.2 12 L 3.1 7.6 L 0 4.5 L 4.2 4.1 Z";

/// The width of a shape's outline at `size` (`GetOutlinePx`).
pub fn outline_width(size: f64) -> f64 {
    (size / 12.0).clamp(1.0, 4.0)
}

/// `file`'s rating controls: like/dislike services, then numerical, then
/// inc/dec, each in the store's order.
pub fn controls(store: &Store, file: HashId) -> Vec<Control> {
    let snapshot = store.snapshot();
    let ratings = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .ok()
        .and_then(|batch| batch.results.into_iter().next())
        .map(|m| m.ratings)
        .unwrap_or_default();
    let mut out = Vec::new();
    for service_type in [
        ServiceType::LocalRatingLike,
        ServiceType::LocalRatingNumerical,
        ServiceType::LocalRatingIncDec,
    ] {
        for service in snapshot.services.of_type(service_type) {
            let rating = ratings.get(&service.id).copied();
            let fraction = match rating {
                Some(Rating::Fraction(f)) => Some(f),
                _ => None,
            };
            let (kind, colours) = match &service.kind {
                ServiceKind::RatingLike(config) => (
                    Kind::Like {
                        state: fraction.map(|f| f > 0.5),
                        shape: shape_path(&config.appearance),
                    },
                    config.display.colours,
                ),
                ServiceKind::RatingNumerical(config) => (
                    Kind::Numerical {
                        stars: fraction.map(|f| config.stars(f)),
                        config: config.clone(),
                        shape: shape_path(&config.appearance),
                    },
                    config.display.colours,
                ),
                ServiceKind::RatingIncDec(display) => (
                    Kind::IncDec {
                        value: match rating {
                            Some(Rating::IncDec(v)) => v,
                            _ => 0,
                        },
                    },
                    display.colours,
                ),
                _ => continue,
            };
            out.push(Control {
                service: service.id,
                name: service.name.clone(),
                kind,
                colours,
            });
        }
    }
    out
}

/// The stars a click `proportion` of the way along a numerical rating's
/// stars sets (`_GetRatingStateAndRatingFromClickEvent`).
pub fn stars_at(config: &NumericalRatingConfig, proportion: f64) -> u32 {
    let n = f64::from(config.num_stars);
    let stars = if config.allow_zero {
        // (Python's round, halves to even)
        (proportion * n).round_ties_even()
    } else {
        (proportion * n).trunc() + if proportion <= 1.0 { 1.0 } else { 0.0 }
    };
    (stars.max(0.0) as u32).clamp(config.min_stars(), config.num_stars)
}

/// A left click on a control, `proportion` of the way along it: like
/// (or, liked, unrated); the stars there; one more.
pub fn left_click(
    store: &Store,
    file: HashId,
    control: &Control,
    proportion: f64,
) -> hydrus_store::Result<()> {
    match &control.kind {
        Kind::Like { state, .. } => {
            let rating = (*state != Some(true)).then_some(1.0);
            set(store, control.service, file, rating)
        }
        Kind::Numerical { config, .. } => {
            let rating = config.rating(stars_at(config, proportion));
            set(store, control.service, file, Some(rating))
        }
        Kind::IncDec { value } => set_incdec(store, control.service, file, value + 1),
    }
}

/// A right click: dislike (or, disliked, unrated); unrated; one less (not
/// below nothing).
pub fn right_click(store: &Store, file: HashId, control: &Control) -> hydrus_store::Result<()> {
    match &control.kind {
        Kind::Like { state, .. } => {
            let rating = (*state != Some(false)).then_some(0.0);
            set(store, control.service, file, rating)
        }
        Kind::Numerical { .. } => set(store, control.service, file, None),
        Kind::IncDec { value } if *value > 0 => set_incdec(store, control.service, file, value - 1),
        Kind::IncDec { .. } => Ok(()),
    }
}

fn set(
    store: &Store,
    service: ServiceId,
    file: HashId,
    rating: Option<f64>,
) -> hydrus_store::Result<()> {
    store.write_content(move |w| w.set_rating(service, &[file], rating))
}

fn set_incdec(
    store: &Store,
    service: ServiceId,
    file: HashId,
    value: i64,
) -> hydrus_store::Result<()> {
    store.write_content(move |w| w.set_incdec(service, &[file], value))
}
