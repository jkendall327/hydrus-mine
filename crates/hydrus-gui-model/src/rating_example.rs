//! Four independent, non-persistent samples for a staged rating service.
use crate::ratings::{self, Control, Kind};
use hydrus_core::ServiceId;
use hydrus_store::services::{NumericalRatingConfig, ServiceKind};

pub const LABELS: [&str; 4] = [
    "Thumbnails",
    "Media Viewer",
    "Preview Window",
    "Dialog (Default)",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sample {
    Like(Option<bool>),
    Numerical(Option<f64>),
    IncDec(u32),
}

#[derive(Debug, Clone)]
pub struct Example {
    samples: [Sample; 4],
    // Qt's numerical example retains its opening click conversion even when
    // other service settings are live. The allow-zero checkbox emits no preview signal.
    click_config: Option<NumericalRatingConfig>,
}
impl Example {
    pub fn new(kind: &ServiceKind) -> Option<Self> {
        let (sample, click_config) = match kind {
            ServiceKind::RatingLike(_) => (Sample::Like(None), None),
            ServiceKind::RatingNumerical(c) => (Sample::Numerical(None), Some(c.clone())),
            ServiceKind::RatingIncDec(_) => (Sample::IncDec(0), None),
            _ => return None,
        };
        Some(Self {
            samples: [sample; 4],
            click_config,
        })
    }
    pub fn samples(&self) -> &[Sample; 4] {
        &self.samples
    }
    /// `proportion` is measured over the star area, excluding a fraction label.
    pub fn click(&mut self, index: usize, right: bool, proportion: f64) {
        let Some(sample) = self.samples.get_mut(index) else {
            return;
        };
        match sample {
            Sample::Like(state) => {
                let wanted = !right;
                *state = (*state != Some(wanted)).then_some(wanted);
            }
            Sample::Numerical(value) => {
                if right {
                    *value = None;
                } else if let Some(conversion) = &self.click_config {
                    *value = Some(
                        conversion
                            .rating(ratings::stars_at(conversion, proportion.clamp(0.0, 1.0))),
                    );
                }
            }
            Sample::IncDec(value) => {
                *value = if right {
                    value.saturating_sub(1)
                } else {
                    value.saturating_add(1)
                };
            }
        }
    }
    /// Whole-widget Qt pointer conversion, including the opening fraction side.
    /// Dragging outside its horizontal active region keeps the last valid value;
    /// an outside press clears. Hover motion is filtered by the UI owner.
    pub fn pointer(
        &mut self,
        index: usize,
        right: bool,
        x: f64,
        width: f64,
        icon: f64,
        drag: bool,
    ) {
        let Some(conversion) = self.click_config.as_ref() else {
            if !drag {
                self.click(index, right, 0.0);
            }
            return;
        };
        if right {
            self.click(index, true, 0.0);
            return;
        }
        let x = x.round();
        let width = width.round();
        if x < 1.0 || x > width - 2.0 {
            if !drag {
                self.click(index, true, 0.0);
            }
            return;
        }
        let adjusted = x - 1.0;
        let active_width = width - 2.0;
        let fraction_width =
            (icon - 1.0) * (conversion.num_stars.to_string().len() * 2 + 1) as f64 / 2.0;
        let proportion = match conversion.show_fraction_beside_stars {
            1 => (adjusted.max(fraction_width) - fraction_width) / (active_width - fraction_width),
            2 => adjusted.min(active_width - fraction_width) / (active_width - fraction_width),
            _ => adjusted / active_width,
        };
        self.click(index, false, proportion);
    }
    pub fn set_counter(&mut self, index: usize, value: u32) {
        if let Some(Sample::IncDec(current)) = self.samples.get_mut(index) {
            *current = value.min(1_000_000);
        }
    }
    pub fn control(&self, index: usize, kind: &ServiceKind) -> Option<Control> {
        let sample = self.samples.get(index)?;
        let (kind, colours) = match (sample, kind) {
            (Sample::Like(state), ServiceKind::RatingLike(c)) => (
                Kind::Like {
                    state: *state,
                    shape: ratings::shape_path(&c.appearance),
                },
                c.display.colours,
            ),
            (Sample::Numerical(value), ServiceKind::RatingNumerical(c)) => (
                Kind::Numerical {
                    stars: value.map(|v| c.stars(v)),
                    config: c.clone(),
                    shape: ratings::shape_path(&c.appearance),
                },
                c.display.colours,
            ),
            (Sample::IncDec(value), ServiceKind::RatingIncDec(c)) => (
                Kind::IncDec {
                    value: i64::from(*value),
                },
                c.colours,
            ),
            _ => return None,
        };
        Some(Control {
            service: ServiceId(0),
            name: LABELS[index].into(),
            kind,
            colours,
        })
    }
    pub fn fraction(&self, index: usize, kind: &ServiceKind) -> String {
        let Some(Control {
            kind: Kind::Numerical { stars, config, .. },
            ..
        }) = self.control(index, kind)
        else {
            return String::new();
        };
        let numerator = stars.map_or_else(|| "-".into(), |v| v.to_string());
        format!(
            "{numerator:>width$}/{}",
            config.num_stars,
            width = config.num_stars.to_string().len()
        )
    }
}

/// Qt's counter grows to fit larger values, independently of other samples.
pub fn counter_width(height: f64, value: u32) -> f64 {
    let digits = f64::from(value.max(1).ilog10() + 1);
    (height * 2.0
        + if digits > 3.0 {
            (height - 1.0) * (digits - (2.0 + digits / 3.0))
        } else {
            0.0
        })
    .trunc()
}
