//! Blurhash, as the reference computes it from a thumbnail
//! (`HydrusBlurhash.GetBlurhashFromNumPy` + the vendored `blurhash.py`).
//!
//! The arithmetic mirrors the Python operation by operation in f64 (with the
//! same libm `cos`/`pow`), so equal input pixels give an equal string.

use crate::imaging::Raster;
use crate::imaging::cv::Interpolation;

const ALPHABET: &[u8; 83] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

fn base83(value: u64, length: u32, out: &mut String) {
    for i in 1..=length {
        let digit = (value / 83u64.pow(length - i)) % 83;
        out.push(char::from(ALPHABET[digit as usize]));
    }
}

fn srgb_to_linear(v: u8) -> f64 {
    let v = f64::from(v) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(v: f64) -> u64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        (v * 12.92 * 255.0 + 0.5) as u64
    } else {
        ((1.055 * v.powf(1.0 / 2.4) - 0.055) * 255.0 + 0.5) as u64
    }
}

fn sign_pow(v: f64, e: f64) -> f64 {
    v.abs().powf(e).copysign(v)
}

/// `blurhash_encode` with the given component counts (RGB of the first three channels).
fn encode(image: &Raster, cx: usize, cy: usize) -> String {
    let (w, h, c) = (
        image.width() as usize,
        image.height() as usize,
        usize::from(image.channels()),
    );
    let (wf, hf) = (w as f64, h as f64);
    let linear: Vec<[f64; 3]> = image
        .data()
        .chunks_exact(c)
        .map(|p| {
            [
                srgb_to_linear(p[0]),
                srgb_to_linear(p[1]),
                srgb_to_linear(p[2]),
            ]
        })
        .collect();
    let mut components = Vec::with_capacity(cx * cy);
    let mut max_ac = 0.0f64;
    for j in 0..cy {
        for i in 0..cx {
            let norm = if i == 0 && j == 0 { 1.0 } else { 2.0 };
            let mut comp = [0.0f64; 3];
            for y in 0..h {
                for x in 0..w {
                    let basis = norm
                        * (std::f64::consts::PI * i as f64 * x as f64 / wf).cos()
                        * (std::f64::consts::PI * j as f64 * y as f64 / hf).cos();
                    let px = linear[y * w + x];
                    comp[0] += basis * px[0];
                    comp[1] += basis * px[1];
                    comp[2] += basis * px[2];
                }
            }
            for v in &mut comp {
                *v /= wf * hf;
            }
            if !(i == 0 && j == 0) {
                max_ac = max_ac
                    .max(comp[0].abs())
                    .max(comp[1].abs())
                    .max(comp[2].abs());
            }
            components.push(comp);
        }
    }
    let dc = components[0];
    let dc_value =
        (linear_to_srgb(dc[0]) << 16) + (linear_to_srgb(dc[1]) << 8) + linear_to_srgb(dc[2]);
    let quant_max = (max_ac * 166.0 - 0.5).floor().clamp(0.0, 82.0) as u64;
    let norm = (quant_max + 1) as f64 / 166.0;
    let q = |v: f64| {
        (sign_pow(v / norm, 0.5) * 9.0 + 9.5)
            .floor()
            .clamp(0.0, 18.0) as u64
    };
    let mut out = String::new();
    base83(((cx - 1) + (cy - 1) * 9) as u64, 1, &mut out);
    base83(quant_max, 1, &mut out);
    base83(dc_value, 4, &mut out);
    for comp in &components[1..] {
        base83(
            q(comp[0]) * 19 * 19 + q(comp[1]) * 19 + q(comp[2]),
            2,
            &mut out,
        );
    }
    out
}

/// `GetBlurhashFromNumPy`: component counts from the aspect ratio, and a
/// bilinear reduction to at most 100x100 first.
pub fn blurhash(image: &Raster) -> Option<String> {
    let (w, h) = (image.width(), image.height());
    if w == 0 || h == 0 {
        return Some(String::new());
    }
    if image.channels() < 3 {
        return None;
    }
    let ratio = f64::from(w) / f64::from(h);
    let (cx, cy) = if ratio > 4.0 / 3.0 {
        (5, 3)
    } else if ratio < 3.0 / 4.0 {
        (3, 5)
    } else {
        (4, 4)
    };
    // through ResizeNumPyImage, whose shortcut skips e.g. 100x160 -> 100x100
    let small;
    let image = if w > 100 || h > 100 {
        small = crate::thumbnail::resize(image, (100, 100), Some(Interpolation::Linear));
        &small
    } else {
        image
    };
    Some(encode(image, cx, cy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base83_encoding() {
        let mut s = String::new();
        base83(83 * 83 + 5, 3, &mut s);
        assert_eq!(s, "105");
    }

    #[test]
    fn flat_colours_match_reference() {
        // values from HydrusBlurhash.GetBlurhashFromNumPy; the integer-x
        // basis gives even a flat image some AC energy
        let white = Raster::new(10, 10, 3, vec![255; 300]).unwrap();
        assert_eq!(
            blurhash(&white).unwrap(),
            "UWTSUA~qfQ~q~qt7fQt7fQfQfQfQ~qt7fQt7"
        );
        let blue = Raster::new(30, 10, 3, [10, 120, 200].repeat(300)).unwrap();
        assert_eq!(
            blurhash(&blue).unwrap(),
            "MI1Gpag5fQg5fQk]flfQflfQfQfQfQfQfQ"
        );
    }
}
