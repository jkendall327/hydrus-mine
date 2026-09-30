//! Opening image files the way Pillow does: same mode, same pixels, same
//! metadata the reference looks at.
//!
//! Decoders are chosen for bit-exactness with Pillow's: libjpeg-turbo for
//! JPEG (pure-Rust JPEG decoders differ by up to 4 levels per channel),
//! image-webp for WebP (verified identical to libwebp), and lossless formats
//! (PNG, GIF, BMP, TIFF, QOI) through small parsers that mirror Pillow's
//! plugins, since the interesting part there is Pillow's choice of mode and
//! palette handling rather than the decompression.

use std::io::{Cursor, Read};

use flate2::read::ZlibDecoder;

use super::exif::{self, ExifSummary};
use super::pil::{Format, Mode, PilImage, Pixels, PngColour, Transparency};
use crate::error::{MediaError, Result};

/// An opened image plus the metadata the import flags need.
#[derive(Debug, Clone)]
pub(crate) struct Opened {
    pub image: PilImage,
    /// Entries of Pillow's `info` dict that could count as human-readable
    /// embedded metadata (after `load()`).
    pub text_info: Vec<(String, InfoValue)>,
}

/// The shape of an `info` value, as far as `render_dict` cares.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InfoValue {
    Text(String),
    Bytes,
}

fn damaged(what: &str) -> MediaError {
    MediaError::damaged(format!(
        "Could not load the image--it was likely malformed! ({what})"
    ))
}

/// Which Pillow plugin would accept these bytes.
pub(crate) fn sniff(data: &[u8]) -> Option<Format> {
    Some(if data.starts_with(b"\xff\xd8\xff") {
        Format::Jpeg
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Format::Png
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Format::Gif
    } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Format::Webp
    } else if data.starts_with(b"BM") {
        Format::Bmp
    } else if data.starts_with(b"\x00\x00\x01\x00") {
        Format::Ico
    } else if data.starts_with(b"II*\x00") || data.starts_with(b"MM\x00*") {
        Format::Tiff
    } else if data.starts_with(b"qoif") {
        Format::Qoi
    } else {
        return None;
    })
}

/// `RawOpenPILImage` + `load()` for the formats we decode natively.
pub(crate) fn open(data: &[u8]) -> Result<Opened> {
    match sniff(data) {
        Some(Format::Jpeg) => open_jpeg(data),
        Some(Format::Png) => open_png(data),
        Some(Format::Gif) => open_gif(data),
        Some(Format::Webp) => open_webp(data),
        Some(Format::Bmp) => open_bmp(data),
        Some(Format::Ico) => open_ico(data),
        Some(Format::Tiff) => open_tiff(data),
        Some(Format::Qoi) => open_qoi(data),
        _ => Err(MediaError::damaged(
            "Could not load the image--it was likely malformed!",
        )),
    }
}

fn plain(image: PilImage) -> Opened {
    Opened {
        image,
        text_info: Vec::new(),
    }
}

fn base_image(format: Format, mode: Mode, width: u32, height: u32, pixels: Pixels) -> PilImage {
    PilImage {
        mode,
        width,
        height,
        pixels,
        palette: vec![[0; 3]; 256],
        transparency: None,
        format: Some(format),
        icc_profile: None,
        exif: ExifSummary::default(),
        png: PngColour::default(),
    }
}

// --------------------------------------------------------------------- JPEG

/// `GeneratePILImage(path, dequantize=False).size`: the size Pillow reports
/// after EXIF rotation, read from headers where that is cheap.
pub(crate) fn image_size(data: &[u8]) -> Result<(u32, u32)> {
    let swap_for = |orientation: Option<u32>, (w, h): (u32, u32)| {
        if matches!(orientation, Some(5..=8)) {
            (h, w)
        } else {
            (w, h)
        }
    };
    match sniff(data) {
        Some(Format::Jpeg) => {
            let h = jpeg_header(data)?;
            if h.components == 0 {
                return Err(damaged("no frame header"));
            }
            let exif = h
                .exif
                .as_deref()
                .map(exif::summarise_exif_blob)
                .unwrap_or_default();
            let exif = exif::with_xmp_fallback(exif, h.xmp.as_deref());
            Ok(swap_for(exif.orientation, h.size))
        }
        Some(Format::Png) => {
            let ihdr = data.get(16..24).ok_or_else(|| damaged("missing IHDR"))?;
            Ok((
                u32::from_be_bytes([ihdr[0], ihdr[1], ihdr[2], ihdr[3]]),
                u32::from_be_bytes([ihdr[4], ihdr[5], ihdr[6], ihdr[7]]),
            ))
        }
        Some(Format::Gif) => Ok(crate::formats::gif::open_size(&crate::formats::gif::parse(
            data,
        )?)),
        Some(Format::Webp) => {
            let dec = image_webp::WebPDecoder::new(Cursor::new(data))
                .map_err(|e| damaged(&e.to_string()))?;
            let exif = crate::formats::webp::chunks(data)
                .into_iter()
                .find(|(k, _)| k == b"EXIF")
                .map(|(_, e)| exif::summarise_exif_blob(e))
                .unwrap_or_default();
            Ok(swap_for(exif.orientation, dec.dimensions()))
        }
        Some(_) => {
            let img = open(data)?.image;
            // TIFFs rotate twice (see PilImage::rotate_exif); quarter turns cancel out
            let orientation = if img.format == Some(Format::Tiff) {
                None
            } else {
                img.exif.orientation
            };
            Ok(swap_for(orientation, (img.width, img.height)))
        }
        None => {
            if let Some(size) = crate::formats::isobmff::display_size(data) {
                return Ok(size);
            }
            Err(damaged("unknown image format"))
        }
    }
}

