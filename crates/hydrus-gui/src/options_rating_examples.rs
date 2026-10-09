//! The ratings page's examples (the reference's `RatingNumericalExample` and
//! `RatingIncDecExample` beside each size): a row of stars styled after the
//! chosen service, and an inc/dec rectangle, each drawn at the sizes typed in
//! its box and clickable to test. The samples are never stored.

use hydrus_core::ServiceKey;
use hydrus_gui_model::options::{Editor, Kind, Row, Value};
use hydrus_gui_model::rating_example::{self, Example};
use hydrus_gui_model::ratings;
use hydrus_store::Store;
use hydrus_store::services::{NumericalRatingConfig, RatingDisplay, ServiceKind};
use std::cell::RefCell;

use crate::{OptionRow, OptionsWindow, ServiceRatingExampleRow};

/// The icon size and inc/dec height rows of each box, by the example's context.
const SIZES: [(&str, &str); 4] = [
    (
        "Media viewer like/dislike and numerical rating icon size:",
        "Media viewer inc/dec rating icon height:",
    ),
    (
        "Preview window like/dislike and numerical rating icon size:",
        "Preview window inc/dec rating icon height:",
    ),
    (
        "Thumbnail like/dislike and numerical rating icon size: ",
        "Thumbnail inc/dec rating height: ",
    ),
    (
        "Dialogs like/dislike and numerical rating icon size:",
        "Dialogs inc/dec rating height:",
    ),
];

/// The numerical service the examples are styled by: a numerical template
/// whole, or a like/dislike one's shape and colours on default stars; the
/// default stars when it is none of the store's.
pub(crate) fn template(store: &Store, key: &ServiceKey) -> NumericalRatingConfig {
    let plain = NumericalRatingConfig {
        display: RatingDisplay::default(),
        // (the reference's preview service: circles, a pad of four)
        appearance: hydrus_store::services::StarAppearance::Shape(
            hydrus_store::services::StarShape(0),
        ),
        num_stars: 5,
        allow_zero: true,
        custom_pad: 4,
        show_fraction_beside_stars: 0,
    };
    let snapshot = store.snapshot();
    match snapshot.services.by_key(key).map(|service| &service.kind) {
        Ok(ServiceKind::RatingNumerical(config)) => config.clone(),
        Ok(ServiceKind::RatingLike(config)) => NumericalRatingConfig {
            display: config.display.clone(),
            appearance: config.appearance.clone(),
            ..plain
        },
        _ => plain,
    }
}

struct Built {
    template: ServiceKey,
    numerical: ServiceKind,
    incdec: ServiceKind,
    stars: Example,
    counters: Example,
}

/// The examples' samples, made when the page first shows them; choosing another
/// service to style them changes only their colours and shape.
#[derive(Default)]
pub(crate) struct RatingExamples(RefCell<Option<Built>>);

impl RatingExamples {
    /// The key of the service chosen to style the stars.
    fn style_of(editor: &Editor) -> Option<ServiceKey> {
        editor.rows().iter().find_map(|row| match row {
            Row::Opt {
                option,
                value: Value::TagService(key),
                ..
            } if option.kind == Kind::RatingStyle => Some(key.clone()),
            _ => None,
        })
    }

    /// The sizes typed in context `context`'s box, whole pixels as the
    /// reference's `int( spin.value() )`.
    fn sizes(editor: &Editor, context: usize) -> (f64, f64) {
        let typed = |label: &str| {
            editor.rows().iter().find_map(|row| match row {
                Row::Opt {
                    option,
                    value: Value::Float(text),
                    ..
                } if option.label == label => match option.kind {
                    Kind::Float { min, max } => text
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .map(|value| value.clamp(min, max).trunc()),
                    _ => None,
                },
                _ => None,
            })
        };
        let (icon, height) = SIZES[context];
        (typed(icon).unwrap_or(12.0), typed(height).unwrap_or(12.0))
    }

