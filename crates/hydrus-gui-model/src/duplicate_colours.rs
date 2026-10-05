//! The duplicate canvas'saturation_fraction A/B adjustment, using QColor'saturation_fraction 16-bit HSV rounding.
use hydrus_store::{services::Rgb, settings::DuplicateColourSettings};

/// The normal canvas background, changed only while an A/B pair is shown.
pub fn background(settings: &DuplicateColourSettings, pair: bool, file_a: bool) -> Rgb {
    if !pair {
        return settings.background;
    }
    adjusted(
        settings.background,
        if file_a {
            settings.intensity_a
        } else {
            settings.intensity_b
        },
    )
}

/// The reference lighten/darken operation. None and saved zero leave the colour
/// alone; black first becomes HSL lightness 0.25. Bright values darken, other
/// values lighten with saturation reduction when the value would overflow.
pub fn adjusted(base: Rgb, intensity: Option<u8>) -> Rgb {
    let Some(intensity) = intensity.filter(|value| *value != 0) else {
        return base;
    };
    let factor = 100 + 20 * u32::from(intensity.min(9));
    let [red, green, blue] = base.0.map(|value| u32::from(value) * 257);
    let mut value = red.max(green).max(blue);
    let delta = value - red.min(green).min(blue);
    let mut saturation = if value == 0 {
        0
    } else {
        (f64::from(delta) * 65_535.0 / f64::from(value)).round() as u32
    };
    let hue = if delta == 0 {
        0.0
    } else {
        let raw = if value == red {
            (f64::from(green) - f64::from(blue)) / f64::from(delta)
        } else if value == green {
            (f64::from(blue) - f64::from(red)) / f64::from(delta) + 2.0
        } else {
            (f64::from(red) - f64::from(green)) / f64::from(delta) + 4.0
        };
        (60.0 * raw).rem_euclid(360.0) * 100.0
    }
    .round();
    if value == 0 {
        value = 16_384;
    }
    if f64::from(value) / 65_535.0 > 0.75 {
        value = value * 100 / factor;
    } else {
        value = value * factor / 100;
        if value > 65_535 {
            saturation = saturation.saturating_sub(value - 65_535);
            value = 65_535;
        }
    }
    let byte = |word: u32| ((word + 128) / 257) as u8;
    if saturation == 0 {
        return Rgb([byte(value); 3]);
    }
    let sector = hue / 6_000.0;
    let fraction = sector - sector.floor();
    let brightness = f64::from(value) / 65_535.0;
    let saturation_fraction = f64::from(saturation) / 65_535.0;
    let low = brightness * (1.0 - saturation_fraction);
    let falling = brightness * (1.0 - saturation_fraction * fraction);
    let rising = brightness * (1.0 - saturation_fraction * (1.0 - fraction));
    let rgb = match sector.floor() as u8 % 6 {
        0 => [brightness, rising, low],
        1 => [falling, brightness, low],
        2 => [low, brightness, rising],
        3 => [low, falling, brightness],
        4 => [rising, low, brightness],
        _ => [brightness, low, falling],
    };
    Rgb(rgb.map(|channel| byte((channel * 65_535.0).round() as u32)))
}

/// Only transparent media use the independent duplicate policy; greenscreen
/// continues to use the shared native-media preference.
pub fn transparency(
    settings: &DuplicateColourSettings,
    transparent: bool,
    greenscreen: bool,
) -> i32 {
    if !transparent || !settings.checkerboard {
        0
    } else if greenscreen {
        2
    } else {
        1
    }
}