struct JpegHeader {
    size: (u32, u32),
    components: u8,
    exif: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
    icc_chunks: Vec<Vec<u8>>,
}

fn jpeg_header(data: &[u8]) -> Result<JpegHeader> {
    let mut h = JpegHeader {
        size: (0, 0),
        components: 0,
        exif: None,
        xmp: None,
        icc_chunks: Vec::new(),
    };
    let mut i = 2;
    loop {
        // skip fill bytes and junk until a marker
        while i < data.len() && data[i] != 0xFF {
            i += 1;
        }
        while i < data.len() && data[i] == 0xFF {
            i += 1;
        }
        let Some(&marker) = data.get(i) else {
            return Err(damaged("no start of scan"));
        };
        i += 1;
        if marker == 0x00 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        let len = usize::from(u16::from_be_bytes([
            *data.get(i).ok_or_else(|| damaged("truncated marker"))?,
            *data.get(i + 1).ok_or_else(|| damaged("truncated marker"))?,
        ]));
        if len < 2 {
            return Err(damaged("bad marker length"));
        }
        let payload = data
            .get(i + 2..i + len)
            .ok_or_else(|| damaged("truncated marker"))?;
        match marker {
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                h.components = *payload.get(5).ok_or_else(|| damaged("short SOF"))?;
                h.size = (
                    u32::from(u16::from_be_bytes([payload[3], payload[4]])),
                    u32::from(u16::from_be_bytes([payload[1], payload[2]])),
                );
            }
            0xE1 if payload.starts_with(b"Exif\x00\x00") => match &mut h.exif {
                Some(e) => e.extend_from_slice(&payload[6..]),
                None => h.exif = Some(payload.to_vec()),
            },
            0xE1 if payload.starts_with(b"http://ns.adobe.com/xap/1.0/\x00") => {
                h.xmp = payload.splitn(2, |&b| b == 0).nth(1).map(<[u8]>::to_vec);
            }
            0xE2 if payload.starts_with(b"ICC_PROFILE\x00") => h.icc_chunks.push(payload.to_vec()),
            0xDA => return Ok(h),
            _ => {}
        }
        i += len;
    }
}

fn jpeg_icc(mut chunks: Vec<Vec<u8>>) -> Option<Vec<u8>> {
    if chunks.is_empty() {
        return None;
    }
    chunks.sort();
    let count = *chunks[0].get(13)?;
    if usize::from(count) != chunks.len() {
        return None;
    }
    Some(
        chunks
            .iter()
            .flat_map(|c| c.get(14..).unwrap_or(&[]).iter().copied())
            .collect(),
    )
}

fn open_jpeg(data: &[u8]) -> Result<Opened> {
    let header = jpeg_header(data)?;
    let (mode, format) = match header.components {
        1 => (Mode::L, turbojpeg::PixelFormat::GRAY),
        3 => (Mode::Rgb, turbojpeg::PixelFormat::RGB),
        4 => (Mode::Cmyk, turbojpeg::PixelFormat::CMYK),
        _ => return Err(damaged("unsupported JPEG component count")),
    };
    let img = turbojpeg::decompress(data, format).map_err(|e| damaged(&e.to_string()))?;
    let mut pixels = img.pixels;
    if mode == Mode::Cmyk {
        // Pillow assumes Adobe's inverted CMYK ("CMYK;I")
        for v in &mut pixels {
            *v = 255 - *v;
        }
    }
    let mut image = base_image(
        Format::Jpeg,
        mode,
        img.width as u32,
        img.height as u32,
        Pixels::U8(pixels),
    );
    image.check()?;
    image.icc_profile = jpeg_icc(header.icc_chunks);
    let summary = header
        .exif
        .as_deref()
        .map(exif::summarise_exif_blob)
        .unwrap_or_default();
    image.exif = exif::with_xmp_fallback(summary, header.xmp.as_deref());
    Ok(plain(image))
}

// ---------------------------------------------------------------------- PNG

struct PngChunk<'a> {
    kind: [u8; 4],
    data: &'a [u8],
}

fn png_chunks(data: &[u8]) -> Vec<PngChunk<'_>> {
    let mut out = Vec::new();
    let mut i = 8;
    while i + 8 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let kind = [data[i + 4], data[i + 5], data[i + 6], data[i + 7]];
        let Some(chunk) = data.get(i + 8..i + 8 + len) else {
            break;
        };
        out.push(PngChunk { kind, data: chunk });
        if &kind == b"IEND" {
            break;
        }
        i += 12 + len;
    }
    out
}

