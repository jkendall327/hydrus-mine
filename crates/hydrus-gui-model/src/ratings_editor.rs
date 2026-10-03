//! The "manage ratings" dialog (the reference's `DialogManageRatings`):
//! each local rating service's control over some files, set in the dialog
//! (a service the files differ on starts "mixed"), copied and pasted as the
//! reference's JSON, and applied as the ratings changed. Ratings compare
//! exactly, as the reference's do.
#![allow(clippy::float_cmp)]

use std::collections::HashMap;

use hydrus_core::ServiceId;
use hydrus_core::pyjson::PyJson;
use hydrus_store::media::Rating;
use hydrus_store::services::{NumericalRatingConfig, ServiceKind, ServiceRegistry};

use crate::ratings::{Control, Kind, stars_at};

/// The dialog's title for `count` files.
pub fn title(count: usize) -> String {
    format!(
        "manage ratings for {} files",
        hydrus_core::numbers::human_int(count as u64)
    )
}

/// A like/dislike control's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Like {
    Like,
    Dislike,
    Null,
    Mixed,
}

/// A numerical control's state: unrated, a rating (a fraction), or mixed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Numerical {
    Null,
    Set(f64),
    Mixed,
}

/// A control's state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Like(Like),
    Numerical(Numerical),
    /// The count, and whether the files differ (the count their average,
    /// rounded down).
    IncDec {
        value: i64,
        mixed: bool,
    },
}

/// One service's control.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub service: ServiceId,
    /// The service's key, as hex (what copy and paste name it by).
    pub key: String,
    pub name: String,
    pub numerical: Option<NumericalRatingConfig>,
    pub original: State,
    pub now: State,
    /// How the viewer draws the service's rating (its shape, colours).
    control: Control,
}

/// A rating to write: a like's or numerical's (`None` unrated), or a count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Update {
    Rating(Option<f64>),
    IncDec(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RatingsEditor {
    pub rows: Vec<Row>,
}

fn like_of(ratings: &[Option<Rating>]) -> Like {
    let mut on = false;
    let mut off = false;
    let mut null = false;
    for rating in ratings {
        match rating {
            Some(Rating::Fraction(f)) if *f == 1.0 => on = true,
            Some(Rating::Fraction(f)) if *f == 0.0 => off = true,
            None => null = true,
            _ => {}
        }
    }
    match (on, off, null) {
        (true, false, false) => Like::Like,
        (false, true, false) => Like::Dislike,
        (false, false, true) => Like::Null,
        _ => Like::Mixed,
    }
}

fn numerical_of(ratings: &[Option<Rating>]) -> Numerical {
    let mut existing: Option<f64> = None;
    let mut null = false;
    for rating in ratings {
        if let Some(Rating::Fraction(f)) = rating {
            if null || existing.is_some_and(|e| e != *f) {
                return Numerical::Mixed;
            }
            existing = Some(*f);
        } else {
            if existing.is_some() {
                return Numerical::Mixed;
            }
            null = true;
        }
    }
    existing.map_or(Numerical::Null, Numerical::Set)
}

fn incdec_of(ratings: &[Option<Rating>]) -> State {
    let values: Vec<i64> = ratings
        .iter()
        .map(|r| match r {
            Some(Rating::IncDec(v)) => *v,
            _ => 0,
        })
        .collect();
    let first = values.first().copied().unwrap_or(0);
    if values.iter().all(|&v| v == first) {
        State::IncDec {
            value: first,
            mixed: false,
        }
    } else {
        let sum: i64 = values.iter().sum();
        State::IncDec {
            value: sum.div_euclid(i64::try_from(values.len()).unwrap_or(1)),
            mixed: true,
        }
    }
}

/// Python's `repr` of a float, as `json.dumps` writes it.
fn py_float(f: f64) -> String {
    let mut out = String::new();
    hydrus_core::pyjson::write_python_float(f, &mut out);
    out
}

/// What the reference says of clipboard text it couldn't read as ratings.
pub fn clipboard_error(content: &str, error: &str) -> String {
    crate::notes_editor::clipboard_parse_error(
        "JSON pairs of service keys and rating values",
        content,
        error,
    )
}

/// `bytes.fromhex`, with Python's error.
fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    let error = |at: usize| {
        format!("ValueError('non-hexadecimal number found in fromhex() arg at position {at}')")
    };
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            i += 1;
            continue;
        }
        let high = chars[i].to_digit(16).ok_or_else(|| error(i))?;
        let low = chars
            .get(i + 1)
            .and_then(|c| c.to_digit(16))
            .ok_or_else(|| error(i + 1))?;
        out.push(u8::try_from(high * 16 + low).unwrap_or(0));
        i += 2;
    }
    Ok(out)
}

