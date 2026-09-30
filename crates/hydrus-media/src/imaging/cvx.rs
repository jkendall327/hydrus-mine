//! More of OpenCV 4.13, for the visual-duplicates comparison
//! (`ClientVisualData`): Gaussian blur (8-bit, bit-exact; float, in the
//! order OpenCV's AVX2/FMA build computes it), 8-bit RGB -> L*a*b*
//! (bit-exact), and `INTER_AREA` resizes of float images.
//!
//! Each follows the OpenCV source it names; the tests in
//! `crates/hydrus-media/tests/visual_data.rs` check them against `cv2`.

// Constants are OpenCV's float bit patterns, written as its source has them.
#![allow(clippy::unreadable_literal)]

use std::sync::OnceLock;

/// `cvRound` for doubles: round half to even.
fn cv_round(v: f64) -> i64 {
    v.round_ties_even() as i64
}

/// `borderInterpolate(p, len, BORDER_REFLECT_101)`.
fn reflect101(mut p: i64, len: i64) -> usize {
    if len == 1 {
        return 0;
    }
    while p < 0 || p >= len {
        if p < 0 {
            p = -p;
        } else {
            p = len - 1 - (p - len) - 1;
        }
    }
    p as usize
}

/// `getGaussianKernelBitExact`: the normalised kernel in doubles.
fn gaussian_kernel(n: usize, sigma: f64) -> Vec<f64> {
    let n_i = n as i64;
    let sigma = if sigma > 0.0 {
        sigma
    } else {
        (n as f64).mul_add(0.15, 0.35)
    };
    let scale2x = -0.125 / (sigma * sigma);
    let n2 = (n - 1) / 2;
    let mut values = Vec::with_capacity(n2);
    let mut sum = 0.0f64;
    let mut x = 1 - n_i;
    for _ in 0..n2 {
        let t = ((x * x) as f64 * scale2x).exp();
        values.push(t);
        sum += t;
        x += 2;
    }
    sum *= 2.0;
    sum += 1.0;
    if n.is_multiple_of(2) {
        sum += 1.0;
    }
    let mul = 1.0 / sum;
    let mut out = vec![0.0; n];
    for (i, v) in values.iter().enumerate() {
        out[i] = v * mul;
        out[n - 1 - i] = out[i];
    }
    out[n2] = mul;
    if n.is_multiple_of(2) {
        out[n2 + 1] = out[n2];
    }
    out
}

/// `getGaussianKernelFixedPoint_ED` with 8 fraction bits (`ufixedpoint16`).
fn gaussian_kernel_fixed(n: usize, sigma: f64) -> Vec<u32> {
    let k = gaussian_kernel(n, sigma);
    let mut out = vec![0u32; n];
    let n2 = n / 2;
    let mut err = 0.0f64;
    let mut sum = 0i64;
    for i in 0..n2 {
        let adj = k[i] * 256.0 + err;
        let v = cv_round(adj);
        err = adj - v as f64;
        out[i] = v as u32;
        out[n - 1 - i] = v as u32;
        sum += v;
    }
    out[n2] = (256 - 2 * sum) as u32;
    out
}

/// The kernel size `GaussianBlur` picks for a sigma (`createGaussianKernels`).
fn gaussian_ksize(sigma: f64, eight_bit: bool) -> usize {
    let per_side = if eight_bit { 3.0 } else { 4.0 };
    (cv_round(sigma * per_side * 2.0 + 1.0) | 1) as usize
}

