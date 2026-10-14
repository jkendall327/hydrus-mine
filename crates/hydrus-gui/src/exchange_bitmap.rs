//! A bitmap on the clipboard, as the reference's exchange panels take it: its
//! pixels are a hydrus PNG, so it is read as the PNG file it would have been
//! (the reference's `LoadFromQtImage` takes the first channel of the pixels,
//! as its `LoadFromPNG` does).

/// The clipboard's bitmap as PNG bytes, if it holds one; the error is the
/// reference's "Problem loading from clipboard".
pub(crate) fn clipboard_png() -> Option<Result<Vec<u8>, String>> {
    let image = crate::clipboard_image()?;
    Some(png_of(&image).map_err(|e| format!("Problem loading from clipboard: {e}")))
}

fn png_of(image: &crate::ClipboardImage) -> Result<Vec<u8>, String> {
    let width = u32::try_from(image.width).map_err(|e| e.to_string())?;
    let height = u32::try_from(image.height).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&image.rgba))
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// The reference's account of an image that is not a hydrus serialised PNG
/// (`LoadFromNumPyImage`), for the decoder's complaint about its pixels;
/// `None` for any other failure.
pub(crate) fn not_a_hydrus_png(error: &str) -> Option<String> {
    let reason = if error.contains("PNG header is missing") {
        "Header bytes were invalid!"
    } else if error.contains("PNG header height is outside the image") {
        "Payload bytes were invalid!"
    } else {
        return None;
    };
    Some(format!(
        "The image loaded, but it did not seem to be a hydrus serialised png! The error was: Exception('{reason}')\n\nIf you believe this is a legit non-resized, non-converted hydrus serialised png, please send it to hydrus_dev."
    ))
}
