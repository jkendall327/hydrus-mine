//! The shape perceptual hash used for similar-file search
//! (`ClientImagePerceptualHashes.GenerateShapePerceptualHashNumPy`).
//!
//! Greyscale (alpha composited onto white), `INTER_AREA` down to 32x32, an
//! orthonormal 2D DCT-II, and one bit per low-frequency 8x8 coefficient:
//! above or below their median (excluding the DC term). The OpenCV steps are
//! bit-exact ports; the DCT is computed directly in f64 rather than via
//! numpy's FFT, so coefficients can differ in the last bits, which only
//! matters when two of them tie within rounding error (flat or perfectly
//! symmetric images).

use hydrus_core::PerceptualHash;

use crate::imaging::Raster;
use crate::imaging::cv::{self, Interpolation};

/// `BLANK_PERCEPTUAL_HASH`: what a flat colour image hashes to.
pub const BLANK_PERCEPTUAL_HASH: PerceptualHash = PerceptualHash([0x80, 0, 0, 0, 0, 0, 0, 0]);

/// Whether the client would discard this hash as uninformative
/// (`DiscardBlankPerceptualHashes`: within 4 bits of the blank hash).
pub fn is_blank(hash: &PerceptualHash) -> bool {
    hash.distance(&BLANK_PERCEPTUAL_HASH) <= 4
}

/// Greyscale as the reference prepares it.
fn greyscale(image: &Raster) -> Raster {
    match image.channels() {
        4 => {
            let small = cv::resize(
                image,
                image.width().min(256),
                image.height().min(256),
                Interpolation::Area,
            );
            let grey = cv::rgb_to_gray(&small);
            let data = grey
                .data()
                .iter()
                .zip(small.data().chunks_exact(4))
                .map(|(&g, px)| {
                    let a = f64::from(px[3]);
                    let v = f64::from(g) * (a / 255.0) + (255.0 - a);
                    v as u8
                })
                .collect();
            Raster::new(small.width(), small.height(), 1, data).expect("sized from source")
        }
        3 => cv::rgb_to_gray(image),
        2 => {
            let data = image.data().chunks_exact(2).map(|p| p[0]).collect();
            Raster::new(image.width(), image.height(), 1, data).expect("sized from source")
        }
        _ => image.clone(),
    }
}

const N: usize = 32;

fn cos_sum(values: &[f64], k: usize, n: usize) -> f64 {
    values
        .iter()
        .enumerate()
        .map(|(x, v)| {
            v * (std::f64::consts::PI * (2 * x + 1) as f64 * k as f64 / (2 * n) as f64).cos()
        })
        .sum()
}

/// Unnormalised DCT-II terms 0..8 of 32 samples, sum_x f(x) cos(pi (2x+1) k / 64).
///
/// Computed with the even/odd butterfly (pairing x with N-1-x, recursively),
/// so a frequency that is zero by symmetry comes out exactly zero, as it does
/// through numpy's FFT. Flat images then hash to exactly the blank hash.
fn dct_low8(f: &[f64; N]) -> [f64; 8] {
    let s1: Vec<f64> = (0..16).map(|x| f[x] + f[N - 1 - x]).collect();
    let d1: Vec<f64> = (0..16).map(|x| f[x] - f[N - 1 - x]).collect();
    let s2: Vec<f64> = (0..8).map(|x| s1[x] + s1[15 - x]).collect();
    let d2: Vec<f64> = (0..8).map(|x| s1[x] - s1[15 - x]).collect();
    let s3: Vec<f64> = (0..4).map(|x| s2[x] + s2[7 - x]).collect();
    let d3: Vec<f64> = (0..4).map(|x| s2[x] - s2[7 - x]).collect();
    let mut out = [0f64; 8];
    out[0] = s3.iter().sum();
    for k in [1, 3, 5, 7] {
        out[k] = cos_sum(&d1, k, N);
    }
    for k in [2, 6] {
        out[k] = cos_sum(&d2, k / 2, N / 2);
    }
    out[4] = cos_sum(&d3, 1, N / 4);
    out
}

/// The 8x8 low-frequency block of the orthonormal DCT-II of a 32x32 image
/// (what the reference's `PILDCT` emulation of `cv2.dct` returns).
fn dct_8x8(pixels: &[u8]) -> [[f64; 8]; 8] {
    let scale = |k: usize| {
        if k == 0 {
            (1.0 / N as f64).sqrt()
        } else {
            (2.0 / N as f64).sqrt()
        }
    };
    // along each row first, then down each of the 8 resulting columns
    let rows: Vec<[f64; 8]> = (0..N)
        .map(|x| {
            let row: [f64; N] = std::array::from_fn(|y| f64::from(pixels[x * N + y]));
            dct_low8(&row)
        })
        .collect();
    let mut out = [[0f64; 8]; 8];
    for v in 0..8 {
        let column: [f64; N] = std::array::from_fn(|x| rows[x][v]);
        let c = dct_low8(&column);
        for u in 0..8 {
            out[u][v] = c[u] * scale(u) * scale(v);
        }
    }
    out
}

/// The 64-bit perceptual hash of a decoded image.
pub fn perceptual_hash(image: &Raster) -> PerceptualHash {
    use hydrus_core::debug_flags::{Flag, report};
    let say = |text: &dyn Fn() -> String| {
        report(Flag::SimilarFilesMetadataGenerationReport, || {
            format!("phash generation: {}", text())
        });
    };
    say(&|| {
        format!(
            "image shape: ({}, {}, {})",
            image.height(),
            image.width(),
            image.channels()
        )
    });
    let grey = greyscale(image);
    say(&|| format!("grey image shape: ({}, {})", image.height(), image.width()));
    let tiny = cv::resize(&grey, 32, 32, Interpolation::Area);
    say(&|| "tiny image shape: (32, 32)".to_owned());
    say(&|| "tiny float image shape: (32, 32)".to_owned());
    say(&|| "generating dct".to_owned());
    let dct = dct_8x8(tiny.data());
    let flat: Vec<f64> = dct.iter().flatten().copied().collect();
    let mut rest: Vec<f64> = flat[1..].to_vec();
    rest.sort_by(f64::total_cmp);
    let median = rest[rest.len() / 2];
    say(&|| format!("median: {median:?}"));
    say(&|| "collapsing bytes".to_owned());
    let mut bytes = [0u8; 8];
    for (i, row) in dct.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            if *v > median {
                bytes[i] |= 0x80 >> j;
            }
        }
    }
    say(&|| format!("perceptual_hash: {}", hex::encode(bytes)));
    PerceptualHash(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_image_is_blank() {
        let img = Raster::new(40, 40, 3, vec![120; 40 * 40 * 3]).unwrap();
        let h = perceptual_hash(&img);
        assert!(is_blank(&h), "{h:?}");
    }
}
