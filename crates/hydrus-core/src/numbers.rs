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

/// Python's `int(text)` for an `i64` (see [`py_int_text`]).
pub fn py_int(text: &str) -> Option<i64> {
    py_int_text(text)?.parse().ok()
}

/// Python's `int(text)`, as the number's decimal text (Python's `str` of it:
/// no `+`, no leading zeros, `0` for minus zero), of any size: surrounding
/// whitespace (Unicode `White_Space`, which, unlike `str.strip`, leaves the
/// separators `\x1c` to `\x1f`), a sign, and decimal digits of any script
/// with single underscores between them.
pub fn py_int_text(text: &str) -> Option<String> {
    let t = text.trim_matches(char::is_whitespace);
    let (negative, digits) = match t.chars().next()? {
        '-' => (true, &t[1..]),
        '+' => (false, &t[1..]),
        _ => (false, t),
    };
    let mut ascii = String::with_capacity(digits.len());
    let mut previous_was_digit = false;
    let mut chars = digits.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' {
            let next_is_digit = chars.peek().is_some_and(|n| decimal_digit(*n).is_some());
            if !previous_was_digit || !next_is_digit {
                return None;
            }
            previous_was_digit = false;
            continue;
        }
        ascii.push(decimal_digit(c)?);
        previous_was_digit = true;
    }
    if ascii.is_empty() {
        return None;
    }
    let ascii = ascii.trim_start_matches('0');
    Some(match (ascii.is_empty(), negative) {
        (true, _) => "0".into(),
        (false, true) => format!("-{ascii}"),
        (false, false) => ascii.into(),
    })
}

/// A decimal digit of any script (Unicode `Nd` in [`DECIMAL_ZEROS`]) as its
/// ASCII digit.
fn decimal_digit(c: char) -> Option<char> {
    let code = c as u32;
    DECIMAL_ZEROS
        .iter()
        .find(|zero| (**zero..**zero + 10).contains(&code))
        .map(|zero| char::from(b'0' + u8::try_from(code - zero).unwrap_or(0)))
}

/// The first code point of each run of ten decimal digits (Unicode `Nd`)
/// in the Basic Multilingual Plane, which Python's `float()` and `int()`
/// read as the digits 0 to 9.
const DECIMAL_ZEROS: &[u32] = &[
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66,
    0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
];

/// Python's `float(text)`: surrounding whitespace allowed, a sign, decimal
/// digits of any script, single underscores between digits, an exponent,
/// and `inf`, `infinity` and `nan` in any case.
pub fn py_float(text: &str) -> Option<f64> {
    // (Unicode White_Space, as Python's float() strips; not \x1c to \x1f)
    let t = text.trim_matches(char::is_whitespace);
    let mut ascii = String::with_capacity(t.len());
    for c in t.chars() {
        let code = c as u32;
        let digit = DECIMAL_ZEROS
            .iter()
            .find(|zero| (**zero..**zero + 10).contains(&code))
            .map(|zero| char::from(b'0' + u8::try_from(code - zero).unwrap_or(0)));
        ascii.push(digit.unwrap_or(c));
    }
    // an underscore only between two digits
    let chars: Vec<char> = ascii.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '_'
            && !(i > 0
                && i + 1 < chars.len()
                && chars[i - 1].is_ascii_digit()
                && chars[i + 1].is_ascii_digit())
        {
            return None;
        }
    }
    let ascii: String = ascii.chars().filter(|c| *c != '_').collect();
    if ascii.is_empty() || !ascii.is_ascii() || ascii.contains(char::is_whitespace) {
        return None;
    }
    ascii.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {

    /// What Python 3.13's `int()` gives (`None` where it raises).
    #[test]
    fn python_ints_are_read_as_python_reads_them() {
        use super::{py_int, py_int_text};
        for (text, want) in [
            (" 5 ", Some("5")),
            ("\u{85}5", Some("5")),
            ("\u{2028}5", Some("5")),
            ("\u{3000}5", Some("5")),
            ("\u{1c}5", None),
            ("5\u{1f}", None),
            ("\u{200e}5", None),
            ("\u{665}", Some("5")),
            ("\u{ff15}", Some("5")),
            ("-\u{661}\u{662}", Some("-12")),
            ("1_0", Some("10")),
            ("1__0", None),
            ("_1", None),
            ("1_", None),
            ("+0042", Some("42")),
            ("-000", Some("0")),
            ("1.5", None),
            ("", None),
            ("-", None),
            (
                "123456789012345678901234567890",
                Some("123456789012345678901234567890"),
            ),
        ] {
            assert_eq!(py_int_text(text).as_deref(), want, "{text:?}");
        }
        assert_eq!(py_int("123456789012345678901234567890"), None);
        assert_eq!(py_int("-12"), Some(-12));
    }

    #[test]
    fn python_floats_are_read_as_python_reads_them() {
        for (text, want) in [
            ("1.5", Some(1.5)),
            (" 2 ", Some(2.0)),
            ("+3", Some(3.0)),
            (".5", Some(0.5)),
            ("5.", Some(5.0)),
            ("1e2", Some(100.0)),
            ("1_0", Some(10.0)),
            ("\u{ff11}", Some(1.0)),
            ("\u{663}", Some(3.0)),
            ("2\u{a0}", Some(2.0)),
            ("1__0", None),
            ("_1", None),
            ("1_", None),
            ("1e", None),
            ("1 2", None),
            ("", None),
            ("abc", None),
        ] {
            assert_eq!(py_float(text), want, "{text:?}");
        }
        assert!(py_float("nan").is_some_and(f64::is_nan));
        assert_eq!(py_float("-Infinity"), Some(f64::NEG_INFINITY));
    }
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
