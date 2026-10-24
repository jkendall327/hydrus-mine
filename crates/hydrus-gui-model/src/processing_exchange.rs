//! The string processor editor's step queue's "export", "import" and
//! "duplicate" (`QueueListBox.AddImportExportButtons`): the selected steps
//! go out as the reference's serialised text (one, or a list, in queue
//! order), and what comes in from the clipboard or png files is appended at
//! once, with the reference's messages.

use hydrus_core::url::strings::ProcessingStep;

use crate::auto_resolution_exchange::{CLIPBOARD_EXPECTED, ListImport, PngFile, clipboard_error};

/// The png picker's title (`_ImportFromPNG`).
pub const PNG_PICKER_TITLE: &str = "select the png or pngs with the encoded data";

/// The selected steps' export.
///
/// # Errors
/// If a step can't be written in the reference's form.
pub fn export_text(steps: &[ProcessingStep]) -> Result<String, String> {
    hydrus_downloader_exchange::processing::encode_text(steps).map_err(|e| e.to_string())
}

/// What a text brings (`_ImportObject`): its steps, the reference's warning
/// for the other types it held, and the count added.
fn read(text: &str) -> Result<ListImport<ProcessingStep>, String> {
    let package =
        hydrus_downloader_exchange::processing::decode_package(text).map_err(|e| e.to_string())?;
    let mut messages = Vec::new();
    if let Some(warning) = package.warning() {
        messages.push(("Warning".to_owned(), warning));
    }
    if !package.steps.is_empty() {
        messages.push((
            "Information".to_owned(),
            crate::auto_resolution_exchange::added(package.steps.len()),
        ));
    }
    Ok(ListImport {
        added: package.steps,
        messages,
    })
}

/// Steps pasted (`_ImportFromClipboard`). `Ok(None)` is an empty clipboard
/// (Qt's `''`); an error, one that could not be read (the reference's
/// `DataMissing`).
pub fn from_clipboard(clipboard: Result<Option<String>, String>) -> ListImport<ProcessingStep> {
    let text = match clipboard {
        Ok(text) => text.unwrap_or_default(),
        Err(e) => {
            return ListImport {
                added: Vec::new(),
                messages: vec![("Problem pasting!".into(), e)],
            };
        }
    };
    read(&text).unwrap_or_else(|e| ListImport {
        added: Vec::new(),
        messages: vec![clipboard_error(CLIPBOARD_EXPECTED, &text, &e)],
    })
}

/// Steps from png files (`_ImportFromPNG`).
pub fn from_pngs(files: &[PngFile]) -> ListImport<ProcessingStep> {
    crate::auto_resolution_exchange::from_png_files(files, |text| read(text).ok())
}

/// A duplicate of the selected (`_Duplicate`): copies, said as an import.
pub fn duplicated(selected: &[ProcessingStep]) -> ListImport<ProcessingStep> {
    crate::auto_resolution_exchange::duplicated(selected)
}
