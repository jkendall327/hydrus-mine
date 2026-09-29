//! Colour normalisation to sRGB with LittleCMS, as Pillow's
//! `ImageCms.profileToProfile` does it for the reference
//! (`NormaliseICCProfilePILImageToSRGB`).
//!
//! Pillow builds `cmsCreateTransform(src, TYPE_RGBA_8|TYPE_CMYK_8, sRGB,
//! TYPE_RGBA_8, INTENT_PERCEPTUAL, 0)` and copies the alpha channel across;
//! we do the same with the same LittleCMS version, so results are identical.

use lcms2::{Intent, PixelFormat, Profile, Transform};

use super::pil::{Mode, PilImage, Pixels};

/// Convert an RGB/RGBA/CMYK image to sRGB using `profile`.
///
/// Returns `None` when Pillow would leave the image untouched: modes it
/// cannot transform (L, LA, P, ...), unreadable profiles, or profiles whose
/// colour space does not match the image.
pub(crate) fn to_srgb(img: &PilImage, profile: &[u8]) -> Option<PilImage> {
    let in_format = match img.mode {
        Mode::Rgb | Mode::Rgba => PixelFormat::RGBA_8,
        Mode::Cmyk => PixelFormat::CMYK_8,
        _ => return None,
    };
    let Ok(src) = Profile::new_icc(profile) else {
        return None;
    };
    let dst = Profile::new_srgb();
    let Ok(transform) = Transform::<[u8; 4], [u8; 4]>::new(
        &src,
        in_format,
        &dst,
        PixelFormat::RGBA_8,
        Intent::Perceptual,
    ) else {
        return None;
    };
    let Pixels::U8(data) = &img.pixels else {
        return None;
    };
    // Pillow holds RGB as RGBX with X=255, and copies extra channels across
    let input: Vec<[u8; 4]> = match img.mode {
        Mode::Rgb => data
            .chunks_exact(3)
            .map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => data
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect(),
    };
    let mut output = vec![[0u8; 4]; input.len()];
    transform.transform_pixels(&input, &mut output);
    let keep_alpha = img.has_transparency();
    let out_data: Vec<u8> = if keep_alpha && img.mode == Mode::Rgba {
        output
            .iter()
            .zip(&input)
            .flat_map(|(o, i)| [o[0], o[1], o[2], i[3]])
            .collect()
    } else {
        output.iter().flat_map(|o| [o[0], o[1], o[2]]).collect()
    };
    let mut result = img.clone();
    result.mode = if keep_alpha && img.mode == Mode::Rgba {
        Mode::Rgba
    } else {
        Mode::Rgb
    };
    result.pixels = Pixels::U8(out_data);
    Some(result)
}

// --- building an ICC profile from PNG gAMA/cHRM (HydrusImageICCProfiles) ---

type Mat3 = [[f64; 3]; 3];

fn xy_to_xyz(x: f64, y: f64) -> [f64; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// `numpy.linalg.solve` for 3x3 (LU with partial pivoting, as LAPACK does).
#[allow(
    clippy::many_single_char_names,
    reason = "matrix maths, named as in the reference"
)]
fn solve(m: &Mat3, b: [f64; 3]) -> [f64; 3] {
    let mut a = *m;
    let mut rhs = b;
    for col in 0..3 {
        let pivot = (col..3)
            .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
            .unwrap_or(col);
        a.swap(col, pivot);
        rhs.swap(col, pivot);
        for row in col + 1..3 {
            let f = a[row][col] / a[col][col];
            let pivot_row = a[col];
            for (v, p) in a[row].iter_mut().zip(pivot_row).skip(col) {
                *v -= f * p;
            }
            rhs[row] -= f * rhs[col];
        }
    }
    let mut x = [0.0; 3];
    for row in (0..3).rev() {
        let mut s = rhs[row];
        for k in row + 1..3 {
            s -= a[row][k] * x[k];
        }
        x[row] = s / a[row][row];
    }
    x
}

