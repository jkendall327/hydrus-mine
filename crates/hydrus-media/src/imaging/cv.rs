//! Ports of the OpenCV (4.x) routines the reference calls on 8-bit images.
//!
//! `cv2.resize` with `INTER_AREA` and `INTER_LINEAR` and `cv2.cvtColor(RGB2GRAY)`
//! are reproduced bit-exactly, including OpenCV's choice of code path
//! (integer-scale "fast area", float "generic area", fixed-point bilinear) and
//! its rounding (`cvRound` is round-half-even; some paths use `(x+2)>>2`).
//! Perceptual hashes depend on this.
//!
//! `INTER_LANCZOS4` is OpenCV's own (non-IPP) implementation. The reference's
//! OpenCV wheel routes 8-bit Lanczos through Intel IPP, which we cannot
//! reproduce; Lanczos is only used for thumbnails, whose pixels need not match.

use super::Raster;

/// `cv2.INTER_*` choices used by the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Area,
    Linear,
    Lanczos4,
}

const COEF_BITS: i32 = 11;
const COEF_SCALE: i32 = 1 << COEF_BITS;

/// `cvRound` for doubles: round half to even (SSE2 conversion semantics).
fn cv_round_f64(v: f64) -> i32 {
    let r = v.round_ties_even();
    if r.is_nan() {
        i32::MIN
    } else {
        r.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    }
}

/// `cvRound` for floats.
fn cv_round_f32(v: f32) -> i32 {
    let r = v.round_ties_even();
    if r.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&r) {
        i32::MIN
    } else {
        r as i32
    }
}

fn sat_u8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