/// The pasted pairs: each service key's hex and its rating.
fn pairs(text: &str) -> Result<Vec<(String, PyJson)>, String> {
    let value = PyJson::parse(text).map_err(|e| format!("JSONDecodeError('{}')", e.message))?;
    let Some(items) = value.as_list() else {
        return Err(format!(
            "TypeError(\"'{}' object is not iterable\")",
            value.kind()
        ));
    };
    let mut out = Vec::new();
    for item in items {
        let pair = match item {
            PyJson::List(pair) => pair,
            other => {
                return Err(format!(
                    "TypeError(\"cannot unpack non-iterable {} object\")",
                    other.kind()
                ));
            }
        };
        match pair.as_slice() {
            [key, rating] => {
                let Some(key) = key.as_str() else {
                    return Err("TypeError('fromhex() argument must be str, not int')".into());
                };
                from_hex(key)?;
                out.push((key.to_owned(), rating.clone()));
            }
            short if short.len() < 2 => {
                return Err(format!(
                    "ValueError('not enough values to unpack (expected 2, got {})')",
                    short.len()
                ));
            }
            _ => return Err("ValueError('too many values to unpack (expected 2)')".into()),
        }
    }
    Ok(out)
}

impl RatingsEditor {
    /// The controls over files rated as `files` say: like/dislike
    /// services, then numerical, then inc/dec, each in the store's order.
    pub fn new(services: &ServiceRegistry, files: &[HashMap<ServiceId, Rating>]) -> Self {
        let controls = crate::ratings::controls_of(services, &HashMap::new());
        let rows = controls
            .into_iter()
            .filter_map(|control| {
                let service = services.get(control.service).ok()?;
                let ratings: Vec<Option<Rating>> =
                    files.iter().map(|f| f.get(&service.id).copied()).collect();
                let (state, numerical) = match &service.kind {
                    ServiceKind::RatingLike(_) => (State::Like(like_of(&ratings)), None),
                    ServiceKind::RatingNumerical(config) => (
                        State::Numerical(numerical_of(&ratings)),
                        Some(config.clone()),
                    ),
                    _ => (incdec_of(&ratings), None),
                };
                Some(Row {
                    service: service.id,
                    key: service.key.to_hex(),
                    name: service.name.clone(),
                    numerical,
                    original: state,
                    now: state,
                    control,
                })
            })
            .collect();
        Self { rows }
    }

    /// A left click on a control, `proportion` of the way along it: like
    /// (or, liked, unrated); the stars there; one more.
    pub fn left(&mut self, row: usize, proportion: f64) {
        let Some(row) = self.rows.get_mut(row) else {
            return;
        };
        row.now = match row.now {
            State::Like(Like::Like) => State::Like(Like::Null),
            State::Like(_) => State::Like(Like::Like),
            State::Numerical(_) => match &row.numerical {
                Some(config) => {
                    State::Numerical(Numerical::Set(config.rating(stars_at(config, proportion))))
                }
                None => row.now,
            },
            State::IncDec { value, .. } => State::IncDec {
                value: value + 1,
                mixed: false,
            },
        };
    }

    /// A right click: dislike (or, disliked, unrated); unrated; one less
    /// (not below nothing).
    pub fn right(&mut self, row: usize) {
        let Some(row) = self.rows.get_mut(row) else {
            return;
        };
        row.now = match row.now {
            State::Like(Like::Dislike) => State::Like(Like::Null),
            State::Like(_) => State::Like(Like::Dislike),
            State::Numerical(_) => State::Numerical(Numerical::Null),
            State::IncDec { value, .. } if value > 0 => State::IncDec {
                value: value - 1,
                mixed: false,
            },
            other @ State::IncDec { .. } => other,
        };
    }

    /// A count typed for an inc/dec control (its middle click).
    pub fn set_count(&mut self, row: usize, value: i64) {
        if let Some(row) = self.rows.get_mut(row)
            && matches!(row.now, State::IncDec { .. })
        {
            row.now = State::IncDec {
                value: value.max(0),
                mixed: false,
            };
        }
    }

