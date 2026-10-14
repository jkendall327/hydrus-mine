//! Help > debug > "what is this object?" (`ReviewWhatIsThisObject`): paste
//! (or drop, or choose) hydrus serialised objects, as JSON text, JSON files,
//! hydrus PNGs or a bitmap on the clipboard, and see what each is
//! (`hydrus_legacy::serialisable::what_is_this`).
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use hydrus_core::numbers::human_int;
use slint::ComponentHandle as _;

use crate::WhatIsThisWindow;

thread_local! {
    /// The windows opened, kept alive (the reference opens a frame each time).
    static OPEN: RefCell<Vec<WhatIsThisWindow>> = const { RefCell::new(Vec::new()) };
}

/// The windows opened so far (for the tests).
pub fn windows() -> Vec<WhatIsThisWindow> {
    OPEN.with(|open| {
        open.borrow()
            .iter()
            .map(WhatIsThisWindow::clone_strong)
            .collect()
    })
}

/// A payload's description (`PNG "path"`, `JSON "path"`, ...) and its text.
type Payload = (String, String);

/// Python's wording of the problem reading a file.
fn io_message(path: &Path, error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => format!(
            "[Errno 2] No such file or directory: {}",
            hydrus_core::pyjson::python_str_repr(&path.to_string_lossy())
        ),
        std::io::ErrorKind::PermissionDenied => format!(
            "[Errno 13] Permission denied: {}",
            hydrus_core::pyjson::python_str_repr(&path.to_string_lossy())
        ),
        _ => error.to_string(),
    }
}

/// `utf-8` decoding as Python reports its failure.
fn utf8_message(error: &std::str::Utf8Error, bytes: &[u8]) -> String {
    let at = error.valid_up_to();
    let byte = bytes.get(at).copied().unwrap_or(0);
    // (a byte that can begin no sequence, or one that begins a sequence
    // whose next byte is wrong)
    let why = if error.error_len().is_none() {
        "unexpected end of data"
    } else if (0xC2..=0xF4).contains(&byte) {
        "invalid continuation byte"
    } else {
        "invalid start byte"
    };
    format!("'utf-8' codec can't decode byte 0x{byte:02x} in position {at}: {why}")
}

/// `IngestPaths`: a PNG is read as a hydrus PNG, anything else as JSON text.
fn ingest_paths(paths: &[PathBuf]) -> Result<Vec<Payload>, String> {
    let mut payloads = Vec::new();
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| io_message(path, &e))?;
        let shown = path.to_string_lossy();
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            let text = hydrus_downloader_exchange::text_png::decode(&bytes).map_err(|e| {
                format!("Problem loading PNG at path \"{shown}\"! Full error written to log; {e}")
            })?;
            payloads.push((format!("PNG \"{shown}\""), text));
        } else {
            let text = std::str::from_utf8(&bytes).map_err(|e| {
                format!(
                    "Problem loading assumed JSON at path \"{shown}\"! Full error written to log; {}",
                    utf8_message(&e, &bytes)
                )
            })?;
            payloads.push((format!("JSON \"{shown}\""), text.to_owned()));
        }
    }
    Ok(payloads)
}

/// `IngestClipboard`: a bitmap (a hydrus PNG copied), file paths, else text.
fn ingest_clipboard() -> Result<Vec<Payload>, String> {
    if let Some(image) = crate::clipboard_image() {
        let first_channel: Vec<u8> = image.rgba.chunks_exact(4).map(|p| p[0]).collect();
        let text = hydrus_downloader_exchange::text_png::decode_pixels(&first_channel, image.width)
            .map_err(|e| format!("Problem pasting image! Full error written to log; {e}"))?;
        return Ok(vec![("clipboard image data".into(), text)]);
    }
    let paths = crate::clipboard_paths();
    if !paths.is_empty() {
        return ingest_paths(&paths);
    }
    let text = crate::from_clipboard()
        .map_err(|e| format!("Problem pasting text! Full error written to log; {e}"))?;
    Ok(vec![("clipboard text data".into(), text)])
}

/// "problem importing!": the reference's critical dialog.
fn problem(error: &str) {
    crate::debug_actions::message(
        "problem importing!",
        &format!("Sorry, there was a problem importing those paths! Error was: {error}"),
    );
}

/// `_ImportPayloads`: what each is, and the texts.
fn import_payloads(window: &WhatIsThisWindow, payloads: &[Payload]) {
    let results: Vec<String> = payloads
        .iter()
        .map(|(_, text)| hydrus_legacy::serialisable::what_is_this(text))
        .collect();
    window.set_description(
        format!(
            "{} items\n\n{}",
            human_int(results.len() as u64),
            results.join("\n\n")
        )
        .into(),
    );
    let texts: Vec<&str> = payloads.iter().map(|(_, text)| text.as_str()).collect();
    window.set_json_content(texts.join("\n\n").into());
}

fn import_paths(window: &WhatIsThisWindow, paths: &[PathBuf]) {
    match ingest_paths(paths) {
        Ok(payloads) => import_payloads(window, &payloads),
        Err(error) => problem(&error),
    }
}

/// Open the window.
///
/// # Errors
/// If the window cannot be made.
pub fn open() -> Result<WhatIsThisWindow, slint::PlatformError> {
    let window = crate::app_title::new::<WhatIsThisWindow>()?;
    let notice = Rc::new(slint::Timer::default());
    window.on_paste({
        let weak = window.as_weak();
        let notice = notice.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            match ingest_clipboard() {
                Ok(payloads) => {
                    import_payloads(&window, &payloads);
                    // (the reference's micro-notification, for three seconds)
                    window.set_notice(
                        format!("Pasted {} items!", human_int(payloads.len() as u64)).into(),
                    );
                    let weak = window.as_weak();
                    notice.start(
                        slint::TimerMode::SingleShot,
                        Duration::from_secs(3),
                        move || {
                            if let Some(window) = weak.upgrade() {
                                window.set_notice("".into());
                            }
                        },
                    );
                }
                Err(error) => problem(&error),
            }
        }
    });
    window.on_picture_clicked({
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let paths = crate::pick_exchange_files("Select the pngs to add.", "png");
            if !paths.is_empty() {
                import_paths(&window, &paths);
            }
        }
    });
    crate::drops::on_files_dropped(window.window(), {
        let weak = window.as_weak();
        move |paths| {
            if let Some(window) = weak.upgrade() {
                let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
                import_paths(&window, &paths);
            }
        }
    });
    window.show()?;
    OPEN.with(|open| open.borrow_mut().push(window.clone_strong()));
    Ok(window)
}

/// What a drop of `paths` does (the drop target is winit's: tests call this).
pub fn drop_paths(window: &WhatIsThisWindow, paths: &[PathBuf]) {
    import_paths(window, paths);
}