fn inverse(m: &Mat3) -> Mat3 {
    let cols: Vec<[f64; 3]> = (0..3)
        .map(|i| {
            let mut e = [0.0; 3];
            e[i] = 1.0;
            solve(m, e)
        })
        .collect();
    let mut out = [[0.0; 3]; 3];
    for (j, col) in cols.iter().enumerate() {
        for i in 0..3 {
            out[i][j] = col[i];
        }
    }
    out
}

fn matmul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            out[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

fn matvec(a: &Mat3, v: [f64; 3]) -> [f64; 3] {
    [
        a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2],
        a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2],
        a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2],
    ]
}

/// `chromaticities_to_rgb_to_xyz`.
#[allow(
    clippy::many_single_char_names,
    reason = "matrix maths, named as in the reference"
)]
fn rgb_to_xyz(white: [f64; 2], red: [f64; 2], green: [f64; 2], blue: [f64; 2]) -> Mat3 {
    let w = xy_to_xyz(white[0], white[1]);
    let r = xy_to_xyz(red[0], red[1]);
    let g = xy_to_xyz(green[0], green[1]);
    let b = xy_to_xyz(blue[0], blue[1]);
    let m = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
    let s = solve(&m, w);
    let mut out = m;
    for row in &mut out {
        for (v, scale) in row.iter_mut().zip(s) {
            *v *= scale;
        }
    }
    out
}

/// `generate_bradford_adaptation_matrix` to D50.
fn bradford_to_d50(src: [f64; 2]) -> Mat3 {
    let m: Mat3 = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    let m_inv = inverse(&m);
    let src_cone = matvec(&m, xy_to_xyz(src[0], src[1]));
    let dst_cone = matvec(&m, xy_to_xyz(0.34567, 0.35850));
    let scale: Mat3 = [
        [dst_cone[0] / src_cone[0], 0.0, 0.0],
        [0.0, dst_cone[1] / src_cone[1], 0.0],
        [0.0, 0.0, dst_cone[2] / src_cone[2]],
    ];
    matmul(&matmul(&m_inv, &scale), &m)
}

/// Python's `int(round(x))` (round half to even).
fn py_round(x: f64) -> i64 {
    x.round_ties_even() as i64
}

fn s15fixed16(v: f64) -> [u8; 4] {
    (py_round(v * 65536.0) as i32).to_be_bytes()
}

fn xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut t = b"XYZ \x00\x00\x00\x00".to_vec();
    for v in [x, y, z] {
        t.extend_from_slice(&s15fixed16(v));
    }
    t
}

fn pad4(t: &mut Vec<u8>) {
    while !t.len().is_multiple_of(4) {
        t.push(0);
    }
}