fn sat_i16(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

/// `cv2.resize(src, (dw, dh), interpolation=...)` on a u8 image.
pub(crate) fn resize(src: &Raster, dw: u32, dh: u32, interpolation: Interpolation) -> Raster {
    let (sw, sh) = (src.width(), src.height());
    if (dw, dh) == (sw, sh) || dw == 0 || dh == 0 || sw == 0 || sh == 0 {
        return if (dw, dh) == (sw, sh) {
            src.clone()
        } else {
            blank(dw, dh, src.channels())
        };
    }
    let inv_x = f64::from(dw) / f64::from(sw);
    let inv_y = f64::from(dh) / f64::from(sh);
    let scale_x = 1.0 / inv_x;
    let scale_y = 1.0 / inv_y;
    let iscale_x = cv_round_f64(scale_x);
    let iscale_y = cv_round_f64(scale_y);
    let is_area_fast = (scale_x - f64::from(iscale_x)).abs() < f64::EPSILON
        && (scale_y - f64::from(iscale_y)).abs() < f64::EPSILON;
    let mut interpolation = interpolation;
    if interpolation == Interpolation::Linear && is_area_fast && iscale_x == 2 && iscale_y == 2 {
        interpolation = Interpolation::Area;
    }
    if interpolation == Interpolation::Area && scale_x >= 1.0 && scale_y >= 1.0 {
        return if is_area_fast {
            area_fast(src, dw, dh, iscale_x as usize, iscale_y as usize)
        } else {
            area_generic(src, dw, dh, scale_x, scale_y)
        };
    }
    generic(src, dw, dh, interpolation, scale_x, scale_y, inv_x, inv_y)
}

fn blank(w: u32, h: u32, c: u8) -> Raster {
    Raster::new(w, h, c, vec![0; w as usize * h as usize * usize::from(c)])
        .expect("buffer sized for its dimensions")
}

/// `resizeAreaFast_`: integer downscale factors.
fn area_fast(src: &Raster, dw: u32, dh: u32, sx: usize, sy: usize) -> Raster {
    let cn = usize::from(src.channels());
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let swc = sw * cn;
    let dwc = dw as usize * cn;
    let s = src.data();
    let area = sx * sy;
    let scale = 1.0f32 / area as f32;
    let dwidth1 = (sw / sx) * cn;
    let fast_mode = sx == 2 && sy == 2 && matches!(cn, 1 | 3 | 4);
    let xofs = |dx: usize| (dx / cn) * sx * cn + dx % cn;
    let mut out = vec![0u8; dwc * dh as usize];
    for dy in 0..dh as usize {
        let row = &mut out[dy * dwc..(dy + 1) * dwc];
        let sy0 = dy * sy;
        if sy0 >= sh {
            continue;
        }
        let w = if sy0 + sy <= sh { dwidth1 } else { 0 };
        let base = sy0 * swc;
        if fast_mode {
            for (dx, d) in row.iter_mut().enumerate().take(w) {
                let i = base + xofs(dx);
                let sum = u32::from(s[i])
                    + u32::from(s[i + cn])
                    + u32::from(s[i + swc])
                    + u32::from(s[i + swc + cn]);
                *d = ((sum + 2) >> 2) as u8;
            }
        } else {
            for (dx, d) in row.iter_mut().enumerate().take(w) {
                let x0 = base + xofs(dx);
                let mut sum = 0i32;
                for a in 0..sy {
                    for b in 0..sx {
                        sum += i32::from(s[x0 + a * swc + b * cn]);
                    }
                }
                *d = sat_u8(cv_round_f32(sum as f32 * scale));
            }
        }
        for (dx, d) in row.iter_mut().enumerate().skip(w) {
            let sx0 = xofs(dx);
            if sx0 >= swc {
                *d = 0;
            }
            let mut sum = 0i32;
            let mut count = 0i32;
            for a in 0..sy {
                if sy0 + a >= sh {
                    break;
                }
                for b in (0..sx * cn).step_by(cn) {
                    if sx0 + b >= swc {
                        break;
                    }
                    sum += i32::from(s[(sy0 + a) * swc + sx0 + b]);
                    count += 1;
                }
            }
            *d = sat_u8(cv_round_f32(sum as f32 / count as f32));
        }
    }
    Raster::new(dw, dh, src.channels(), out).expect("buffer sized for its dimensions")
}

#[derive(Debug, Clone, Copy)]
struct Decimate {
    di: usize,
    si: usize,
    alpha: f32,
}

/// `computeResizeAreaTab`.
fn area_tab(ssize: usize, dsize: usize, cn: usize, scale: f64) -> Vec<Decimate> {
    let mut tab = Vec::with_capacity(ssize * 2);
    for dx in 0..dsize {
        let fsx1 = dx as f64 * scale;
        let fsx2 = fsx1 + scale;
        let cell_width = scale.min(ssize as f64 - fsx1);
        let mut sx1 = fsx1.ceil() as i64;
        let mut sx2 = fsx2.floor() as i64;
        sx2 = sx2.min(ssize as i64 - 1);
        sx1 = sx1.min(sx2);
        if sx1 as f64 - fsx1 > 1e-3 {
            tab.push(Decimate {
                di: dx * cn,
                si: (sx1 - 1) as usize * cn,
                alpha: ((sx1 as f64 - fsx1) / cell_width) as f32,
            });
        }
        for sx in sx1..sx2 {
            tab.push(Decimate {
                di: dx * cn,
                si: sx as usize * cn,
                alpha: (1.0 / cell_width) as f32,
            });
        }
        if fsx2 - sx2 as f64 > 1e-3 {
            tab.push(Decimate {
                di: dx * cn,
                si: sx2 as usize * cn,
                alpha: ((fsx2 - sx2 as f64).min(1.0).min(cell_width) / cell_width) as f32,
            });
        }
    }
    tab
}

/// `resizeArea_<uchar, float>`: non-integer downscale factors.
fn area_generic(src: &Raster, dw: u32, dh: u32, scale_x: f64, scale_y: f64) -> Raster {
    let cn = usize::from(src.channels());
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let swc = sw * cn;
    let dwc = dw as usize * cn;
    let s = src.data();
    let xtab = area_tab(sw, dw as usize, cn, scale_x);
    let ytab = area_tab(sh, dh as usize, 1, scale_y);
    let mut out = vec![0u8; dwc * dh as usize];
    let mut buf = vec![0f32; dwc];
    let mut sum = vec![0f32; dwc];
    let store = |sum: &[f32], out: &mut [u8], dy: usize| {
        for (d, v) in out[dy * dwc..(dy + 1) * dwc].iter_mut().zip(sum) {
            *d = sat_u8(cv_round_f32(*v));
        }
    };
    let mut prev_dy = ytab.first().map_or(0, |t| t.di);
    for yt in &ytab {
        let row = &s[yt.si * swc..(yt.si + 1) * swc];
        buf.fill(0.0);
        for xt in &xtab {
            for c in 0..cn {
                buf[xt.di + c] += f32::from(row[xt.si + c]) * xt.alpha;
            }
        }
        if yt.di == prev_dy {
            for (s, b) in sum.iter_mut().zip(&buf) {
                *s += yt.alpha * b;
            }
        } else {
            store(&sum, &mut out, prev_dy);
            for (s, b) in sum.iter_mut().zip(&buf) {
                *s = yt.alpha * b;
            }
            prev_dy = yt.di;
        }
    }
    store(&sum, &mut out, prev_dy);
    Raster::new(dw, dh, src.channels(), out).expect("buffer sized for its dimensions")
}

/// `interpolateLanczos4`.
fn lanczos4_coeffs(x: f32) -> [f32; 8] {
    const S45: f64 = std::f64::consts::FRAC_1_SQRT_2;
    const CS: [[f64; 2]; 8] = [
        [1.0, 0.0],
        [-S45, -S45],
        [0.0, 1.0],
        [S45, -S45],
        [-1.0, 0.0],
        [S45, S45],
        [0.0, -1.0],
        [-S45, S45],
    ];
    let mut coeffs = [0f32; 8];
    let mut sum = 0f32;
    let y0 = -f64::from(x + 3.0) * std::f64::consts::PI * 0.25;
    let (s0, c0) = (y0.sin(), y0.cos());
    for (i, c) in coeffs.iter_mut().enumerate() {
        let y0_ = x + 3.0 - i as f32;
        *c = if y0_.abs() >= 1e-6 {
            let y = -f64::from(y0_) * std::f64::consts::PI * 0.25;
            ((CS[i][0] * s0 + CS[i][1] * c0) / (y * y)) as f32
        } else {
            1e30
        };
        sum += *c;
    }
    let sum = 1.0 / sum;
    for c in &mut coeffs {
        *c *= sum;
    }
    coeffs
}

/// The generic separable resize (`resizeGeneric_`) in fixed point for u8:
/// bilinear, the bilinear emulation of `INTER_AREA` when upscaling, or Lanczos4.
#[allow(clippy::too_many_arguments)]
#[allow(
    clippy::many_single_char_names,
    reason = "mirrors OpenCV's resize.cpp naming"
)]
fn generic(
    src: &Raster,
    dw: u32,
    dh: u32,
    interpolation: Interpolation,
    scale_x: f64,
    scale_y: f64,
    inv_x: f64,
    inv_y: f64,
) -> Raster {
    let cn = usize::from(src.channels());
    let (sw, sh) = (i64::from(src.width()), i64::from(src.height()));
    let (dwu, dhu) = (dw as usize, dh as usize);
    let area_mode = interpolation == Interpolation::Area;
    let lanczos = interpolation == Interpolation::Lanczos4;
    let ksize: usize = if lanczos { 8 } else { 2 };
    let ksize2 = (ksize / 2) as i64;

    let coeffs = |f: f32| -> Vec<i16> {
        let cbuf: Vec<f32> = if lanczos {
            lanczos4_coeffs(f).to_vec()
        } else {
            vec![1.0 - f, f]
        };
        cbuf.iter()
            .map(|c| sat_i16(cv_round_f32(c * COEF_SCALE as f32)))
            .collect()
    };
    let position = |d: usize, scale: f64, inv: f64| -> (i64, f32) {
        if area_mode {
            let s = (d as f64 * scale).floor() as i64;
            let f = ((d + 1) as f64 - (s + 1) as f64 * inv) as f32;
            (s, if f <= 0.0 { 0.0 } else { f - f.floor() })
        } else {
            let f = ((d as f64 + 0.5) * scale - 0.5) as f32;
            let s = f.floor() as i64;
            (s, f - s as f32)
        }
    };

    let mut xmin = 0usize;
    let mut xmax = dwu;
    let mut xofs = vec![0i64; dwu];
    let mut ialpha = vec![0i16; dwu * ksize];
    for dx in 0..dwu {
        let (mut sx, mut fx) = position(dx, scale_x, inv_x);
        if sx < ksize2 - 1 {
            xmin = dx + 1;
            if sx < 0 && !lanczos {
                fx = 0.0;
                sx = 0;
            }
        }
        if sx + ksize2 >= sw {
            xmax = xmax.min(dx);
            if sx >= sw - 1 && !lanczos {
                fx = 0.0;
                sx = sw - 1;
            }
        }
        xofs[dx] = sx;
        ialpha[dx * ksize..(dx + 1) * ksize].copy_from_slice(&coeffs(fx));
    }
    let mut yofs = vec![0i64; dhu];
    let mut ibeta = vec![0i16; dhu * ksize];
    for dy in 0..dhu {
        let (sy, fy) = position(dy, scale_y, inv_y);
        yofs[dy] = sy;
        ibeta[dy * ksize..(dy + 1) * ksize].copy_from_slice(&coeffs(fy));
    }

    let s = src.data();
    let swc = sw as usize * cn;
    let dwc = dwu * cn;
    // horizontal pass of one source row, in channel-expanded coordinates
    let hresize = |sy: usize| -> Vec<i32> {
        let row = &s[sy * swc..(sy + 1) * swc];
        let mut d = vec![0i32; dwc];
        for (dxc, out) in d.iter_mut().enumerate() {
            let (px, c) = (dxc / cn, dxc % cn);
            // Lanczos leaves source offsets unclamped (they may be negative at the edges)
            let sx_signed = xofs[px] * cn as i64 + c as i64;
            let sx = usize::try_from(sx_signed).unwrap_or(0);
            let a = &ialpha[px * ksize..(px + 1) * ksize];
            *out = if lanczos {
                if px < xmin || px >= xmax {
                    let base = sx_signed - cn as i64 * 3;
                    let mut v = 0i32;
                    for (j, &aj) in a.iter().enumerate() {
                        let mut sxj = base + (j * cn) as i64;
                        if sxj < 0 || sxj >= swc as i64 {
                            while sxj < 0 {
                                sxj += cn as i64;
                            }
                            while sxj >= swc as i64 {
                                sxj -= cn as i64;
                            }
                        }
                        v += i32::from(row[sxj as usize]) * i32::from(aj);
                    }
                    v
                } else {
                    (0..8)
                        .map(|j| i32::from(row[sx + j * cn - 3 * cn]) * i32::from(a[j]))
                        .sum()
                }
            } else if px < xmax {
                i32::from(row[sx]) * i32::from(a[0]) + i32::from(row[sx + cn]) * i32::from(a[1])
            } else {
                i32::from(row[sx]) * COEF_SCALE
            };
        }
        d
    };

    let mut out = vec![0u8; dwc * dhu];
    let mut cache: Vec<(usize, Vec<i32>)> = Vec::new();
    for dy in 0..dhu {
        let sy0 = yofs[dy];
        let rows: Vec<Vec<i32>> = (0..ksize as i64)
            .map(|k| {
                let sy = (sy0 - ksize2 + 1 + k).clamp(0, sh - 1) as usize;
                if let Some((_, r)) = cache.iter().find(|(y, _)| *y == sy) {
                    r.clone()
                } else {
                    let r = hresize(sy);
                    cache.push((sy, r.clone()));
                    if cache.len() > ksize * 2 {
                        cache.remove(0);
                    }
                    r
                }
            })
            .collect();
        let b = &ibeta[dy * ksize..(dy + 1) * ksize];
        let drow = &mut out[dy * dwc..(dy + 1) * dwc];
        for (x, d) in drow.iter_mut().enumerate() {
            *d = if lanczos {
                let v: i32 = (0..8).map(|k| rows[k][x] * i32::from(b[k])).sum();
                sat_u8((v + (1 << 21)) >> 22)
            } else {
                let v0 = (i32::from(b[0]) * (rows[0][x] >> 4)) >> 16;
                let v1 = (i32::from(b[1]) * (rows[1][x] >> 4)) >> 16;
                ((v0 + v1 + 2) >> 2) as u8
            };
        }
    }
    Raster::new(dw, dh, src.channels(), out).expect("buffer sized for its dimensions")
}