fn zlib_inflate(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    ZlibDecoder::new(data).read_to_end(&mut out)?;
    Ok(out)
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Pillow's `info` entries from tEXt/zTXt/iTXt chunks.
fn png_text(chunk: &PngChunk<'_>) -> Option<(String, InfoValue)> {
    match &chunk.kind {
        b"tEXt" => {
            let (k, v) = match chunk.data.iter().position(|&b| b == 0) {
                Some(p) => (&chunk.data[..p], &chunk.data[p + 1..]),
                None => (chunk.data, &[][..]),
            };
            if k.is_empty() {
                return None;
            }
            let value = if k == b"exif" {
                InfoValue::Bytes
            } else {
                InfoValue::Text(latin1(v))
            };
            Some((latin1(k), value))
        }
        b"zTXt" => {
            let (k, v) = match chunk.data.iter().position(|&b| b == 0) {
                Some(p) => (&chunk.data[..p], &chunk.data[p + 1..]),
                None => (chunk.data, &[][..]),
            };
            if v.first().is_some_and(|&m| m != 0) || k.is_empty() {
                return None;
            }
            let text = v
                .get(1..)
                .and_then(|z| zlib_inflate(z).ok())
                .unwrap_or_default();
            let value = if k == b"exif" {
                InfoValue::Bytes
            } else {
                InfoValue::Text(latin1(&text))
            };
            Some((latin1(k), value))
        }
        b"iTXt" => {
            let p = chunk.data.iter().position(|&b| b == 0)?;
            let (k, r) = (&chunk.data[..p], &chunk.data[p + 1..]);
            if r.len() < 2 {
                return None;
            }
            let (cf, cm, r) = (r[0], r[1], &r[2..]);
            let mut parts = r.splitn(3, |&b| b == 0);
            let (lang, tk, v) = (parts.next()?, parts.next()?, parts.next()?);
            let v = if cf != 0 {
                if cm != 0 {
                    return None;
                }
                zlib_inflate(v).ok()?
            } else {
                v.to_vec()
            };
            std::str::from_utf8(lang).ok()?;
            std::str::from_utf8(tk).ok()?;
            let v = String::from_utf8(v).ok()?;
            Some((latin1(k), InfoValue::Text(v)))
        }
        _ => None,
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i16::from(a), i16::from(b), i16::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Undo PNG filtering for one (sub)image; returns the raw rows.
fn unfilter(data: &[u8], row_bytes: usize, rows: usize, bpp: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; row_bytes * rows];
    for y in 0..rows {
        let start = y * (row_bytes + 1);
        let line = data
            .get(start..start + row_bytes + 1)
            .ok_or_else(|| damaged("image file is truncated"))?;
        let (filter, line) = (line[0], &line[1..]);
        let (prev_rows, cur_rows) = out.split_at_mut(y * row_bytes);
        let prev = if y == 0 {
            None
        } else {
            Some(&prev_rows[(y - 1) * row_bytes..])
        };
        let cur = &mut cur_rows[..row_bytes];
        for x in 0..row_bytes {
            let a = if x >= bpp { cur[x - bpp] } else { 0 };
            let b = prev.map_or(0, |p| p[x]);
            let c = if x >= bpp {
                prev.map_or(0, |p| p[x - bpp])
            } else {
                0
            };
            let v = line[x];
            cur[x] = match filter {
                0 => v,
                1 => v.wrapping_add(a),
                2 => v.wrapping_add(b),
                3 => v.wrapping_add(u16::midpoint(u16::from(a), u16::from(b)) as u8),
                4 => v.wrapping_add(paeth(a, b, c)),
                _ => return Err(damaged("unknown PNG filter type")),
            };
        }
    }
    Ok(out)
}

/// Read sample `x` of a raw row at `bit_depth` bits per sample.
fn sample(row: &[u8], x: usize, bit_depth: u8) -> u16 {
    match bit_depth {
        16 => u16::from_be_bytes([row[2 * x], row[2 * x + 1]]),
        8 => u16::from(row[x]),
        d => {
            let per_byte = 8 / usize::from(d);
            let byte = row[x / per_byte];
            let shift = 8 - usize::from(d) * (x % per_byte + 1);
            u16::from((byte >> shift) & ((1u8 << d) - 1))
        }
    }
}

fn open_png(data: &[u8]) -> Result<Opened> {
    let chunks = png_chunks(data);
    let ihdr = chunks
        .first()
        .filter(|c| &c.kind == b"IHDR" && c.data.len() >= 13)
        .ok_or_else(|| damaged("missing IHDR"))?
        .data;
    let width = u32::from_be_bytes([ihdr[0], ihdr[1], ihdr[2], ihdr[3]]);
    let height = u32::from_be_bytes([ihdr[4], ihdr[5], ihdr[6], ihdr[7]]);
    let (bit_depth, colour_type, interlaced) = (ihdr[8], ihdr[9], ihdr[12] != 0);
    let mode = match (bit_depth, colour_type) {
        (1, 0) => Mode::One,
        (2 | 4 | 8, 0) => Mode::L,
        (16, 0) => Mode::I16,
        (8 | 16, 2) => Mode::Rgb,
        (1 | 2 | 4 | 8, 3) => Mode::P,
        (8, 4) => Mode::LA,
        (16, 4) | (8 | 16, 6) => Mode::Rgba,
        _ => return Err(damaged("unsupported PNG bit depth/colour type")),
    };
    let samples = match colour_type {
        2 => 3,
        4 => 2,
        6 => 4,
        _ => 1,
    };

    let mut image = base_image(Format::Png, mode, width, height, Pixels::U8(Vec::new()));
    let mut text_info = Vec::new();
    let mut idat = Vec::new();
    let mut seen_idat = false;
    let mut exif_blob: Option<Vec<u8>> = None;
    let mut xmp: Option<Vec<u8>> = None;
    let mut raw_exif_text: Option<String> = None;
    for chunk in &chunks {
        let before_idat = !seen_idat;
        match &chunk.kind {
            b"IDAT" => {
                seen_idat = true;
                idat.extend_from_slice(chunk.data);
            }
            b"PLTE" if before_idat && mode == Mode::P => {
                for (slot, rgb) in image.palette.iter_mut().zip(chunk.data.chunks_exact(3)) {
                    *slot = [rgb[0], rgb[1], rgb[2]];
                }
            }
            b"tRNS" if before_idat => {
                image.transparency = Some(match mode {
                    Mode::P => {
                        let first_zero = chunk.data.iter().position(|&b| b == 0);
                        let single_zero = first_zero.is_some()
                            && first_zero == chunk.data.iter().rposition(|&b| b == 0);
                        let others_opaque = chunk.data.iter().all(|&b| b == 0 || b == 0xFF);
                        if single_zero && others_opaque {
                            let i = chunk.data.iter().position(|&b| b == 0).unwrap_or(0);
                            Transparency::Index(i as u8)
                        } else {
                            Transparency::Alphas(chunk.data.to_vec())
                        }
                    }
                    _ => Transparency::ColourKey,
                });
            }
            b"gAMA" if before_idat && chunk.data.len() >= 4 => {
                let g = u32::from_be_bytes([
                    chunk.data[0],
                    chunk.data[1],
                    chunk.data[2],
                    chunk.data[3],
                ]);
                image.png.gamma = Some(f64::from(g) / 100_000.0);
            }
            b"cHRM" if before_idat => {
                let vals: Vec<f64> = chunk
                    .data
                    .chunks_exact(4)
                    .map(|c| f64::from(u32::from_be_bytes([c[0], c[1], c[2], c[3]])) / 100_000.0)
                    .collect();
                image.png.chromaticity = vals.try_into().ok();
            }
            b"sRGB" if before_idat && !chunk.data.is_empty() => image.png.srgb = true,
            b"iCCP" if before_idat => {
                let p = chunk.data.iter().position(|&b| b == 0).unwrap_or(0);
                if chunk.data.get(p + 1) == Some(&0) {
                    image.icc_profile = zlib_inflate(&chunk.data[p + 2..]).ok();
                }
            }
            b"eXIf" => {
                let mut blob = b"Exif\x00\x00".to_vec();
                blob.extend_from_slice(chunk.data);
                exif_blob = Some(blob);
            }
            b"tEXt" | b"zTXt" | b"iTXt" => {
                if &chunk.kind == b"iTXt" && chunk.data.starts_with(b"XML:com.adobe.xmp\x00") {
                    xmp = Some(chunk.data.to_vec());
                }
                if let Some((k, v)) = png_text(chunk) {
                    if k == "Raw profile type exif"
                        && let InfoValue::Text(t) = &v
                    {
                        raw_exif_text = Some(t.clone());
                    }
                    text_info.push((k, v));
                }
            }
            _ => {}
        }
    }
    if idat.is_empty() {
        return Err(damaged("no image data"));
    }
    let raw = zlib_inflate(&idat).or_else(|e| {
        // a truncated stream still yields what was decompressed so far
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            Ok(Vec::new())
        } else {
            Err(e)
        }
    });
    let raw = raw.map_err(|_| damaged("bad zlib stream"))?;

    let (w, h) = (width as usize, height as usize);
    let bits_pp = samples * usize::from(bit_depth);
    let bpp = bits_pp.div_ceil(8).max(1);
    // unfiltered rows for the full image, row_bytes each
    let row_bytes = (w * bits_pp).div_ceil(8);
    let full = if interlaced {
        let passes = [
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ];
        // deinterlace into a samples-per-pixel u16 buffer, then repack
        let mut px = vec![0u16; w * h * samples];
        let mut offset = 0;
        for (x0, y0, dx, dy) in passes {
            let pw = if w > x0 { (w - x0).div_ceil(dx) } else { 0 };
            let ph = if h > y0 { (h - y0).div_ceil(dy) } else { 0 };
            if pw == 0 || ph == 0 {
                continue;
            }
            let prb = (pw * bits_pp).div_ceil(8);
            let pass_len = (prb + 1) * ph;
            let pass = unfilter(raw.get(offset..).unwrap_or(&[]), prb, ph, bpp)?;
            offset += pass_len;
            for py in 0..ph {
                let row = &pass[py * prb..(py + 1) * prb];
                for pxi in 0..pw {
                    let (x, y) = (x0 + pxi * dx, y0 + py * dy);
                    for s in 0..samples {
                        px[(y * w + x) * samples + s] = sample(row, pxi * samples + s, bit_depth);
                    }
                }
            }
        }
        px
    } else {
        let rows = unfilter(&raw, row_bytes, h, bpp)?;
        let mut px = Vec::with_capacity(w * h * samples);
        for y in 0..h {
            let row = &rows[y * row_bytes..(y + 1) * row_bytes];
            for i in 0..w * samples {
                px.push(sample(row, i, bit_depth));
            }
        }
        px
    };

    let msb = |v: u16| (v >> 8) as u8;
    image.pixels = match (mode, bit_depth) {
        (Mode::One, _) => Pixels::U8(full.iter().map(|&v| if v != 0 { 255 } else { 0 }).collect()),
        (Mode::L, 2) => Pixels::U8(full.iter().map(|&v| (v * 0x55) as u8).collect()),
        (Mode::L, 4) => Pixels::U8(full.iter().map(|&v| (v * 0x11) as u8).collect()),
        (Mode::I16, _) => Pixels::U16(full),
        (Mode::Rgba, 16) if colour_type == 4 => Pixels::U8(
            full.chunks_exact(2)
                .flat_map(|p| [msb(p[0]), msb(p[0]), msb(p[0]), msb(p[1])])
                .collect(),
        ),
        (_, 16) => Pixels::U8(full.iter().map(|&v| msb(v)).collect()),
        _ => Pixels::U8(full.iter().map(|&v| v as u8).collect()),
    };
    image.check()?;

    let summary = if let Some(blob) = &exif_blob {
        exif::summarise_exif_blob(blob)
    } else if let Some(text) = &raw_exif_text {
        let hex: String = text.split('\n').skip(3).collect();
        decode_hex(&hex)
            .map(|b| exif::summarise_exif_blob(&b))
            .unwrap_or_default()
    } else {
        ExifSummary::default()
    };
    let xmp_value = xmp.as_deref().and_then(|x| x.splitn(2, |&b| b == 0).nth(1));
    image.exif = exif::with_xmp_fallback(summary, xmp_value);
    Ok(Opened { image, text_info })
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    let s: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    s.chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
        .collect()
}

// ---------------------------------------------------------------------- GIF

/// Frame 0 of a GIF as Pillow's `GifImageFile` opens it.
fn open_gif(data: &[u8]) -> Result<Opened> {
    let frame = crate::formats::gif::first_frame(data)?;
    let mut image = base_image(
        Format::Gif,
        if frame.palette.is_some() {
            Mode::P
        } else {
            Mode::L
        },
        frame.width,
        frame.height,
        Pixels::U8(frame.indices),
    );
    if let Some(palette) = frame.palette {
        for (slot, rgb) in image.palette.iter_mut().zip(palette.chunks_exact(3)) {
            *slot = [rgb[0], rgb[1], rgb[2]];
        }
    }
    image.transparency = frame.transparency.map(|t| {
        if image.mode == Mode::P {
            Transparency::Index(t)
        } else {
            Transparency::ColourKey
        }
    });
    image.check()?;
    Ok(plain(image))
}

// --------------------------------------------------------------------- WebP

fn open_webp(data: &[u8]) -> Result<Opened> {
    let chunks = crate::formats::webp::chunks(data);
    let find = |kind: &[u8; 4]| {
        chunks
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, d)| d.to_vec())
    };
    let mut decoder =
        image_webp::WebPDecoder::new(Cursor::new(data)).map_err(|e| damaged(&e.to_string()))?;
    let (width, height) = decoder.dimensions();
    let has_alpha = decoder.has_alpha();
    let rgba = if decoder.is_animated() {
        crate::formats::webp::first_frame_rgba(data, width, height)?
    } else {
        let size = decoder
            .output_buffer_size()
            .ok_or_else(|| damaged("webp too large"))?;
        let mut buf = vec![0u8; size];
        decoder
            .read_image(&mut buf)
            .map_err(|e| damaged(&e.to_string()))?;
        if has_alpha {
            buf
        } else {
            buf.chunks_exact(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect()
        }
    };
    let (mode, pixels) = if has_alpha {
        (Mode::Rgba, rgba)
    } else {
        (
            Mode::Rgb,
            rgba.chunks_exact(4)
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect(),
        )
    };
    let mut image = base_image(Format::Webp, mode, width, height, Pixels::U8(pixels));
    image.check()?;
    image.icc_profile = find(b"ICCP");
    let summary = find(b"EXIF")
        .map(|e| exif::summarise_exif_blob(&e))
        .unwrap_or_default();
    image.exif = exif::with_xmp_fallback(summary, find(b"XMP ").as_deref());
    Ok(plain(image))
}