/// `cv2.GaussianBlur(plane, (0, 0), sigmaX=sigma)` on one 8-bit channel
/// (the bit-exact fixed-point implementation OpenCV uses for 8-bit images).
pub fn gaussian_blur_u8(src: &[u8], w: usize, h: usize, sigma: f64) -> Vec<u8> {
    let ksize = gaussian_ksize(sigma, true);
    let kx_len = if w == 1 { 1 } else { ksize };
    let ky_len = if h == 1 { 1 } else { ksize };
    if kx_len == 1 && ky_len == 1 {
        return src.to_vec();
    }
    let kx = gaussian_kernel_fixed(kx_len, sigma);
    let ky = gaussian_kernel_fixed(ky_len, sigma);
    let (rx, ry) = ((kx_len / 2) as i64, (ky_len / 2) as i64);
    // rows: 8.8 fixed point
    let mut rows = vec![0u32; w * h];
    for y in 0..h {
        let line = &src[y * w..(y + 1) * w];
        for x in 0..w {
            let mut v = 0u32;
            for (j, &k) in kx.iter().enumerate() {
                let sx = reflect101(x as i64 + j as i64 - rx, w as i64);
                v += k * u32::from(line[sx]);
            }
            rows[y * w + x] = v;
        }
    }
    // columns: 16.16 fixed point, rounded
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut v = 0u64;
            for (j, &k) in ky.iter().enumerate() {
                let sy = reflect101(y as i64 + j as i64 - ry, h as i64);
                v += u64::from(k) * u64::from(rows[sy * w + x]);
            }
            out[y * w + x] = ((v + (1 << 15)) >> 16).min(255) as u8;
        }
    }
    out
}