/// `cv2.cvtColor(img, cv2.COLOR_RGB2GRAY)` on the first three channels.
pub(crate) fn rgb_to_gray(src: &Raster) -> Raster {
    let cn = usize::from(src.channels());
    let data: Vec<u8> = src
        .data()
        .chunks_exact(cn)
        .map(|p| {
            let y = u32::from(p[0]) * 9798 + u32::from(p[1]) * 19235 + u32::from(p[2]) * 3735;
            ((y + (1 << 14)) >> 15) as u8
        })
        .collect();
    Raster::new(src.width(), src.height(), 1, data).expect("buffer sized for its dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32, c: u8) -> Raster {
        let data = (0..w * h * u32::from(c))
            .map(|i| (i * 37 % 251) as u8)
            .collect();
        Raster::new(w, h, c, data).unwrap()
    }

    #[test]
    fn identity_and_shapes() {
        let img = gradient(10, 6, 3);
        assert_eq!(resize(&img, 10, 6, Interpolation::Area), img);
        for interp in [
            Interpolation::Area,
            Interpolation::Linear,
            Interpolation::Lanczos4,
        ] {
            for (w, h) in [(5, 3), (7, 4), (20, 13), (1, 1), (3, 30)] {
                let r = resize(&img, w, h, interp);
                assert_eq!((r.width(), r.height(), r.channels()), (w, h, 3));
            }
        }
    }

    #[test]
    fn flat_images_stay_flat() {
        let img = Raster::new(9, 7, 1, vec![77; 63]).unwrap();
        for interp in [
            Interpolation::Area,
            Interpolation::Linear,
            Interpolation::Lanczos4,
        ] {
            for (w, h) in [(4, 3), (3, 3), (18, 14), (13, 5)] {
                assert!(
                    resize(&img, w, h, interp).data().iter().all(|&v| v == 77),
                    "{interp:?} {w}x{h}"
                );
            }
        }
    }

    #[test]
    fn grey_weights() {
        let img = Raster::new(3, 1, 3, vec![255, 0, 0, 0, 255, 0, 0, 0, 255]).unwrap();
        // 0.299, 0.587, 0.114 in Q15 with rounding
        assert_eq!(rgb_to_gray(&img).data(), &[76, 150, 29]);
    }
}
