//! The thumbnail grid's live scroll decisions, independent of UI widgets.

/// Python float text: surrounding whitespace, Unicode decimal digits and
/// underscores only between decimal digits. Keep the caller's original text.
pub fn parse_rate(text: &str) -> Option<f64> {
    let normalized: String = text
        .trim()
        .chars()
        .map(|c| {
            hydrus_core::tag_presentation::decimal_digit(c)
                .and_then(|digit| char::from_digit(digit, 10))
                .unwrap_or(c)
        })
        .collect();
    let bytes = normalized.as_bytes();
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b'_'
            && (i == 0
                || i + 1 == bytes.len()
                || !bytes[i - 1].is_ascii_digit()
                || !bytes[i + 1].is_ascii_digit())
        {
            return None;
        }
    }
    normalized.replace('_', "").parse().ok()
}

/// Qt takes the absolute signed-int singleStep; a non-finite rate leaves the
/// previous step after the reference exception. Python round uses ties-to-even.
pub fn single_step(span: f64, rate: &str, previous: i32) -> i32 {
    let Some(rate) = parse_rate(rate) else {
        return previous;
    };
    let value = (span * rate).round_ties_even().abs();
    if value.is_finite() && value >= 0.0 && value <= f64::from(i32::MAX) {
        value as i32
    } else {
        previous
    }
}

/// Reference ScrollToMedia chooses the top or bottom point using strict >,
/// then QGraphicsView ensureVisible applies its default 50px point margin.
pub fn scroll_target(
    top: f64,
    span: f64,
    offset: f64,
    viewport: f64,
    content: f64,
    percent: u8,
) -> f64 {
    let point = if top < offset || top <= offset + viewport - span * f64::from(percent) / 100.0 {
        top
    } else {
        top + span
    };
    let target = if point < offset + 50.0 {
        point - 50.0
    } else if point > offset + viewport - 50.0 {
        point + 50.0 - viewport
    } else {
        offset
    };
    target.clamp(0.0, (content - viewport).max(0.0))
}

/// Winit normalizes one line tick to60px; Qt's default wheel scrolls three
/// singleSteps, capped to a page. Smooth physical-pixel deltas use this scale.
pub fn wheel_target(delta: f64, step: i32, viewport: f64, offset: f64) -> f64 {
    let distance = (delta / 60.0 * 3.0 * f64::from(step)).clamp(-viewport, viewport);
    offset - distance
}
