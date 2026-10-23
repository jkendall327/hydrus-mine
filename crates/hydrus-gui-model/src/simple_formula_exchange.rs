//! The saved simple downloader formula list's "export", "import" and
//! "duplicate" (`AddEditDeleteListBoxUniqueNamedObjects` with
//! `AddImportExportButtons`): the selected formulae go out as the
//! reference's serialised text (one, or a list), and what comes in from the
//! clipboard or png files is appended under unique names, with the
//! reference's messages.

use std::collections::BTreeSet;

use hydrus_downloader_exchange::{Definition, Native};
use hydrus_legacy::serialisable::{Body, Meta, SerialisableObject};
use hydrus_parse::simple::SimpleFormula;

use crate::auto_resolution_exchange::{CLIPBOARD_EXPECTED, ListImport, Message, clipboard_error};

/// The serialisable type of a simple downloader formula.
const SIMPLE_FORMULA: u16 = 63;
const LIST: u16 = 26;

/// What the list allows, in its refusal.
const ALLOWED: &str = "SimpleDownloaderParsingFormula";

/// The png picker's title (`_ImportFromPNG`).
pub const PNG_PICKER_TITLE: &str = "select the png or pngs with the encoded data";

/// The selected formulae's export: the reference's text of one, or of a
/// list of them.
///
/// # Errors
/// If a formula can't be written in the reference's form.
pub fn export_text(formulae: &[SimpleFormula]) -> Result<String, String> {
    let definitions: Vec<Definition> = formulae
        .iter()
        .map(|f| Definition::new(Native::Simple(f.clone())))
        .collect();
    hydrus_downloader_exchange::encode_text(&definitions).map_err(|e| e.to_string())
}

/// What a text brought: its formulae, and the types it held that the list
/// does not take.
fn read(text: &str) -> Result<(Vec<SimpleFormula>, BTreeSet<String>), String> {
    let object = SerialisableObject::from_tuple_str(text.trim()).map_err(|e| e.to_string())?;
    let mut formulae = Vec::new();
    let mut refused = BTreeSet::new();
    let objects: Vec<&SerialisableObject> = match (object.kind.0, &object.body) {
        (LIST, Body::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                Meta::Object(o) => Some(&**o),
                _ => None,
            })
            .collect(),
        _ => vec![&object],
    };
    for o in objects {
        let kind = o.kind.0;
        if kind != SIMPLE_FORMULA {
            refused.insert(
                hydrus_downloader_exchange::subscription_import::type_name(kind)
                    .unwrap_or_else(|| format!("serialisable type {kind}")),
            );
            continue;
        }
        let mut definitions = hydrus_downloader_exchange::decode_text(&o.to_tuple_string())
            .map_err(|e| e.to_string())?;
        match definitions.pop().map(|d| d.native) {
            Some(Native::Simple(f)) => formulae.push(f),
            _ => return Err("not a simple downloader formula".into()),
        }
    }
    Ok((formulae, refused))
}

fn imported(formulae: Vec<SimpleFormula>, refused: &BTreeSet<String>) -> ListImport<SimpleFormula> {
    let mut messages: Vec<Message> = Vec::new();
    if !refused.is_empty() {
        messages.push((
            "Warning".into(),
            crate::auto_resolution_exchange::refused_message_for(refused, ALLOWED),
        ));
    }
    if !formulae.is_empty() {
        messages.push((
            "Information".into(),
            crate::auto_resolution_exchange::added(formulae.len()),
        ));
    }
    ListImport {
        added: formulae,
        messages,
    }
}

/// Formulae pasted (`_ImportFromClipboard`). `Ok(None)` is an empty
/// clipboard (Qt's `''`); an error, one that could not be read (the
/// reference's `DataMissing`).
pub fn from_clipboard(clipboard: Result<Option<String>, String>) -> ListImport<SimpleFormula> {
    let text = match clipboard {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => {
            return ListImport {
                added: Vec::new(),
                messages: vec![("Problem pasting!".into(), e)],
            };
        }
    };
    match read(&text) {
        Ok((formulae, refused)) => imported(formulae, &refused),
        Err(e) => ListImport {
            added: Vec::new(),
            messages: vec![clipboard_error(CLIPBOARD_EXPECTED, &text, &e)],
        },
    }
}

/// Formulae from png files (`_ImportFromPNG`), each file an import of its
/// own; the first that can't be read stops the rest.
pub fn from_pngs(files: &[crate::auto_resolution_exchange::PngFile]) -> ListImport<SimpleFormula> {
    crate::auto_resolution_exchange::from_png_files(files, |text| {
        read(text)
            .ok()
            .map(|(formulae, refused)| imported(formulae, &refused))
    })
}

/// A duplicate of the selected (`_Duplicate`).
pub fn duplicated(selected: &[SimpleFormula]) -> ListImport<SimpleFormula> {
    imported(selected.to_vec(), &BTreeSet::new())
}
