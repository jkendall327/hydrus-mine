//! Reference grayscale PNG pixels and compressed UTF-8 payloads.
use crate::{Definition, Error, MAX_BYTES, Result, decode_text, encode_text};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::io::{Cursor, Read, Write};

/// Read a real hydrus PNG, including RGB bitmap copies and old plain JSON.
pub fn decode_png(bytes: &[u8]) -> Result<Vec<Definition>> {
    decode_text(&decode_payload(bytes)?)
}

pub fn decode_payload(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: MAX_BYTES });
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| Error::Invalid(format!("PNG: {e}")))?;
    let info = reader.info();
    if info.bit_depth != png::BitDepth::Eight {
        return Err(Error::Invalid("PNG requires 8-bit pixel data.".into()));
    }
    let size = reader.output_buffer_size().ok_or(Error::Limit)?;
    if size > MAX_BYTES {
        return Err(Error::Limit);
    }
    let mut pixels = vec![0; size];
    let output = reader
        .next_frame(&mut pixels)
        .map_err(|e| Error::Invalid(format!("PNG: {e}")))?;
    let channels = output.color_type.samples();
    let width = usize::try_from(output.width).map_err(|_| Error::Limit)?;
    let data = pixels[..output.buffer_size()]
        .chunks_exact(channels)
        .map(|p| p[0])
        .collect::<Vec<_>>();
    if data.len() < 2 || width < 2 {
        return Err(Error::Invalid("PNG header is missing.".into()));
    }
    let header_rows = usize::from(u16::from_be_bytes([data[0], data[1]]));
    let start = width.checked_mul(header_rows).ok_or(Error::Limit)?;
    let payload = data
        .get(start..)
        .filter(|p| p.len() >= 4)
        .ok_or_else(|| Error::Invalid("PNG header height is outside the image.".into()))?;
    let length = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    if length > MAX_BYTES {
        return Err(Error::Limit);
    }
    let payload = payload
        .get(4..4 + length)
        .ok_or_else(|| Error::Invalid("PNG payload length is outside the image.".into()))?;
    let mut decoded = Vec::new();
    let mut zlib = ZlibDecoder::new(payload);
    match zlib
        .by_ref()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut decoded)
    {
        Ok(_) => {
            if decoded.len() > MAX_BYTES {
                return Err(Error::Limit);
            }
            if zlib.total_in() != payload.len() as u64 {
                return Err(Error::Invalid(
                    "Trailing compressed PNG payload data.".into(),
                ));
            }
        }
        Err(_) => {
            decoded = payload.to_vec();
        }
    }
    let text = std::str::from_utf8(&decoded)
        .map_err(|_| Error::Invalid("PNG payload is not compressed or plain UTF-8 text.".into()))?;
    Ok(text.to_owned())
}
/// Export the reference's grayscale PNG format with a small white header.
pub fn encode_png(definitions: &[Definition]) -> Result<Vec<u8>> {
    encode_payload(&encode_text(definitions)?)
}

pub(super) fn encode_payload(text: &str) -> Result<Vec<u8>> {
    encode_payload_with_header(text, 512, &vec![255; 512])
}

/// Write compressed UTF-8 text in the reference grayscale PNG carrier, using
/// an already rendered header. Its first two pixels encode the header height.
pub fn encode_payload_with_header(text: &str, width: u32, header: &[u8]) -> Result<Vec<u8>> {
    let width = usize::try_from(width).map_err(|_| Error::Limit)?;
    if width < 2 || header.is_empty() || !header.len().is_multiple_of(width) {
        return Err(Error::Invalid(
            "PNG header must contain complete grayscale rows.".into(),
        ));
    }
    let header_rows = header.len() / width;
    let height = u16::try_from(header_rows).map_err(|_| Error::Limit)?;
    if text.len() > MAX_BYTES || header.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let mut compressor = ZlibEncoder::new(Vec::new(), Compression::best());
    compressor
        .write_all(text.as_bytes())
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let payload = compressor
        .finish()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let rows = (payload.len() + 4).div_ceil(width);
    let total_rows = rows.checked_add(header_rows).ok_or(Error::Limit)?;
    let size = width
        .checked_mul(total_rows)
        .filter(|size| *size <= MAX_BYTES)
        .ok_or(Error::Limit)?;
    let mut pixels = header.to_vec();
    pixels[..2].copy_from_slice(&height.to_be_bytes());
    pixels.extend_from_slice(
        &u32::try_from(payload.len())
            .map_err(|_| Error::Limit)?
            .to_be_bytes(),
    );
    pixels.extend_from_slice(&payload);
    pixels.resize(size, 0);
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            &mut output,
            u32::try_from(width).map_err(|_| Error::Limit)?,
            u32::try_from(total_rows).map_err(|_| Error::Limit)?,
        );
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| Error::Invalid(e.to_string()))?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| Error::Invalid(e.to_string()))?;
    }
    Ok(output)
}