/// `GenerateICCProfileBytesFromGammaAndChromaticityPNG`: a matrix/TRC display
/// profile equivalent to the PNG's gAMA and cHRM chunks.
pub(crate) fn profile_from_png_gamma_chromaticity(linear_gamma: f64, chroma: &[f64; 8]) -> Vec<u8> {
    let encoding_gamma = 1.0 / linear_gamma;
    let white = [chroma[0], chroma[1]];
    let m = rgb_to_xyz(
        white,
        [chroma[2], chroma[3]],
        [chroma[4], chroma[5]],
        [chroma[6], chroma[7]],
    );
    let m50 = matmul(&bradford_to_d50(white), &m);

    let mut desc = b"desc\x00\x00\x00\x00".to_vec();
    let text = b"iccp\x00";
    desc.extend_from_slice(&(text.len() as u32).to_be_bytes());
    desc.extend_from_slice(text);
    pad4(&mut desc);
    let mut cprt = b"text\x00\x00\x00\x00".to_vec();
    cprt.extend_from_slice(b"Generated from PNG gamma/chromaticities");
    pad4(&mut cprt);
    let wtpt = xyz_tag(0.9642, 1.0, 0.8249);
    let mut chad = b"sf32\x00\x00\x00\x00".to_vec();
    for (i, j) in (0..3).flat_map(|i| (0..3).map(move |j| (i, j))) {
        let v: f32 = if i == j { 1.0 } else { 0.0 };
        chad.extend_from_slice(&v.to_be_bytes());
    }
    let column = |c: usize| xyz_tag(m50[0][c], m50[1][c], m50[2][c]);
    let g = py_round(encoding_gamma * 256.0).clamp(0, 0xFFFF) as u16;
    let mut trc = b"curv\x00\x00\x00\x00".to_vec();
    trc.extend_from_slice(&1u32.to_be_bytes());
    trc.extend_from_slice(&g.to_be_bytes());
    pad4(&mut trc);

    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc),
        (b"cprt", cprt),
        (b"wtpt", wtpt),
        (b"chad", chad),
        (b"rXYZ", column(0)),
        (b"gXYZ", column(1)),
        (b"bXYZ", column(2)),
        (b"rTRC", trc.clone()),
        (b"gTRC", trc.clone()),
        (b"bTRC", trc),
    ];
    let mut table = (tags.len() as u32).to_be_bytes().to_vec();
    let mut blocks = Vec::new();
    let mut offset = 128 + 4 + tags.len() * 12;
    for (sig, data) in &tags {
        let mut padded = data.clone();
        pad4(&mut padded);
        table.extend_from_slice(*sig);
        table.extend_from_slice(&(offset as u32).to_be_bytes());
        table.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += padded.len();
        blocks.extend(padded);
    }
    let mut body = table;
    body.extend(blocks);
    let mut total = 128 + body.len();
    let final_pad = (4 - total % 4) % 4;
    total += final_pad;

    let mut header = Vec::with_capacity(128);
    header.extend_from_slice(&(total as u32).to_be_bytes());
    header.extend_from_slice(b"lcms");
    header.extend_from_slice(&[0x02, 0x10, 0, 0]);
    header.extend_from_slice(b"mntrRGB XYZ ");
    // creation date: the reference writes "now"; it does not affect transforms
    for v in [2020u16, 1, 1, 0, 0, 0] {
        header.extend_from_slice(&v.to_be_bytes());
    }
    header.extend_from_slice(b"acsp    ");
    header.extend_from_slice(&[0; 4 + 4 + 4 + 8 + 4]);
    for v in [0.9642, 1.0, 0.8249] {
        header.extend_from_slice(&s15fixed16(v));
    }
    header.extend_from_slice(b"none");
    header.extend_from_slice(&[0; 16 + 28]);
    debug_assert_eq!(header.len(), 128);

    let mut profile = header;
    profile.extend(body);
    profile.extend(std::iter::repeat_n(0, final_pad));
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_to_srgb_is_near_identity() {
        let srgb = lcms2::Profile::new_srgb().icc().unwrap();
        let img = PilImage::from_u8(Mode::Rgb, 2, 1, vec![10, 200, 30, 255, 0, 128]).unwrap();
        let out = to_srgb(&img, &srgb).unwrap();
        let Pixels::U8(px) = out.pixels else {
            unreachable!()
        };
        for (a, b) in px.iter().zip([10u8, 200, 30, 255, 0, 128]) {
            assert!(a.abs_diff(b) <= 1, "{px:?}");
        }
    }

    #[test]
    fn untransformable_modes_are_left_alone() {
        let srgb = lcms2::Profile::new_srgb().icc().unwrap();
        let img = PilImage::from_u8(Mode::L, 1, 1, vec![7]).unwrap();
        assert!(to_srgb(&img, &srgb).is_none());
        let img = PilImage::from_u8(Mode::Rgb, 1, 1, vec![7, 7, 7]).unwrap();
        assert!(to_srgb(&img, b"not a profile").is_none());
    }

    #[test]
    fn generated_profile_is_loadable() {
        let chroma = [0.3127, 0.329, 0.64, 0.33, 0.3, 0.6, 0.15, 0.06];
        let p = profile_from_png_gamma_chromaticity(0.45455, &chroma);
        assert_eq!(p.len() % 4, 0);
        assert!(lcms2::Profile::new_icc(&p).is_ok());
    }
}