    /// Copy: each control not mixed, as the reference's JSON pairs, and
    /// the notice ("Copied 3 ratings!").
    pub fn copy(&self) -> (String, String) {
        let mut items = Vec::new();
        for row in &self.rows {
            let rating = match row.now {
                State::Like(Like::Like) => "1".to_owned(),
                State::Like(Like::Dislike) => "0".to_owned(),
                State::Like(Like::Null) | State::Numerical(Numerical::Null) => "null".to_owned(),
                State::Numerical(Numerical::Set(f)) => py_float(f),
                State::IncDec {
                    value,
                    mixed: false,
                } => value.to_string(),
                State::Like(Like::Mixed)
                | State::Numerical(Numerical::Mixed)
                | State::IncDec { mixed: true, .. } => continue,
            };
            let mut key = String::new();
            hydrus_core::pyjson::write_python_string(&row.key, &mut key);
            items.push(format!("[{key}, {rating}]"));
        }
        let notice = format!(
            "Copied {} ratings!",
            hydrus_core::numbers::human_int(items.len() as u64)
        );
        (format!("[{}]", items.join(", ")), notice)
    }

    /// Paste: the pairs read set the controls they name, as each can take
    /// them; the notice ("Pasted 2 ratings!"), or the error.
    pub fn paste(&mut self, text: &str) -> Result<String, String> {
        let pairs = pairs(text).map_err(|e| clipboard_error(text, &e))?;
        for (key, rating) in &pairs {
            let Some(row) = self.rows.iter_mut().find(|r| r.key == *key) else {
                continue;
            };
            let number = match rating {
                PyJson::Int(i) => Some(*i as f64),
                PyJson::Float(f) => Some(*f),
                PyJson::Bool(b) => Some(f64::from(u8::from(*b))),
                _ => None,
            };
            row.now = match row.now {
                State::Like(_) => match (rating, number) {
                    (PyJson::Null, _) => State::Like(Like::Null),
                    (_, Some(1.0)) => State::Like(Like::Like),
                    (_, Some(0.0)) => State::Like(Like::Dislike),
                    _ => continue,
                },
                State::Numerical(_) => match (rating, number) {
                    (PyJson::Null, _) => State::Numerical(Numerical::Null),
                    (_, Some(n)) if (0.0..=1.0).contains(&n) => State::Numerical(Numerical::Set(n)),
                    _ => continue,
                },
                State::IncDec { .. } => match rating {
                    PyJson::Int(i) => State::IncDec {
                        value: *i,
                        mixed: false,
                    },
                    PyJson::Bool(b) => State::IncDec {
                        value: i64::from(*b),
                        mixed: false,
                    },
                    _ => continue,
                },
            };
        }
        Ok(format!(
            "Pasted {} ratings!",
            hydrus_core::numbers::human_int(pairs.len() as u64)
        ))
    }

    /// What "apply" writes, in the controls' order: each control not
    /// mixed whose rating is not what it was (a like's state, a
    /// numerical's or inc/dec's rating: so a mixed inc/dec set back to its
    /// average writes nothing, as the reference's doesn't).
    pub fn updates(&self) -> Vec<(ServiceId, Update)> {
        let mut out = Vec::new();
        for row in &self.rows {
            let update = match (row.original, row.now) {
                (
                    _,
                    State::Like(Like::Mixed)
                    | State::Numerical(Numerical::Mixed)
                    | State::IncDec { mixed: true, .. },
                ) => None,
                (State::Like(was), State::Like(now)) => {
                    (was != now).then_some(Update::Rating(match now {
                        Like::Like => Some(1.0),
                        Like::Dislike => Some(0.0),
                        _ => None,
                    }))
                }
                (State::Numerical(was), State::Numerical(now)) => {
                    let rating = |s: Numerical| match s {
                        Numerical::Set(f) => Some(f),
                        _ => None,
                    };
                    (rating(was) != rating(now)).then_some(Update::Rating(rating(now)))
                }
                (State::IncDec { value: was, .. }, State::IncDec { value, .. }) => {
                    (was != value).then_some(Update::IncDec(value))
                }
                _ => None,
            };
            out.extend(update.map(|u| (row.service, u)));
        }
        out
    }
}

impl Row {
    /// The control as the viewer draws it, and whether it is mixed (drawn
    /// in the service's mixed colours).
    pub fn display(&self) -> (Control, bool) {
        let mut control = self.control.clone();
        let mixed = match (&mut control.kind, self.now) {
            (Kind::Like { state, .. }, State::Like(like)) => {
                *state = match like {
                    Like::Like => Some(true),
                    Like::Dislike => Some(false),
                    _ => None,
                };
                like == Like::Mixed
            }
            (Kind::Numerical { stars, config, .. }, State::Numerical(n)) => {
                *stars = match n {
                    Numerical::Set(f) => Some(config.stars(f)),
                    _ => None,
                };
                n == Numerical::Mixed
            }
            (Kind::IncDec { value: shown }, State::IncDec { value, mixed }) => {
                *shown = value;
                mixed
            }
            _ => false,
        };
        (control, mixed)
    }
}