// ---------------------------------------------------------------------- BMP

fn le16(d: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*d.get(i)?, *d.get(i + 1)?]))
}

fn le32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *d.get(i)?,
        *d.get(i + 1)?,
        *d.get(i + 2)?,
        *d.get(i + 3)?,
    ]))
}

/// A BMP/DIB decoded as Pillow's `BmpImageFile._bitmap` does.
struct Dib {
    image: PilImage,
    /// Offset of the pixel array (for ICO alpha extraction).
    pixel_offset: usize,
    bits: u16,
}

/// How a DIB's pixel data is laid out (Pillow's raw mode for the BMP decoder).
#[derive(PartialEq, Clone, Copy)]
enum Raw {
    Index(u16),
    Bgr15,
    Bgr16,
    Bgr,
    Bgrx,
    Bgra,
    Rgba,
    Xbgr,
    Bgxr,
    Abgr,
    Bgar,
}

#[allow(
    clippy::many_single_char_names,
    reason = "image geometry, named as in Pillow"
)]
fn decode_dib(
    data: &[u8],
    header_at: usize,
    file_offset: usize,
    height_halved: bool,
) -> Result<Dib> {
    let bad = || damaged("bad BMP header");
    let header_size = le32(data, header_at).ok_or_else(bad)? as usize;
    let hd = data
        .get(header_at + 4..header_at + header_size)
        .ok_or_else(bad)?;
    let (width, mut height, bits, compression, mut colors, padding, direction_up);
    let mut masks: Option<[u32; 4]> = None;
    let mut read_pos = header_at + header_size;
    if header_size == 12 {
        width = u32::from(le16(hd, 0).ok_or_else(bad)?);
        height = u32::from(le16(hd, 2).ok_or_else(bad)?);
        bits = le16(hd, 6).ok_or_else(bad)?;
        compression = 0;
        colors = 0;
        padding = 3;
        direction_up = true;
    } else if [40, 52, 56, 64, 108, 124].contains(&header_size) {
        let y_flip = hd.get(7) == Some(&0xFF);
        width = le32(hd, 0).ok_or_else(bad)?;
        let raw_h = le32(hd, 4).ok_or_else(bad)?;
        height = if y_flip { raw_h.wrapping_neg() } else { raw_h };
        bits = le16(hd, 10).ok_or_else(bad)?;
        compression = le32(hd, 12).ok_or_else(bad)?;
        colors = le32(hd, 28).ok_or_else(bad)?;
        padding = 4;
        direction_up = !y_flip;
        if compression == 3 {
            if hd.len() >= 48 {
                let a = if hd.len() >= 52 {
                    le32(hd, 48).ok_or_else(bad)?
                } else {
                    0
                };
                masks = Some([
                    le32(hd, 36).ok_or_else(bad)?,
                    le32(hd, 40).ok_or_else(bad)?,
                    le32(hd, 44).ok_or_else(bad)?,
                    a,
                ]);
            } else {
                let m = [
                    le32(data, read_pos),
                    le32(data, read_pos + 4),
                    le32(data, read_pos + 8),
                ];
                read_pos += 12;
                masks = Some([
                    m[0].ok_or_else(bad)?,
                    m[1].ok_or_else(bad)?,
                    m[2].ok_or_else(bad)?,
                    0,
                ]);
            }
        }
    } else {
        return Err(damaged("unsupported BMP header type"));
    }
    if height_halved {
        height /= 2;
    }
    if colors == 0 {
        colors = 1u32.checked_shl(u32::from(bits)).unwrap_or(0);
    }
    let mut offset = file_offset;
    if offset == 14 + header_size && bits <= 8 {
        offset += padding * colors as usize;
    }
    let (mut mode, mut raw) = match bits {
        1 | 4 | 8 => (Mode::P, Raw::Index(bits)),
        16 => (Mode::Rgb, Raw::Bgr15),
        24 => (Mode::Rgb, Raw::Bgr),
        32 => (Mode::Rgb, Raw::Bgrx),
        _ => return Err(damaged("unsupported BMP pixel depth")),
    };
    match compression {
        3 => {
            let m = masks.ok_or_else(bad)?;
            raw = match (bits, m) {
                (32, [0x00FF_0000, 0xFF00, 0xFF, 0]) => Raw::Bgrx,
                (32, [0xFF00_0000, 0x00FF_0000, 0xFF00, 0]) => Raw::Xbgr,
                (32, [0xFF00_0000, 0xFF00, 0xFF, 0]) => Raw::Bgxr,
                (32, [0xFF00_0000, 0x00FF_0000, 0xFF00, 0xFF]) => Raw::Abgr,
                (32, [0xFF, 0xFF00, 0x00FF_0000, 0xFF00_0000]) => Raw::Rgba,
                (32, [0x00FF_0000, 0xFF00, 0xFF, 0xFF00_0000] | [0, 0, 0, 0]) => Raw::Bgra,
                (32, [0xFF00_0000, 0xFF00, 0xFF, 0x00FF_0000]) => Raw::Bgar,
                (24, [0x00FF_0000, 0xFF00, 0xFF, _]) => Raw::Bgr,
                (16, [0xF800, 0x7E0, 0x1F, _]) => Raw::Bgr16,
                (16, [0x7C00, 0x3E0, 0x1F, _]) => Raw::Bgr15,
                _ => return Err(damaged("Unsupported BMP bitfields layout")),
            };
            if matches!(raw, Raw::Bgra | Raw::Rgba | Raw::Abgr | Raw::Bgar) {
                mode = Mode::Rgba;
            }
        }
        0 => {}
        _ => return Err(damaged("unsupported BMP compression")),
    }
    let mut palette = vec![[0u8; 3]; 256];
    if mode == Mode::P {
        if colors == 0 || colors > 65536 {
            return Err(damaged("Unsupported BMP Palette size"));
        }
        let pal = data
            .get(read_pos..read_pos + padding * colors as usize)
            .ok_or_else(bad)?;
        read_pos += padding * colors as usize;
        let indices: Vec<u32> = if colors == 2 {
            vec![0, 255]
        } else {
            (0..colors).collect()
        };
        let mut grayscale = true;
        for (ind, val) in indices.iter().enumerate() {
            let rgb = pal.get(ind * padding..ind * padding + 3).unwrap_or(&[]);
            if rgb != [*val as u8; 3] || *val > 255 {
                grayscale = false;
            }
        }
        for (i, slot) in palette.iter_mut().enumerate() {
            if let Some(bgr) = pal.get(i * padding..i * padding + 3) {
                *slot = [bgr[2], bgr[1], bgr[0]];
            }
        }
        if grayscale {
            mode = if colors == 2 { Mode::One } else { Mode::L };
        }
    }
    let pixel_offset = if offset != 0 { offset } else { read_pos };
    let (w, h) = (width as usize, height as usize);
    let stride = ((w * usize::from(bits) + 31) >> 3) & !3;
    let src = data.get(pixel_offset..).ok_or_else(bad)?;
    if src.len() < stride * h {
        return Err(damaged("image file is truncated"));
    }
    let mut out = Vec::with_capacity(w * h * mode.channels());
    for y in 0..h {
        let sy = if direction_up { h - 1 - y } else { y };
        let row = &src[sy * stride..(sy + 1) * stride];
        for x in 0..w {
            match raw {
                Raw::Index(b) => {
                    let v = sample(row, x, b as u8) as u8;
                    out.push(match mode {
                        Mode::One => {
                            if v != 0 {
                                255
                            } else {
                                0
                            }
                        }
                        _ => v,
                    });
                }
                Raw::Bgr => out.extend_from_slice(&[row[3 * x + 2], row[3 * x + 1], row[3 * x]]),
                Raw::Bgr15 | Raw::Bgr16 => {
                    let p = u16::from_le_bytes([row[2 * x], row[2 * x + 1]]);
                    let (r, g, b) = if raw == Raw::Bgr15 {
                        ((p >> 10) & 31, (p >> 5) & 31, p & 31)
                    } else {
                        ((p >> 11) & 31, (p >> 5) & 63, p & 31)
                    };
                    let g_max = if raw == Raw::Bgr15 { 31 } else { 63 };
                    out.extend_from_slice(&[
                        (u32::from(r) * 255 / 31) as u8,
                        (u32::from(g) * 255 / g_max) as u8,
                        (u32::from(b) * 255 / 31) as u8,
                    ]);
                }
                _ => {
                    let p = &row[4 * x..4 * x + 4];
                    let (r, g, b, a) = match raw {
                        Raw::Bgrx | Raw::Bgra => (p[2], p[1], p[0], p[3]),
                        Raw::Rgba => (p[0], p[1], p[2], p[3]),
                        Raw::Xbgr | Raw::Abgr => (p[3], p[2], p[1], p[0]),
                        Raw::Bgxr => (p[3], p[1], p[0], p[2]),
                        Raw::Bgar => (p[3], p[2], p[0], p[1]),
                        _ => unreachable!("handled above"),
                    };
                    out.extend_from_slice(&[r, g, b]);
                    if mode == Mode::Rgba {
                        out.push(a);
                    }
                }
            }
        }
    }
    let mut image = base_image(Format::Bmp, mode, width, height, Pixels::U8(out));
    image.palette = palette;
    image.check()?;
    Ok(Dib {
        image,
        pixel_offset,
        bits,
    })
}

