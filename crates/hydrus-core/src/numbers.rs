//! Numbers as the reference writes them in human-readable text.

/// An integer with thousands separators: `1234567` -> `1,234,567`
/// (the reference's `HydrusNumbers.ToHumanInt`).
pub fn human_int(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A value of a range, as `processed/total`
/// (`HydrusNumbers.ValueRangeToPrettyString`).
pub fn value_range(value: u64, range: u64) -> String {
    format!("{}/{}", human_int(value.min(range)), human_int(range))
}

/// A fraction as a percentage with one decimal place, or none for a whole
/// number: `0.5` -> `50%`, `-0.8951` -> `-89.5%`
/// (`HydrusNumbers.FloatToPercentage`).
#[allow(clippy::float_cmp)] // as the reference tests for a whole number
pub fn float_to_percentage(f: f64) -> String {
    let percent = f * 100.0;
    if percent == percent.trunc() {
        format!("{}%", percent as i64)
    } else {
        format!("{percent:.1}%")
    }
}

/// A size as the reference words it (`HydrusData.BaseToHumanBytes` at its
/// default three significant figures): `512B`, `1 KB`, `1.5 KB`, `237 KB`,
/// `10.3 MB`; every whole digit is kept, and the rounding is half to even,
/// on the exact value.
pub fn human_bytes(size: u64) -> String {
    human_bytes_with_figures(size, 3)
}

/// Client-configured pseudo significant figures, preserving every whole digit.
pub fn human_bytes_with_figures(size: u64, figures: u8) -> String {
    human_bytes_ratio_with_figures(u128::from(size), 1, figures)
}

/// [`human_bytes`] of `numerator / denominator` exactly (a rate, say): as
/// the reference words a float, under 1 KB its whole bytes.
pub fn human_bytes_ratio(numerator: u128, denominator: u128) -> String {
    human_bytes_ratio_with_figures(numerator, denominator, 3)
}

/// The same exact half-even rounding for a rate with configured precision.
pub fn human_bytes_ratio_with_figures(numerator: u128, denominator: u128, figures: u8) -> String {
    let denominator = denominator.max(1);
    if numerator < 1024 * denominator {
        return format!("{}B", human_int((numerator / denominator) as u64));
    }
    let mut divisor: u128 = denominator;
    let mut suffix = 0;
    while numerator >= divisor * 1024 && suffix < 5 {
        divisor *= 1024;
        suffix += 1;
    }
    let whole_digits = (numerator / divisor).to_string().len();
    let decimals = usize::from(figures.clamp(1, 6)).saturating_sub(whole_digits);
    // the value to `decimals` places, rounded half to even
    let scaled = numerator * 10u128.pow(decimals as u32);
    let (mut rounded, remainder) = (scaled / divisor, scaled % divisor);
    if remainder * 2 > divisor || (remainder * 2 == divisor && rounded % 2 == 1) {
        rounded += 1;
    }
    let digits = rounded.to_string();
    let text = if decimals == 0 {
        digits
    } else {
        let digits = format!("{digits:0>width$}", width = decimals + 1);
        let (whole, fraction) = digits.split_at(digits.len() - decimals);
        let fraction = fraction.trim_end_matches('0');
        if fraction.is_empty() {
            whole.to_owned()
        } else {
            format!("{whole}.{fraction}")
        }
    };
    format!("{text} {}B", ["", "K", "M", "G", "T", "P"][suffix])
}

/// A resolution as the reference writes it (`ClientData.
/// ResolutionToPrettyString`, with its default of naming common video
/// resolutions): `1,920x1,200`, `1080p`, `vertical 720p`.
pub fn resolution_text(width: u64, height: u64) -> String {
    let nice = match (width, height) {
        (640, 480) => "480p",
        (1280, 720) => "720p",
        (1920, 1080) => "1080p",
        (3840, 2160) => "2160p",
        (720, 1280) => "vertical 720p",
        (1080, 1920) => "vertical 1080p",
        (2160, 3840) => "vertical 2160p",
        _ => return format!("{}x{}", human_int(width), human_int(height)),
    };
    nice.to_owned()
}

/// Python's `int(text)` for an `i64`: surrounding whitespace allowed, a
/// sign, digits with single underscores between them.
pub fn py_int(text: &str) -> Option<i64> {
    let t = text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'));
    let (negative, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    let value: i64 = digits.replace('_', "").parse().ok()?;
    Some(if negative { -value } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(human_int(0), "0");
        assert_eq!(human_int(999), "999");
        assert_eq!(human_int(1000), "1,000");
        assert_eq!(human_int(1_234_567), "1,234,567");
    }

    #[test]
    fn sizes_read_like_the_reference() {
        // (checked against BaseToHumanBytes)
        for (size, text) in [
            (0, "0B"),
            (512, "512B"),
            (1023, "1,023B"),
            (1024, "1 KB"),
            (1152, "1.12 KB"),
            (1536, "1.5 KB"),
            (2775, "2.71 KB"),
            (237 * 1024, "237 KB"),
            (1_048_064, "1024 KB"),
            (10 * 1024 * 1024 + 300 * 1024, "10.3 MB"),
            (188_213_746, "179 MB"),
            (5 * 1024 * 1024 * 1024, "5 GB"),
            (1_023_576, "1000 KB"),
            (1_048_575, "1024 KB"),
        ] {
            assert_eq!(human_bytes(size), text, "{size}");
        }
        // floats, as rates (checked against BaseToHumanBytes)
        for (numerator, denominator, text) in [
            (5127, 10, "512B"),
            (46186, 10, "4.51 KB"),
            (2_771_000, 600, "4.51 KB"),
            (10_239_999, 10_000, "1,023B"),
            (15270, 1, "14.9 KB"),
            (10_245_120, 10, "1000 KB"),
        ] {
            assert_eq!(human_bytes_ratio(numerator, denominator), text);
        }
    }

    #[test]
    fn percentages() {
        assert_eq!(float_to_percentage(0.5), "50%");
        assert_eq!(float_to_percentage(19.679), "1967.9%");
        assert_eq!(float_to_percentage(-0.895_12), "-89.5%");
    }
}
