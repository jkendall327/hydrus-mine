//! Reference raw parsing data previews; the document remains separate from display text.
use hydrus_core::{Mime, pyjson::PyJson};
use std::{collections::BTreeMap, io::Write as _};

/// The reference test panel's character limit, applied after JSON formatting.
pub const PREVIEW_CHARS: usize = 500_000;
/// What the read-only raw data panel displays and whether test parse is available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub description: String,
    pub text: String,
    pub parse_enabled: bool,
}
fn looks_like_html(text: &str) -> bool {
    ["<html", "<HTML", "<!DOCTYPE html", "<!DOCTYPE HTML"]
        .iter()
        .any(|needle| text.contains(needle))
}
fn indent(out: &mut String, depth: usize) {
    out.push_str(&" ".repeat(depth * 4));
}
fn pretty(value: &PyJson, out: &mut String, depth: usize) {
    match value {
        PyJson::List(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                indent(out, depth + 1);
                pretty(item, out, depth + 1);
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push(']');
        }
        PyJson::Object(items) if !items.is_empty() => {
            out.push_str("{\n");
            for (index, (key, item)) in items.iter().enumerate() {
                indent(out, depth + 1);
                out.push_str(&PyJson::Str(key.clone()).to_python_string());
                out.push_str(": ");
                pretty(item, out, depth + 1);
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            indent(out, depth);
            out.push('}');
        }
        _ => out.push_str(&value.to_python_string()),
    }
}
/// Inspect fetched bytes through the existing media engine, off the GUI thread.
/// HTML/JSON takes precedence over a file signature, as in the reference.
pub fn detect_mime(text: &str, bytes: &[u8]) -> Option<Mime> {
    detect_mime_with_tools(text, bytes, &hydrus_media::MediaTools::new())
}

/// Detect with a caller-owned Store policy, preserving the standalone API.
pub fn detect_mime_with_tools(
    text: &str,
    bytes: &[u8],
    tools: &hydrus_media::MediaTools,
) -> Option<Mime> {
    if text.is_empty() || PyJson::parse(text).is_ok() || looks_like_html(text) {
        return None;
    }
    let mut file = tempfile::NamedTempFile::new().ok()?;
    file.write_all(bytes).ok()?;
    tools
        .detect_mime(file.path())
        .ok()
        .filter(|mime| hydrus_media::mimes::is_allowed(*mime))
}
/// Format the displayed preview without changing the parser/clipboard document.
pub fn preview(text: &str, mime: Option<Mime>) -> Preview {
    preview_with_format(
        text,
        mime,
        &hydrus_store::settings::GuiFormatting::default(),
    )
}
pub fn preview_with_format(
    text: &str,
    mime: Option<Mime>,
    formatting: &hydrus_store::settings::GuiFormatting,
) -> Preview {
    if text.is_empty() {
        return Preview {
            description: "no example data set yet".into(),
            text: String::new(),
            parse_enabled: false,
        };
    }
    let json = PyJson::parse(text).ok();
    let mut shown = text.to_owned();
    let mut description = if let Some(json) = json {
        shown.clear();
        pretty(&json, &mut shown, 0);
        format!(
            "{} total, looks like JSON",
            crate::gui_format::bytes(formatting, text.chars().count() as u64)
        )
    } else if looks_like_html(text) {
        format!(
            "{} total, looks like HTML",
            crate::gui_format::bytes(formatting, text.chars().count() as u64)
        )
    } else if let Some(mime) = mime.filter(|mime| hydrus_media::mimes::is_allowed(*mime)) {
        return Preview {
            description: format!("That looked like a {}!", mime.human_name()),
            text: "no preview".into(),
            parse_enabled: false,
        };
    } else {
        "That did not look like a full HTML document, nor JSON, but will try to show it anyway"
            .into()
    };
    if shown.chars().count() > PREVIEW_CHARS {
        description
            .push_str("\nThe data was more than 500,000 characters. Clipping what is shown.");
        shown = shown.chars().take(PREVIEW_CHARS).collect();
    }
    Preview {
        description,
        text: shown,
        parse_enabled: true,
    }
}
/// Detected media belongs only to a fetched example, never to inherited child text.
#[derive(Debug, Clone, Default)]
pub struct ExampleMimes(BTreeMap<usize, (String, Mime)>);
impl ExampleMimes {
    /// Retain byte inspection only while this example's raw document is unchanged.
    pub fn remember(&mut self, index: usize, text: &str, mime: Option<Mime>) {
        if let Some(mime) = mime {
            self.0.insert(index, (text.into(), mime));
        } else {
            self.0.remove(&index);
        }
    }
    pub fn get(&self, index: usize, text: &str) -> Option<Mime> {
        self.0
            .get(&index)
            .filter(|(original, _)| original == text)
            .map(|(_, mime)| *mime)
    }
    /// Align metadata with the remaining selectable examples.
    pub fn remove(&mut self, index: usize) {
        self.0 = std::mem::take(&mut self.0)
            .into_iter()
            .filter_map(|(i, data)| {
                if i == index {
                    None
                } else {
                    Some((if i > index { i - 1 } else { i }, data))
                }
            })
            .collect();
    }
}