/// `cv2.GaussianBlur(plane, (0, 0), sigmaX=sigma)` on one float channel,
/// with the separable filter's arithmetic in the order OpenCV's AVX2/FMA
/// build does it (`RowVec_32f`, `SymmColumnVec_32f`).
pub fn gaussian_blur_f32(src: &[f32], w: usize, h: usize, sigma: f64) -> Vec<f32> {
    let ksize = gaussian_ksize(sigma, false);
    let kx_len = if w == 1 { 1 } else { ksize };
    let ky_len = if h == 1 { 1 } else { ksize };
    if kx_len == 1 && ky_len == 1 {
        return src.to_vec();
    }
    let kx: Vec<f32> = gaussian_kernel(kx_len, sigma)
        .iter()
        .map(|&v| v as f32)
        .collect();
    let ky: Vec<f32> = gaussian_kernel(ky_len, sigma)
        .iter()
        .map(|&v| v as f32)
        .collect();
    // Without hardware FMA each `mul_add` is a libm call, and a 2048x2048
    // edge map takes seconds.
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("fma") && std::arch::is_x86_feature_detected!("avx2") {
        // SAFETY: the CPU has the features the function is compiled for
        #[allow(unsafe_code)]
        return unsafe { blur_f32_fma(src, w, h, &kx, &ky) };
    }
    blur_f32(src, w, h, &kx, &ky)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
fn blur_f32_fma(src: &[f32], w: usize, h: usize, kx: &[f32], ky: &[f32]) -> Vec<f32> {
    blur_f32(src, w, h, kx, ky)
}

/// The separable float filter. Each output is accumulated in OpenCV's
/// order; the loops run along rows so they vectorise.
// always inlined, so the FMA variant compiles its own copy
#[allow(clippy::inline_always)]
#[inline(always)]
fn blur_f32(src: &[f32], w: usize, h: usize, kx: &[f32], ky: &[f32]) -> Vec<f32> {
    // Which columns fuse their multiply-adds, as OpenCV's build does: the
    // vector loops (8 lanes at a time) do; after them the row filter's
    // scalar loop unrolled 4 at a time does too, and the rest does not.
    let vector = w / 8 * 8;
    let rv = if w % 8 >= 4 { vector + 4 } else { vector };
    let cv = vector;
    let rx = kx.len() / 2;
    let mut rows = vec![0f32; w * h];
    let mut padded = vec![0f32; w + 2 * rx];
    for y in 0..h {
        let line = &src[y * w..(y + 1) * w];
        for (i, p) in padded.iter_mut().enumerate() {
            *p = line[reflect101(i as i64 - rx as i64, w as i64)];
        }
        let out = &mut rows[y * w..(y + 1) * w];
        for (o, &v) in out.iter_mut().zip(&padded) {
            *o = v * kx[0];
        }
        for (j, &k) in kx.iter().enumerate().skip(1) {
            let (head, tail) = out.split_at_mut(rv);
            for (o, &v) in head.iter_mut().zip(&padded[j..]) {
                *o = v.mul_add(k, *o);
            }
            for (o, &v) in tail.iter_mut().zip(&padded[j + rv..]) {
                *o += v * k;
            }
        }
    }
    let c = ky.len() / 2;
    let mut out = vec![0f32; w * h];
    let row = |y: i64| {
        let r = reflect101(y, h as i64);
        &rows[r * w..(r + 1) * w]
    };
    for y in 0..h {
        let o = &mut out[y * w..(y + 1) * w];
        for (o, &v) in o.iter_mut().zip(row(y as i64)) {
            *o = v * ky[c];
        }
        for k in 1..=c {
            let (up, down) = (row(y as i64 + k as i64), row(y as i64 - k as i64));
            let kk = ky[c + k];
            let (head, tail) = o.split_at_mut(cv);
            for ((o, &u), &d) in head.iter_mut().zip(up).zip(down) {
                *o = (u + d).mul_add(kk, *o);
            }
            for ((o, &u), &d) in tail.iter_mut().zip(&up[cv..]).zip(&down[cv..]) {
                *o += kk * (u + d);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// RGB -> L*a*b* (`RGB2Lab_b`)

struct LabTables {
    gamma: [u16; 256],
    cbrt: Vec<u16>,
    coeffs: [i32; 9],
}

const LAB_SHIFT: i32 = 12;
const LAB_SHIFT2: i32 = 15;

fn lab_tables() -> &'static LabTables {
    static TABLES: OnceLock<LabTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let apply_gamma = |x: f32| -> f32 {
            let xd = f64::from(x);
            let v = if xd <= 809.0 / 20000.0 {
                xd / (323.0 / 25.0)
            } else {
                ((xd + 11.0 / 200.0) / (1.0 + 11.0 / 200.0)).powf(12.0 / 5.0)
            };
            v as f32
        };
        let mut gamma = [0u16; 256];
        for (i, g) in gamma.iter_mut().enumerate() {
            let x = i as f32 / 255.0f32;
            *g = (2040.0f32 * apply_gamma(x)).round_ties_even() as u16;
        }
        let lthresh = 216.0f32 / 24389.0f32;
        let lscale = 841.0f32 / 108.0f32;
        let lbias = 16.0f32 / 116.0f32;
        let cb_scale = 1.0f32 / 2040.0f32;
        let cbrt = (0..256 * 3 / 2 * 8)
            .map(|i| {
                let x = cb_scale * i as f32;
                let v = if x < lthresh {
                    x.mul_add(lscale, lbias)
                } else {
                    soft_cbrt(x)
                };
                (32768.0f32 * v).round_ties_even() as u16
            })
            .collect();
        // sRGB -> XYZ (D65), divided by the white point, in 4.12 fixed point
        let srgb2xyz: [f64; 9] = [
            f64::from_bits(0x3fda65a14488c60d),
            f64::from_bits(0x3fd6e297396d0918),
            f64::from_bits(0x3fc71819d2391d58),
            f64::from_bits(0x3fcb38cda6e75ff6),
            f64::from_bits(0x3fe6e297396d0918),
            f64::from_bits(0x3fb279aae6c8f755),
            f64::from_bits(0x3f93cc4ac6cdaf4b),
            f64::from_bits(0x3fbe836eb4e98138),
            f64::from_bits(0x3fee68427418d691),
        ];
        let white = [
            f64::from_bits(0x3fee6a22b3892ee8),
            1.0,
            f64::from_bits(0x3ff16b8950763a19),
        ];
        let mut coeffs = [0i32; 9];
        for i in 0..3 {
            for j in 0..3 {
                coeffs[i * 3 + j] = cv_round(4096.0 * srgb2xyz[i * 3 + j] / white[i]) as i32;
            }
        }
        LabTables {
            gamma,
            cbrt,
            coeffs,
        }
    })
}

/// OpenCV's softfloat `cbrt` (Turkowski's): a rational approximation on the
/// mantissa, whose result is truncated rather than rounded. Positive
/// normal inputs only.
fn soft_cbrt(x: f32) -> f32 {
    if x == 0.0 {
        return 0.0;
    }
    let bits = x.to_bits();
    let ex = ((bits >> 23) & 0xff) as i32 - 127;
    let mut shx = ex % 3;
    shx -= if shx >= 0 { 3 } else { 0 };
    let ex = (ex - shx) / 3 - 1;
    let fr = f64::from_bits((((shx + 1023) as u64) << 52) | (u64::from(bits & 0x7f_ffff) << 29));
    let a = [
        0x4046a09e6653ba70u64,
        0x406808f46c6116e0,
        0x405dca97439cae14,
        0x402add70d2827500,
        0x3fc4f15f83f55d2d,
        0x402d9e20660edb21,
        0x4062ff15c0285815,
        0x406510d06a8112ce,
        0x4040fecbc9e2c375,
    ]
    .map(f64::from_bits);
    let num = (((a[0] * fr + a[1]) * fr + a[2]) * fr + a[3]) * fr + a[4];
    let den = (((a[5] * fr + a[6]) * fr + a[7]) * fr + a[8]) * fr + 1.0;
    let fr = num / den;
    let mantissa = ((fr.to_bits() & 0x000f_ffff_ffff_ffff) >> 29) as u32;
    f32::from_bits(((((ex + 127) as u32) & 0xff) << 23) | mantissa)
}

fn descale(x: i32, n: i32) -> i32 {
    (x + (1 << (n - 1))) >> n
}

/// `cv2.cvtColor(rgb, cv2.COLOR_RGB2Lab)` on 8-bit RGB (3 channels,
/// interleaved): 8-bit L, a, b interleaved.
#[allow(clippy::many_single_char_names)]
pub fn rgb_to_lab(rgb: &[u8]) -> Vec<u8> {
    let t = lab_tables();
    let c = &t.coeffs;
    let lscale = (116 * 255 + 50) / 100;
    let lshift = -((16 * 255 * (1 << LAB_SHIFT2) + 50) / 100);
    let mut out = vec![0u8; rgb.len()];
    for (px, o) in rgb.chunks_exact(3).zip(out.chunks_exact_mut(3)) {
        let (r, g, b) = (
            i32::from(t.gamma[usize::from(px[0])]),
            i32::from(t.gamma[usize::from(px[1])]),
            i32::from(t.gamma[usize::from(px[2])]),
        );
        let cb = |v: i32| i32::from(t.cbrt[descale(v, LAB_SHIFT) as usize]);
        let fx = cb(r * c[0] + g * c[1] + b * c[2]);
        let fy = cb(r * c[3] + g * c[4] + b * c[5]);
        let fz = cb(r * c[6] + g * c[7] + b * c[8]);
        let l = descale(lscale * fy + lshift, LAB_SHIFT2);
        let a = descale(500 * (fx - fy) + 128 * (1 << LAB_SHIFT2), LAB_SHIFT2);
        let bb = descale(200 * (fy - fz) + 128 * (1 << LAB_SHIFT2), LAB_SHIFT2);
        o[0] = l.clamp(0, 255) as u8;
        o[1] = a.clamp(0, 255) as u8;
        o[2] = bb.clamp(0, 255) as u8;
    }
    out
}

// ---------------------------------------------------------------------------
// INTER_AREA on floats

/// `cv2.resize(image, (dw, dh), interpolation=cv2.INTER_AREA)` on an
/// interleaved float image of `cn` channels.
#[allow(clippy::many_single_char_names)]
pub fn resize_area_f32(
    src: &[f32],
    sw: usize,
    sh: usize,
    cn: usize,
    dw: usize,
    dh: usize,
) -> Vec<f32> {
    if (dw, dh) == (sw, sh) {
        return src.to_vec();
    }
    let inv_x = dw as f64 / sw as f64;
    let inv_y = dh as f64 / sh as f64;
    let scale_x = 1.0 / inv_x;
    let scale_y = 1.0 / inv_y;
    let iscale_x = cv_round(scale_x);
    let iscale_y = cv_round(scale_y);
    let is_area_fast = (scale_x - iscale_x as f64).abs() < f64::EPSILON
        && (scale_y - iscale_y as f64).abs() < f64::EPSILON;
    if scale_x >= 1.0 && scale_y >= 1.0 {
        if is_area_fast {
            return area_fast_f32(
                src,
                sw,
                sh,
                cn,
                dw,
                dh,
                iscale_x as usize,
                iscale_y as usize,
            );
        }
        return area_generic_f32(src, sw, sh, cn, dw, dh, scale_x, scale_y);
    }
    linear_area_f32(src, sw, sh, cn, dw, dh, scale_x, scale_y, inv_x, inv_y)
}

/// `resizeAreaFast_<float, float>` (the vectorised 2x2 case is only for 1
/// or 4 channels; others sum in the scalar order).
#[allow(clippy::too_many_arguments, clippy::many_single_char_names)]
fn area_fast_f32(
    src: &[f32],
    sw: usize,
    sh: usize,
    cn: usize,
    dw: usize,
    dh: usize,
    sx: usize,
    sy: usize,
) -> Vec<f32> {
    let swc = sw * cn;
    let dwc = dw * cn;
    let area = sx * sy;
    let scale = 1.0f32 / area as f32;
    let dwidth1 = (sw / sx) * cn;
    let ofs: Vec<usize> = (0..sy)
        .flat_map(|a| (0..sx).map(move |b| a * swc + b * cn))
        .collect();
    let xofs = |dx: usize| (dx / cn) * sx * cn + dx % cn;
    let vector = sx == 2 && sy == 2 && (cn == 1 || cn == 4);
    let mut out = vec![0f32; dwc * dh];
    for dy in 0..dh {
        let row = &mut out[dy * dwc..(dy + 1) * dwc];
        let sy0 = dy * sy;
        if sy0 >= sh {
            continue;
        }
        let w = if sy0 + sy <= sh { dwidth1 } else { 0 };
        let base = sy0 * swc;
        for (dx, d) in row.iter_mut().enumerate().take(w) {
            let s = base + xofs(dx);
            if vector {
                let (a, b) = (src[s], src[s + cn]);
                let (c, e) = (src[s + swc], src[s + swc + cn]);
                *d = ((a + b) + (c + e)) * 0.25;
                continue;
            }
            let mut sum = 0f32;
            let mut k = 0;
            while k + 4 <= area {
                sum += src[s + ofs[k]]
                    + src[s + ofs[k + 1]]
                    + src[s + ofs[k + 2]]
                    + src[s + ofs[k + 3]];
                k += 4;
            }
            while k < area {
                sum += src[s + ofs[k]];
                k += 1;
            }
            *d = sum * scale;
        }
        for (dx, d) in row.iter_mut().enumerate().skip(w) {
            let sx0 = xofs(dx);
            let mut sum = 0f32;
            let mut count = 0i32;
            for a in 0..sy {
                if sy0 + a >= sh {
                    break;
                }
                for b in (0..sx * cn).step_by(cn) {
                    if sx0 + b >= swc {
                        break;
                    }
                    sum += src[(sy0 + a) * swc + sx0 + b];
                    count += 1;
                }
            }
            *d = sum / count as f32;
        }
    }
    out
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

/// `resizeArea_<float, float>`.
#[allow(clippy::too_many_arguments)]
fn area_generic_f32(
    src: &[f32],
    sw: usize,
    sh: usize,
    cn: usize,
    dw: usize,
    dh: usize,
    scale_x: f64,
    scale_y: f64,
) -> Vec<f32> {
    let swc = sw * cn;
    let dwc = dw * cn;
    let xtab = area_tab(sw, dw, cn, scale_x);
    let ytab = area_tab(sh, dh, 1, scale_y);
    let mut out = vec![0f32; dwc * dh];
    let mut buf = vec![0f32; dwc];
    let mut sum = vec![0f32; dwc];
    let mut prev_dy = ytab.first().map_or(0, |t| t.di);
    for yt in &ytab {
        let row = &src[yt.si * swc..(yt.si + 1) * swc];
        buf.fill(0.0);
        for xt in &xtab {
            for c in 0..cn {
                buf[xt.di + c] += row[xt.si + c] * xt.alpha;
            }
        }
        if yt.di == prev_dy {
            for (s, b) in sum.iter_mut().zip(&buf) {
                *s += yt.alpha * b;
            }
        } else {
            out[prev_dy * dwc..(prev_dy + 1) * dwc].copy_from_slice(&sum);
            for (s, b) in sum.iter_mut().zip(&buf) {
                *s = yt.alpha * b;
            }
            prev_dy = yt.di;
        }
    }
    out[prev_dy * dwc..(prev_dy + 1) * dwc].copy_from_slice(&sum);
    out
}

/// `resizeGeneric_` with `HResizeLinear`/`VResizeLinear` on floats, with the
/// `INTER_AREA` upscaling coefficients.
#[allow(clippy::too_many_arguments)]
fn linear_area_f32(
    src: &[f32],
    sw: usize,
    sh: usize,
    cn: usize,
    dw: usize,
    dh: usize,
    scale_x: f64,
    scale_y: f64,
    inv_x: f64,
    inv_y: f64,
) -> Vec<f32> {
    let position = |d: usize, scale: f64, inv: f64| -> (i64, f32) {
        let s = (d as f64 * scale).floor() as i64;
        let f = ((d + 1) as f64 - (s + 1) as f64 * inv) as f32;
        (s, if f <= 0.0 { 0.0 } else { f - f.floor() })
    };
    let (swi, shi) = (sw as i64, sh as i64);
    let mut xmax = dw;
    let mut xofs = vec![0usize; dw];
    let mut alpha = vec![[0f32; 2]; dw];
    for dx in 0..dw {
        let (mut sx, mut fx) = position(dx, scale_x, inv_x);
        if sx < 0 {
            fx = 0.0;
            sx = 0;
        }
        if sx + 1 >= swi {
            xmax = xmax.min(dx);
            if sx >= swi - 1 {
                fx = 0.0;
                sx = swi - 1;
            }
        }
        xofs[dx] = sx as usize;
        alpha[dx] = [1.0 - fx, fx];
    }
    let swc = sw * cn;
    let dwc = dw * cn;
    let hresize = |sy: usize| -> Vec<f32> {
        let row = &src[sy * swc..(sy + 1) * swc];
        let mut d = vec![0f32; dwc];
        for (dxc, out) in d.iter_mut().enumerate() {
            let (px, c) = (dxc / cn, dxc % cn);
            let sx = xofs[px] * cn + c;
            *out = if px < xmax {
                row[sx] * alpha[px][0] + row[sx + cn] * alpha[px][1]
            } else {
                row[sx]
            };
        }
        d
    };
    let mut out = vec![0f32; dwc * dh];
    for dy in 0..dh {
        let (sy, fy) = position(dy, scale_y, inv_y);
        let (b0, b1) = (1.0 - fy, fy);
        let r0 = hresize(sy.clamp(0, shi - 1) as usize);
        let r1 = hresize((sy + 1).clamp(0, shi - 1) as usize);
        for (x, d) in out[dy * dwc..(dy + 1) * dwc].iter_mut().enumerate() {
            *d = r0[x] * b0 + r1[x] * b1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernels_sum_to_one() {
        let k = gaussian_kernel_fixed(7, 0.95);
        assert_eq!(k.iter().sum::<u32>(), 256);
        assert_eq!(gaussian_ksize(0.95, true), 7);
        assert_eq!(gaussian_ksize(10.0, false), 81);
    }

    #[test]
    fn reflect_101() {
        assert_eq!(reflect101(-1, 5), 1);
        assert_eq!(reflect101(5, 5), 3);
        assert_eq!(reflect101(-7, 3), 1);
        assert_eq!(reflect101(3, 1), 0);
    }
}