fn open_bmp(data: &[u8]) -> Result<Opened> {
    let offset = le32(data, 10).ok_or_else(|| damaged("bad BMP header"))? as usize;
    Ok(plain(decode_dib(data, 14, offset, false)?.image))
}

// ---------------------------------------------------------------------- ICO

fn open_ico(data: &[u8]) -> Result<Opened> {
    struct Entry {
        width: u32,
        height: u32,
        bpp: u16,
        size: usize,
        offset: usize,
        depth: u32,
    }
    let bad = || damaged("bad ICO file");
    let count = usize::from(le16(data, 4).ok_or_else(bad)?);
    let mut entries = Vec::new();
    for i in 0..count {
        let e = data.get(6 + i * 16..6 + i * 16 + 16).ok_or_else(bad)?;
        let width = if e[0] == 0 { 256 } else { u32::from(e[0]) };
        let height = if e[1] == 0 { 256 } else { u32::from(e[1]) };
        let nb_color = u32::from(e[2]);
        let bpp = le16(e, 6).ok_or_else(bad)?;
        let depth = if bpp != 0 {
            u32::from(bpp)
        } else if nb_color != 0 {
            f64::from(nb_color).log2().ceil() as u32
        } else {
            256
        };
        entries.push(Entry {
            width,
            height,
            bpp,
            size: le32(e, 8).ok_or_else(bad)? as usize,
            offset: le32(e, 12).ok_or_else(bad)? as usize,
            depth,
        });
    }
    entries.sort_by_key(|e| e.depth);
    entries.sort_by_key(|e| std::cmp::Reverse(e.width * e.height));
    let e = entries.first().ok_or_else(bad)?;
    let body = data.get(e.offset..).ok_or_else(bad)?;
    let mut image = if body.starts_with(b"\x89PNG\r\n\x1a\n") {
        open_png(body)?.image
    } else {
        let dib = decode_dib(data, e.offset, 0, true)?;
        let mut im = dib.image;
        let (w, h) = (im.width as usize, im.height as usize);
        let alpha: Option<Vec<u8>> = if e.bpp == 32 {
            // the alpha bytes of the (bottom-up) pixel array
            let px = data.get(dib.pixel_offset..dib.pixel_offset + w * h * 4);
            px.map(|px| {
                let mut a = vec![0u8; w * h];
                for y in 0..h {
                    for x in 0..w {
                        a[(h - 1 - y) * w + x] = px[(y * w + x) * 4 + 3];
                    }
                }
                a
            })
        } else {
            let mut padded_w = w;
            if w % 32 > 0 {
                padded_w += 32 - w % 32;
            }
            let total = padded_w * h / 8;
            let start = (e.offset + e.size).checked_sub(total).ok_or_else(bad)?;
            data.get(start..start + total).map(|mask| {
                let stride = padded_w / 8;
                let mut a = vec![0u8; w * h];
                for y in 0..h {
                    let row = &mask[(h - 1 - y) * stride..(h - y) * stride];
                    for x in 0..w {
                        let bit = (row[x / 8] >> (7 - x % 8)) & 1;
                        a[y * w + x] = if bit == 0 { 255 } else { 0 };
                    }
                }
                a
            })
        };
        let _ = dib.bits;
        let alpha = alpha.ok_or_else(|| damaged("image file is truncated"))?;
        let rgb = im.convert_to_rgb(false)?;
        let Pixels::U8(rgb_px) = rgb.pixels else {
            return Err(bad());
        };
        im.pixels = Pixels::U8(
            rgb_px
                .chunks_exact(3)
                .zip(alpha)
                .flat_map(|(p, a)| [p[0], p[1], p[2], a])
                .collect(),
        );
        im.mode = Mode::Rgba;
        im.transparency = None;
        im
    };
    image.format = Some(Format::Ico);
    image.icc_profile = None;
    image.exif = ExifSummary::default();
    image.png = PngColour::default();
    Ok(plain(image))
}

