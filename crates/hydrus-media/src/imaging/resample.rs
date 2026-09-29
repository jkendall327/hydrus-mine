//! Pillow's `Image.resize(size, Resampling.LANCZOS)` (libImaging/Resample.c),
//! which the reference uses for default thumbnails (`ImageOps.pad`) and for
//! the embedded previews of Krita, OpenRaster, Paint.NET, PowerPoint and
//! Ugoira files.
//!
//! A separable two-pass convolution: filter weights in f64, normalised to
//! 22-bit fixed point, accumulated in integers. Images with alpha are
//! premultiplied first and un-premultiplied afterwards, exactly as Pillow's
//! `RGBA -> RGBa -> RGBA` round trip does. Bit-exact with Pillow 12.

use super::Raster;

const PRECISION_BITS: u32 = 32 - 8 - 2;
const SUPPORT: f64 = 3.0;

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let x = x * std::f64::consts::PI;
    x.sin() / x
}

fn lanczos(x: f64) -> f64 {
    if (-SUPPORT..SUPPORT).contains(&x) {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

/// Per output position: first source index and fixed-point weights.
struct Coeffs {
    bounds: Vec<(usize, usize)>,
    ksize: usize,
    weights: Vec<i32>,
}

/// `precompute_coeffs` + `normalize_coeffs_8bpc` for the default box.
fn coeffs(in_size: u32, out_size: u32) -> Coeffs {
    // `(double)(in1 - in0) / outSize`, with the box edges as C floats
    let scale = f64::from(in_size as f32) / f64::from(out_size);
    let filterscale = scale.max(1.0);
    let support = SUPPORT * filterscale;
    let ksize = support.ceil() as usize * 2 + 1;
    let inv_filterscale = 1.0 / filterscale;
    let mut bounds = Vec::with_capacity(out_size as usize);
    let mut weights = vec![0i32; out_size as usize * ksize];
    for xx in 0..out_size as usize {
        let center = (xx as f64 + 0.5) * scale;
        // C's (int) cast truncates toward zero
        let xmin = ((center - support + 0.5) as i64).max(0);
        let xmax = ((center + support + 0.5) as i64).min(i64::from(in_size)) - xmin;
        let count = xmax.max(0) as usize;
        let mut k: Vec<f64> = (0..count)
            .map(|x| lanczos(((x as i64 + xmin) as f64 - center + 0.5) * inv_filterscale))
            .collect();
        let total: f64 = k.iter().sum();
        if total != 0.0 {
            for w in &mut k {
                *w /= total;
            }
        }
        let scale_fixed = f64::from(1u32 << PRECISION_BITS);
        for (slot, w) in weights[xx * ksize..].iter_mut().zip(&k) {
            *slot = if *w < 0.0 {
                (-0.5 + w * scale_fixed) as i32
            } else {
                (0.5 + w * scale_fixed) as i32
            };
        }
        bounds.push((xmin as usize, count));
    }
    Coeffs {
        bounds,
        ksize,
        weights,
    }
}

/// `clip8`: back from fixed point, saturating.
fn clip8(v: i64) -> u8 {
    (v >> PRECISION_BITS).clamp(0, 255) as u8
}

const ROUNDING: i64 = 1 << (PRECISION_BITS - 1);

fn horizontal(src: &[u8], width: usize, height: usize, cn: usize, c: &Coeffs) -> Vec<u8> {
    let out_w = c.bounds.len();
    let mut out = vec![0u8; out_w * height * cn];
    for y in 0..height {
        let row = &src[y * width * cn..(y + 1) * width * cn];
        let dst = &mut out[y * out_w * cn..(y + 1) * out_w * cn];
        for (xx, &(xmin, count)) in c.bounds.iter().enumerate() {
            let k = &c.weights[xx * c.ksize..xx * c.ksize + count];
            for ch in 0..cn {
                let sum: i64 = k
                    .iter()
                    .enumerate()
                    .map(|(x, &w)| i64::from(row[(x + xmin) * cn + ch]) * i64::from(w))
                    .sum();
                dst[xx * cn + ch] = clip8(ROUNDING + sum);
            }
        }
    }
    out
}

fn vertical(src: &[u8], width: usize, cn: usize, c: &Coeffs) -> Vec<u8> {
    let stride = width * cn;
    let mut out = vec![0u8; c.bounds.len() * stride];
    for (yy, &(ymin, count)) in c.bounds.iter().enumerate() {
        let k = &c.weights[yy * c.ksize..yy * c.ksize + count];
        let dst = &mut out[yy * stride..(yy + 1) * stride];
        for (i, d) in dst.iter_mut().enumerate() {
            let sum: i64 = k
                .iter()
                .enumerate()
                .map(|(y, &w)| i64::from(src[(y + ymin) * stride + i]) * i64::from(w))
                .sum();
            *d = clip8(ROUNDING + sum);
        }
    }
    out
}

/// `MULDIV255`.
fn muldiv255(a: u8, b: u8) -> u8 {
    let t = u32::from(a) * u32::from(b) + 128;
    (((t >> 8) + t) >> 8) as u8
}

/// `RGBA -> RGBa` / `LA -> La`.
fn premultiply(data: &mut [u8], cn: usize) {
    for px in data.chunks_exact_mut(cn) {
        let (colour, alpha) = px.split_at_mut(cn - 1);
        for v in colour {
            *v = muldiv255(*v, alpha[0]);
        }
    }
}

/// `RGBa -> RGBA` / `La -> LA`.
fn unpremultiply(data: &mut [u8], cn: usize) {
    for px in data.chunks_exact_mut(cn) {
        let (colour, alpha) = px.split_at_mut(cn - 1);
        let a = u32::from(alpha[0]);
        if a != 0 && a != 255 {
            for v in colour {
                *v = (255 * u32::from(*v) / a).min(255) as u8;
            }
        }
    }
}

/// Pillow's LANCZOS resize of an L, LA, RGB or RGBA image.
pub(crate) fn resize_lanczos(image: &Raster, width: u32, height: u32) -> Raster {
    let (iw, ih) = (image.width(), image.height());
    if (iw, ih) == (width, height) {
        return image.clone();
    }
    let cn = usize::from(image.channels());
    let alpha = image.has_alpha_channel();
    let mut data = image.data().to_vec();
    if alpha {
        premultiply(&mut data, cn);
    }
    let (cur_w, cur_h) = (iw as usize, ih as usize);
    let vert = coeffs(ih, height);
    if width == iw {
        data = vertical(&data, cur_w, cn, &vert);
    } else {
        // only the source rows the vertical pass reads
        let first = vert.bounds.first().map_or(0, |b| b.0);
        let last = vert.bounds.last().map_or(0, |b| b.0 + b.1);
        let rows = &data[first * cur_w * cn..last * cur_w * cn];
        data = horizontal(rows, cur_w, last - first, cn, &coeffs(iw, width));
        if height == ih {
            // no vertical pass: the row window was the whole image
            debug_assert_eq!((first, last), (0, cur_h));
        } else {
            let shifted = Coeffs {
                bounds: vert.bounds.iter().map(|&(m, n)| (m - first, n)).collect(),
                ..vert
            };
            data = vertical(&data, width as usize, cn, &shifted);
        }
    }
    if alpha {
        unpremultiply(&mut data, cn);
    }
    Raster::new(width, height, image.channels(), data).expect("sized from the resize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefficients_sum_to_one_in_fixed_point() {
        let c = coeffs(1000, 150);
        for (i, &(_, n)) in c.bounds.iter().enumerate() {
            let s: i64 = c.weights[i * c.ksize..i * c.ksize + n]
                .iter()
                .map(|&w| i64::from(w))
                .sum();
            assert!((s - (1 << PRECISION_BITS)).abs() <= n as i64, "{s}");
        }
    }

    #[test]
    fn flat_images_stay_flat() {
        let img = Raster::new(40, 30, 4, [10, 200, 30, 128].repeat(1200)).unwrap();
        let out = resize_lanczos(&img, 13, 7);
        assert_eq!((out.width(), out.height()), (13, 7));
        for px in out.data().chunks_exact(4) {
            assert_eq!(px[3], 128);
            // premultiplied rounding moves colour a little, as in Pillow
            assert!(
                px[0].abs_diff(10) <= 1 && px[1].abs_diff(200) <= 1 && px[2].abs_diff(30) <= 1,
                "{px:?}"
            );
        }
    }
}
