//! Reference string PNG export parameters and visible grayscale headers.
use hydrus_store::settings::{GuiFormatting, Setting};
use serde::{Deserialize, Serialize};

/// Remember the last successful PNG export folder, as the reference picker does.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Directory(pub Option<String>);
impl Setting for Directory {
    const KEY: &'static str = "last_png_export_dir";
}

/// `GetPayloadDescriptionAndBytes` describes string length in characters.
pub fn payload_description(payload: &str) -> String {
    payload_description_with_format(payload, &GuiFormatting::default())
}
pub fn payload_description_with_format(payload: &str, formatting: &GuiFormatting) -> String {
    format!(
        "String - {}",
        crate::gui_format::bytes(formatting, payload.chars().count() as u64)
    )
}

/// The reference describes selected serialisable objects by type and count.
pub fn object_payload_description(payload: &str, object_type: &str, count: usize) -> String {
    object_payload_description_with_format(payload, object_type, count, &GuiFormatting::default())
}
pub fn object_payload_description_with_format(
    payload: &str,
    object_type: &str,
    count: usize,
    formatting: &GuiFormatting,
) -> String {
    let kind = if count == 1 {
        object_type.to_owned()
    } else {
        format!(
            "A list of {} {object_type}",
            hydrus_core::numbers::human_int(count as u64)
        )
    };
    format!(
        "{kind} - {}",
        crate::gui_format::bytes(formatting, payload.chars().count() as u64)
    )
}

/// Invalid exports stay open and do not write their chosen path.
pub fn validate(path: &str, title: &str, width: i32) -> Result<(), String> {
    let mut problems = Vec::new();
    if path.is_empty() {
        problems.push("select a path");
    }
    let parent = std::path::Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    if path.is_empty() || !parent.is_dir() {
        problems.push("please select a directory that exists");
    }
    if title.is_empty() {
        problems.push("set a title");
    }
    if !(100..=4096).contains(&width) {
        problems.push("set width between 100 and 4096");
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join(" and "))
    }
}

fn xml(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Encode compressed UTF-8 string sources with a readable title/description
/// header. Native fonts/rasterisation differ from Qt; carrier bytes are exact.
pub fn encode(
    payload: &str,
    width: u32,
    title: &str,
    description: &str,
) -> Result<Vec<u8>, String> {
    encode_with_summary(
        payload,
        width,
        title,
        &payload_description(payload),
        description,
    )
}

/// Typed editor payloads retain their reference type/count summary in the header.
pub fn encode_with_summary(
    payload: &str,
    width: u32,
    title: &str,
    summary: &str,
    description: &str,
) -> Result<Vec<u8>, String> {
    if !(100..=4096).contains(&width) {
        return Err("Set width between 100 and 4096.".into());
    }
    if title.chars().count() > 4096
        || description.chars().count() > 4096
        || summary.chars().count() > 4096
    {
        return Err("PNG title and description must each contain at most 4096 characters.".into());
    }
    let mut y = 12_u32;
    let mut texts = String::new();
    for (text, size) in [(title, 24_u32), (summary, 17), (description, 12)] {
        let count = usize::try_from((width - 20) / (size * 3 / 5))
            .map_err(|e| e.to_string())?
            .max(1);
        let chars: Vec<char> = text.chars().collect();
        if !chars.is_empty() {
            y += 10;
        }
        for line in chars.chunks(count) {
            y += size + 4;
            let line: String = line.iter().collect();
            texts.push_str(&format!("<text x=\"{}\" y=\"{y}\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"{size}\" fill=\"black\">{}</text>", width / 2, xml(&line)));
        }
    }
    let height = y + 12;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>{texts}</svg>"
    );
    let raster = hydrus_media::render_svg(svg.as_bytes(), (width, height))
        .ok_or_else(|| "Could not render the PNG header.".to_owned())?;
    let header: Vec<u8> = raster.data().chunks_exact(4).map(|p| p[0]).collect();
    hydrus_downloader_exchange::text_png::encode(payload, width, &header).map_err(|e| e.to_string())
}

/// What `PNGExportPanel` starts with for a serialised object's text: its
/// payload description (`GetPayloadDescriptionAndBytes`: the object's
/// serialisable name, or "A list of" counts of them, and the text's
/// length) and its title (a named object's name, else that description).
/// `None` if the text is not a serialised object.
pub fn serialised_payload_description(
    payload: &str,
    formatting: &GuiFormatting,
) -> Option<(String, String)> {
    use hydrus_legacy::serialisable::{Body, Meta, SerialisableObject, class_name};
    let object = SerialisableObject::from_tuple_str(payload).ok()?;
    let kind = match &object.body {
        Body::List(items) if object.kind.0 == 26 => {
            // (`collections.Counter`: by first appearance)
            let mut counts: Vec<(&str, u64)> = Vec::new();
            for item in items {
                let name = match item {
                    Meta::Object(o) => class_name(o.kind.0)?,
                    _ => return None,
                };
                match counts.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, count)) => *count += 1,
                    None => counts.push((name, 1)),
                }
            }
            let listed: Vec<String> = counts
                .iter()
                .map(|(name, count)| format!("{} {name}", hydrus_core::numbers::human_int(*count)))
                .collect();
            format!("A list of {}", listed.join(", "))
        }
        _ => class_name(object.kind.0)?.to_owned(),
    };
    let description = format!(
        "{kind} - {}",
        crate::gui_format::bytes(formatting, payload.chars().count() as u64)
    );
    let title = object.name.clone().unwrap_or_else(|| description.clone());
    Some((description, title))
}