// --------------------------------------------------------------------- TIFF

/// The tiff crate refuses palette images; relabel them as greyscale in a
/// copy so it hands us the raw indices.
fn tiff_palette_as_grey(data: &[u8]) -> Option<Vec<u8>> {
    let big = data.starts_with(b"MM");
    let rd16 = |i: usize| -> Option<u16> {
        let b = [*data.get(i)?, *data.get(i + 1)?];
        Some(if big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let rd32 = |i: usize| -> Option<u32> {
        let b = [
            *data.get(i)?,
            *data.get(i + 1)?,
            *data.get(i + 2)?,
            *data.get(i + 3)?,
        ];
        Some(if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let ifd = rd32(4)? as usize;
    let count = usize::from(rd16(ifd)?);
    for i in 0..count {
        let e = ifd + 2 + i * 12;
        if rd16(e)? == 262 && rd16(e + 2)? == 3 && rd16(e + 8)? == 3 {
            let mut copy = data.to_vec();
            let one: [u8; 2] = if big {
                1u16.to_be_bytes()
            } else {
                1u16.to_le_bytes()
            };
            copy[e + 8..e + 10].copy_from_slice(&one);
            return Some(copy);
        }
    }
    None
}

fn open_tiff(data: &[u8]) -> Result<Opened> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;
    let err = |e: tiff::TiffError| damaged(&e.to_string());
    let photometric = Decoder::new(Cursor::new(data))
        .and_then(|mut d| d.get_tag_u32(Tag::PhotometricInterpretation))
        .unwrap_or(1);
    let source: std::borrow::Cow<'_, [u8]> = if photometric == 3 {
        tiff_palette_as_grey(data)
            .ok_or_else(|| damaged("unreadable palette TIFF"))?
            .into()
    } else {
        data.into()
    };
    let mut dec = Decoder::new(Cursor::new(&*source)).map_err(err)?;
    let (width, height) = dec.dimensions().map_err(err)?;
    let samples = dec.get_tag_u32(Tag::SamplesPerPixel).unwrap_or(1);
    let bits = dec
        .get_tag_u16_vec(Tag::BitsPerSample)
        .ok()
        .and_then(|v| v.first().copied())
        .unwrap_or(1);
    let extra = dec
        .get_tag_u16_vec(Tag::ExtraSamples)
        .ok()
        .and_then(|v| v.first().copied());
    let colormap = dec.get_tag_u16_vec(Tag::ColorMap).ok();
    let icc = dec
        .find_tag(Tag::Unknown(34675))
        .ok()
        .flatten()
        .and_then(|v| v.into_u8_vec().ok());
    let result = dec.read_image().map_err(err)?;
    let (mode, pixels) = match (result, photometric, samples) {
        (DecodingResult::U8(v), 0 | 1, 1) => {
            let v = if photometric == 0 {
                v.iter().map(|x| 255 - x).collect()
            } else {
                v
            };
            if bits == 1 {
                (
                    Mode::One,
                    Pixels::U8(v.iter().map(|&x| if x != 0 { 255 } else { 0 }).collect()),
                )
            } else {
                (Mode::L, Pixels::U8(v))
            }
        }
        (DecodingResult::U8(v), 0 | 1, 2) => (Mode::LA, Pixels::U8(v)),
        (DecodingResult::U16(v), 1, 1) => (Mode::I16, Pixels::U16(v)),
        // Pillow reads unsigned 32-bit samples straight into its signed `I` mode
        (DecodingResult::U32(v), 1, 1) => (
            Mode::I32,
            Pixels::I32(v.into_iter().map(|x| x as i32).collect()),
        ),
        (DecodingResult::I32(v), 1, 1) => (Mode::I32, Pixels::I32(v)),
        (DecodingResult::F32(v), 1, 1) => (Mode::F32, Pixels::F32(v)),
        (DecodingResult::U8(v), 2, 3) => (Mode::Rgb, Pixels::U8(v)),
        (DecodingResult::U8(v), 2, 4) => match extra {
            Some(2 | 1) => (Mode::Rgba, Pixels::U8(v)),
            _ => (
                Mode::Rgb,
                Pixels::U8(v.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()),
            ),
        },
        (DecodingResult::U16(v), 2, 3) => (
            Mode::Rgb,
            Pixels::U8(v.iter().map(|x| (x >> 8) as u8).collect()),
        ),
        (DecodingResult::U8(v), 3, 1) => (Mode::P, Pixels::U8(v)),
        (DecodingResult::U8(v), 5, 4) => (Mode::Cmyk, Pixels::U8(v)),
        _ => return Err(damaged("unsupported TIFF layout")),
    };
    let mut image = base_image(Format::Tiff, mode, width, height, pixels);
    if let Some(map) = colormap {
        let n = map.len() / 3;
        for (i, slot) in image.palette.iter_mut().enumerate().take(n) {
            *slot = [
                (map[i] / 256) as u8,
                (map[n + i] / 256) as u8,
                (map[2 * n + i] / 256) as u8,
            ];
        }
    }
    image.check()?;
    image.icc_profile = icc;
    image.exif = exif::summarise_tiff(data, None);
    Ok(plain(image))
}

// ---------------------------------------------------------------------- QOI

fn open_qoi(data: &[u8]) -> Result<Opened> {
    let (header, pixels) = qoi::decode_to_vec(data).map_err(|e| damaged(&e.to_string()))?;
    let mode = if header.channels.as_u8() == 4 {
        Mode::Rgba
    } else {
        Mode::Rgb
    };
    let image = base_image(
        Format::Qoi,
        mode,
        header.width,
        header.height,
        Pixels::U8(pixels),
    );
    image.check()?;
    Ok(plain(image))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffing() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nxxxx"), Some(Format::Png));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::Webp));
        assert_eq!(sniff(b"nope"), None);
    }

    #[test]
    fn png_bit_unpacking() {
        let row = [0b1011_0001u8];
        assert_eq!(sample(&row, 0, 1), 1);
        assert_eq!(sample(&row, 1, 1), 0);
        assert_eq!(sample(&row, 0, 2), 2);
        assert_eq!(sample(&row, 3, 2), 1);
        assert_eq!(sample(&row, 1, 4), 1);
    }
}