    fn rebuild(&self, editor: &Editor, store: &Store) {
        let key = Self::style_of(editor).unwrap_or_else(|| ServiceKey::new(Vec::new()));
        let mut built = self.0.borrow_mut();
        if let Some(existing) = built.as_mut() {
            if existing.template == key {
                return;
            }
            // `SetServiceTemplate` clones only the colours and the shape of
            // the service chosen; the stars' count and the rest stay as the
            // template the panel opened with made them.
            if let ServiceKind::RatingNumerical(config) = &mut existing.numerical {
                let chosen = template(store, &key);
                config.display.colours = chosen.display.colours;
                config.appearance = chosen.appearance;
            }
            existing.template = key;
            return;
        }
        let numerical = ServiceKind::RatingNumerical(template(store, &key));
        let incdec = ServiceKind::RatingIncDec(RatingDisplay::default());
        let (Some(stars), Some(counters)) = (Example::new(&numerical), Example::new(&incdec))
        else {
            return;
        };
        *built = Some(Built {
            template: key,
            numerical,
            incdec,
            stars,
            counters,
        });
    }

    /// A press or drag on one of context `context`'s examples (`which` 0 the
    /// stars, 1 the inc/dec).
    pub(crate) fn pointer(
        &self,
        context: usize,
        which: i32,
        right: bool,
        (x, width, icon): (f64, f64, f64),
        drag: bool,
    ) {
        if let Some(built) = self.0.borrow_mut().as_mut() {
            let example = if which == 0 {
                &mut built.stars
            } else {
                &mut built.counters
            };
            example.pointer(context, right, x, width, icon, drag);
        }
    }

    /// The row for context `context`'s examples as the editor stands.
    fn row(
        &self,
        editor: &Editor,
        store: &Store,
        context: usize,
    ) -> Option<(ServiceRatingExampleRow, ServiceRatingExampleRow)> {
        self.rebuild(editor, store);
        let built = self.0.borrow();
        let built = built.as_ref()?;
        let (icon, height) = Self::sizes(editor, context);
        let sample = |example: &Example, kind: &ServiceKind| {
            let control = example.control(context, kind)?;
            let mut graphic = crate::rating_row(&control);
            let mut placement = 0;
            if let ServiceKind::RatingNumerical(config) = kind {
                graphic.pad = config.custom_pad as f32;
                placement = i32::from(config.show_fraction_beside_stars);
            }
            let counter_width = match example.samples()[context] {
                rating_example::Sample::IncDec(value) => {
                    rating_example::counter_width(height, value)
                }
                _ => 0.0,
            };
            Some(ServiceRatingExampleRow {
                label: slint::SharedString::new(),
                graphic,
                icon_size: icon as f32,
                incdec_height: height as f32,
                outline: ratings::outline_width(icon) as f32,
                counter_width: counter_width as f32,
                fraction: example.fraction(context, kind).into(),
                fraction_placement: placement,
            })
        };
        Some((
            sample(&built.stars, &built.numerical)?,
            sample(&built.counters, &built.incdec)?,
        ))
    }

    /// Each example row of the page `editor` shows: its place and samples.
    fn each(
        &self,
        editor: &Editor,
        store: &Store,
        mut apply: impl FnMut(usize, ServiceRatingExampleRow, ServiceRatingExampleRow),
    ) {
        for (at, row) in editor.rows().iter().enumerate() {
            if let Row::Opt { option, .. } = row
                && let Kind::RatingExamples(context) = option.kind
                && let Some((star, incdec)) = self.row(editor, store, context)
            {
                apply(at, star, incdec);
            }
        }
    }

    /// Fill the example rows among `rows` (a page as `editor` shows it).
    pub(crate) fn fill(&self, rows: &mut [OptionRow], editor: &Editor, store: &Store) {
        self.each(editor, store, |at, star, incdec| {
            if let Some(shown) = rows.get_mut(at) {
                shown.kind = 39;
                shown.star = star;
                shown.incdec = incdec;
            }
        });
    }

    /// Redraw the example rows on show in `window`, after an edit.
    pub(crate) fn refresh(&self, window: &OptionsWindow, editor: &Editor, store: &Store) {
        use slint::Model as _;
        let rows = window.get_rows();
        self.each(editor, store, |at, star, incdec| {
            if let Some(mut shown) = rows.row_data(at) {
                shown.star = star;
                shown.incdec = incdec;
                rows.set_row_data(at, shown);
            }
        });
    }
}
