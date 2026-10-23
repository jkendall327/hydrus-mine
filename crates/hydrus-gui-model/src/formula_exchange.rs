//! The formula panel's "export" and "import" (`EditFormulaPanel`
//! `ExportToClipboard`, `ExportToPNG`, `ImportFromClipboard`,
//! `ImportFromPNG`, `_ImportObject`): the formula goes out as the
//! reference's serialised text, and a formula coming in replaces the one
//! held at once; anything else is refused with the reference's words.

use hydrus_downloader_exchange::{Definition, Native};
use hydrus_parse::formula::Formula;

use crate::auto_resolution_exchange::Message;

/// What the clipboard import is expecting, in its error.
pub const CLIPBOARD_EXPECTED: &str = "JSON-serialised Formula";

/// The png picker's title (`ImportFromPNG`).
pub const PNG_PICKER_TITLE: &str = "select the png with the encoded formula";

/// The serialisable types of the reference's formulae (HTML, JSON, zipper,
/// context variable, nested, static).
const FORMULA_TYPES: [u16; 6] = [27, 31, 59, 60, 133, 136];

/// The formula's export: the reference's serialised text.
///
/// # Errors
/// If the formula can't be written in the reference's form.
pub fn export_text(formula: &Formula) -> Result<String, String> {
    hydrus_downloader_exchange::encode_text(&[Definition::new(Native::Formula(formula.clone()))])
        .map_err(|e| e.to_string())
}

/// Why imported text did not give a formula.
enum Refused {
    /// Not a serialised object (or one hydrus-rs can't read).
    Unreadable(String),
    /// An object that is not a formula: its type, as the reference names it.
    NotAFormula(String),
}

fn formula_of(text: &str) -> Result<Formula, Refused> {
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(text.trim())
        .map_err(|e| Refused::Unreadable(e.to_string()))?;
    let kind = object.kind.0;
    if !FORMULA_TYPES.contains(&kind) {
        return Err(Refused::NotAFormula(
            hydrus_downloader_exchange::subscription_import::type_name(kind)
                .unwrap_or_else(|| format!("serialisable type {kind}")),
        ));
    }
    let mut definitions = hydrus_downloader_exchange::decode_text(text)
        .map_err(|e| Refused::Unreadable(e.to_string()))?;
    match definitions.pop().map(|d| d.native) {
        Some(Native::Formula(formula)) if definitions.is_empty() => Ok(formula),
        _ => Err(Refused::Unreadable("not one formula".into())),
    }
}

fn not_a_formula(name: &str) -> Message {
    (
        "Warning".into(),
        format!("That was not a formula--it was a: {name}"),
    )
}

/// The formula pasted (`ImportFromClipboard`), or what to say instead.
/// `Ok(None)` is an empty clipboard (Qt's `''`); an error, a clipboard that
/// could not be read (the reference's `DataMissing`).
///
/// # Errors
/// The message to show, if no formula came in.
pub fn from_clipboard(clipboard: Result<Option<String>, String>) -> Result<Formula, Message> {
    let text = match clipboard {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => return Err(("Problem importing!".into(), e)),
    };
    formula_of(&text).map_err(|refused| match refused {
        Refused::NotAFormula(name) => not_a_formula(&name),
        Refused::Unreadable(error) => {
            crate::auto_resolution_exchange::clipboard_error(CLIPBOARD_EXPECTED, &text, &error)
        }
    })
}

/// The formula from a png file (`ImportFromPNG`), or what to say instead.
///
/// # Errors
/// The message to show, if no formula came in.
pub fn from_png(path: &std::path::Path, data: &[u8]) -> Result<Formula, Message> {
    let problem = |text: String| ("Problem loading!".to_owned(), text);
    let text = hydrus_downloader_exchange::text_png::decode(data).map_err(|e| {
        if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            problem(e.to_string())
        } else {
            // (`LoadFromPNG`, when the file is no image)
            problem(format!(
                "\"{}\" did not appear to be a valid image!",
                path.display()
            ))
        }
    })?;
    formula_of(&text).map_err(|refused| match refused {
        Refused::NotAFormula(name) => not_a_formula(&name),
        Refused::Unreadable(_) => {
            problem("I could not understand what was encoded in the png!".into())
        }
    })
}
