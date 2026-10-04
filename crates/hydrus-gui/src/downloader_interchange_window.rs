//! Shared downloader exchange window: bounded files, clipboard text, and
//! concrete review before its owner stages or atomically saves the package.
use crate::DownloaderExchangeWindow;
use hydrus_gui_model::downloader_interchange::{self as model, Definition, Draft};
use hydrus_store::Store;
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    io::Read,
    path::Path,
    rc::Rc,
    sync::Arc,
};

/// An owned exchange child, cancelled with its parent editor.
#[derive(Clone, Default)]
pub struct Slots(
    pub Rc<RefCell<Option<DownloaderExchangeWindow>>>,
    pub crate::png_export_window::Slots,
);
impl Drop for Slots {
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) == 1 {
            self.cancel();
        }
    }
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExchangeSlots")
            .field(&self.0.borrow().is_some())
            .finish()
    }
}
impl Slots {
    /// Whether a child currently blocks changes to its owner.
    pub fn has_open(&self) -> bool {
        self.0.borrow().is_some()
    }
    /// Close the child and invalidate callbacks on its old window handle.
    pub fn cancel(&self) {
        let window = self
            .0
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(w) = window {
            w.invoke_action("cancel".into());
        }
        self.1.cancel();
    }
}
/// Validate a package without mutations and describe its concrete decisions.
pub type Preview<T = Definition> = Rc<dyn Fn(Vec<T>) -> Result<String, String>>;
/// Stage or save the package after the user accepts that review.
pub type Apply<T = Definition> = Rc<dyn Fn(Vec<T>) -> Result<(), String>>;

/// Open a scoped editor's exchange child. Export payloads come from its draft.
pub fn open(
    slots: &Slots,
    importing: bool,
    definitions: Vec<Definition>,
    preview: Preview,
    applied: Apply,
) -> Result<DownloaderExchangeWindow, String> {
    open_objects(
        slots,
        importing,
        definitions,
        preview,
        applied,
        Codec {
            encode_text: model::encode_text,
            decode_text: model::decode_text,
            encode_png: model::encode_png,
            decode_png: model::decode_png,
            processing: false,
        },
    )
}

struct Codec<T> {
    encode_text: fn(&[T]) -> hydrus_downloader_exchange::Result<String>,
    decode_text: fn(&str) -> hydrus_downloader_exchange::Result<Vec<T>>,
    encode_png: fn(&[T]) -> hydrus_downloader_exchange::Result<Vec<u8>>,
    decode_png: fn(&[u8]) -> hydrus_downloader_exchange::Result<Vec<T>>,
    processing: bool,
}

/// Import/export the shared processor editor's selected steps using reference
/// JSON, clipboard text or PNG. Applying appends only to the owner's draft.
pub fn open_steps(
    slots: &Slots,
    importing: bool,
    steps: Vec<hydrus_core::url::strings::ProcessingStep>,
    applied: Apply<hydrus_core::url::strings::ProcessingStep>,
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::processing;
    let preview = Rc::new(|steps: Vec<hydrus_core::url::strings::ProcessingStep>| {
        Ok(format!(
            "Append {} processing steps:\n{}\nChanges are saved only when you apply the owning editor.",
            steps.len(),
            steps
                .iter()
                .map(|step| step.describe(false, true))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    });
    open_objects(
        slots,
        importing,
        steps,
        preview,
        applied,
        Codec {
            encode_text: processing::encode_text,
            decode_text: processing::decode_text,
            encode_png: processing::encode_png,
            decode_png: processing::decode_png,
            processing: true,
        },
    )
}

/// Share a bounded reference login script package from its owning list draft.
pub fn open_login_scripts(
    slots: &Slots,
    importing: bool,
    scripts: Vec<hydrus_parse::login::LoginScript>,
    preview: Preview<hydrus_parse::login::LoginScript>,
    applied: Apply<hydrus_parse::login::LoginScript>,
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::logins;
    let window = open_objects(
        slots,
        importing,
        scripts,
        preview,
        applied,
        Codec {
            encode_text: logins::encode_text,
            decode_text: logins::decode_text,
            encode_png: logins::encode_png,
            decode_png: logins::decode_png,
            processing: false,
        },
    )?;
    window.set_window_title(
        if importing {
            "import login scripts"
        } else {
            "export login scripts"
        }
        .into(),
    );
    window.set_instructions(if importing { "Paste reference login script text or open a hydrus PNG. Review the scripts before adding them." } else { "Copy selected login scripts or save a hydrus PNG to share them." }.into());
    Ok(window)
}

/// Share selected subsidiary wrappers, including their formula and sort settings.
pub fn open_subsidiaries(
    slots: &Slots,
    importing: bool,
    parsers: Vec<hydrus_parse::content::SubsidiaryPageParser>,
    preview: Preview<hydrus_parse::content::SubsidiaryPageParser>,
    applied: Apply<hydrus_parse::content::SubsidiaryPageParser>,
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::subsidiaries;
    let window = open_objects(
        slots,
        importing,
        parsers,
        preview,
        applied,
        Codec {
            encode_text: subsidiaries::encode_text,
            decode_text: subsidiaries::decode_text,
            encode_png: subsidiaries::encode_png,
            decode_png: subsidiaries::decode_png,
            processing: false,
        },
    )?;
    window.set_window_title(
        if importing {
            "import subsidiary parsers"
        } else {
            "export subsidiary parsers"
        }
        .into(),
    );
    window.set_instructions(if importing { "Paste reference subsidiary text or open a hydrus PNG. Review the recursive wrappers before adding them." } else { "Copy selected subsidiary parsers or save a hydrus PNG to share them." }.into());
    Ok(window)
}

/// Share selected metadata routers through a caller's permitted migration context.
pub fn open_routers(
    slots: &Slots,
    importing: bool,
    routers: Vec<hydrus_parse::sidecar::Router>,
    preview: Preview<hydrus_parse::sidecar::Router>,
    applied: Apply<hydrus_parse::sidecar::Router>,
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::routers;
    let window = open_objects(
        slots,
        importing,
        routers,
        preview,
        applied,
        Codec {
            encode_text: routers::encode_text,
            decode_text: routers::decode_text,
            encode_png: routers::encode_png,
            decode_png: routers::decode_png,
            processing: false,
        },
    )?;
    window.set_window_title(
        if importing {
            "import metadata routers"
        } else {
            "export metadata routers"
        }
        .into(),
    );
    window.set_instructions(if importing { "Paste reference router text or open a hydrus PNG. Review the permitted sources and destinations before adding them." } else { "Copy selected metadata routers or save a hydrus PNG to share them." }.into());
    Ok(window)
}

/// Open a router package with the reference owned title/width PNG export child.
pub fn open_routers_with_store(
    store: &Arc<Store>,
    slots: &Slots,
    importing: bool,
    routers: Vec<hydrus_parse::sidecar::Router>,
    preview: Preview<hydrus_parse::sidecar::Router>,
    applied: Apply<hydrus_parse::sidecar::Router>,
) -> Result<DownloaderExchangeWindow, String> {
    let payload = if importing {
        None
    } else {
        Some((
            hydrus_downloader_exchange::routers::encode_text(&routers)
                .map_err(|e| e.to_string())?,
            routers.len(),
        ))
    };
    let window = open_routers(slots, importing, routers, preview, applied)?;
    if let Some((payload, count)) = payload {
        let summary = hydrus_gui_model::png_export::object_payload_description(
            &payload,
            "Metadata Single File Router",
            count,
        );
        attach_png(store, slots, &window, payload, summary);
    }
    Ok(window)
}

/// Open a subsidiary package with the reference owned title/width PNG export child.
pub fn open_subsidiaries_with_store(
    store: &Arc<Store>,
    slots: &Slots,
    importing: bool,
    parsers: Vec<hydrus_parse::content::SubsidiaryPageParser>,
    preview: Preview<hydrus_parse::content::SubsidiaryPageParser>,
    applied: Apply<hydrus_parse::content::SubsidiaryPageParser>,
) -> Result<DownloaderExchangeWindow, String> {
    let payload = if importing {
        None
    } else {
        Some((
            hydrus_downloader_exchange::subsidiaries::encode_text(&parsers)
                .map_err(|e| e.to_string())?,
            parsers.len(),
        ))
    };
    let window = open_subsidiaries(slots, importing, parsers, preview, applied)?;
    if let Some((payload, count)) = payload {
        let summary = hydrus_gui_model::png_export::object_payload_description(
            &payload,
            "Subsidiary Page Parser",
            count,
        );
        attach_png(store, slots, &window, payload, summary);
    }
    Ok(window)
}

fn attach_png(
    store: &Arc<Store>,
    slots: &Slots,
    window: &DownloaderExchangeWindow,
    payload: String,
    summary: String,
) {
    window.set_png_enabled(true);
    window.on_export_png({
        let weak = window.as_weak();
        let store = store.clone();
        let png = slots.1.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !window.get_active() || png.has_open() {
                return;
            }
            let closed = Rc::new({
                let weak = weak.clone();
                move || {
                    if let Some(window) = weak.upgrade()
                        && window.get_active()
                    {
                        window.set_png_child(false);
                    }
                }
            });
            match crate::png_export_window::open_with_summary(
                &png,
                &store,
                payload.clone(),
                summary.clone(),
                closed,
            ) {
                Ok(_) => window.set_png_child(true),
                Err(error) => window.set_error(error.into()),
            }
        }
    });
}

fn open_objects<T: Clone + 'static>(
    slots: &Slots,
    importing: bool,
    definitions: Vec<T>,
    preview: Preview<T>,
    applied: Apply<T>,
    codec: Codec<T>,
) -> Result<DownloaderExchangeWindow, String> {
    if let Some(w) = slots.0.borrow().as_ref() {
        return Ok(w.clone_strong());
    }
    let w = DownloaderExchangeWindow::new().map_err(|e| e.to_string())?;
    w.set_importing(importing);
    w.set_active(true);
    if codec.processing {
        w.set_window_title(
            if importing {
                "import processing steps"
            } else {
                "export processing steps"
            }
            .into(),
        );
        w.set_instructions(if importing { "Paste reference processing-step text or open a hydrus PNG. Review the steps before appending them." } else { "Copy selected processing steps in queue order or save a hydrus PNG to share them." }.into());
    }
    if !importing {
        w.set_text(
            (codec.encode_text)(&definitions)
                .map_err(|e| e.to_string())?
                .into(),
        );
    }
    let active = Rc::new(Cell::new(true));
    let pending = Rc::new(RefCell::new(None::<Vec<T>>));
    let close: Rc<dyn Fn()> = Rc::new({
        let active = active.clone();
        let slot = Rc::downgrade(&slots.0);
        let weak = w.as_weak();
        let png = slots.1.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                w.set_active(false);
                w.set_png_child(false);
                let _ = w.hide();
            }
            png.cancel();
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(w) = weak.upgrade() {
                w.invoke_closed();
            }
        }
    });
    w.on_action({
        let weak = w.as_weak();
        let active = active.clone();
        let close = close.clone();
        move |action| {
            if !active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            if w.get_png_child() && action != "cancel" {
                return;
            }
            let result = (|| -> Result<(), String> {
                match action.as_str() {
                    "cancel" => close(),
                    "back" => {
                        pending.borrow_mut().take();
                        w.set_ready(false);
                        w.set_review("".into());
                    }
                    "paste" => w.set_text(crate::from_clipboard()?.into()),
                    "copy" => crate::copy_to_clipboard(w.get_text().as_str()),
                    "browse" => {
                        let dialog = rfd::FileDialog::new()
                            .add_filter("Hydrus downloader definitions", &["png", "json", "txt"]);
                        let path = if importing {
                            dialog.pick_file()
                        } else {
                            dialog.set_file_name("downloaders.png").save_file()
                        };
                        if let Some(path) = path {
                            w.set_path(path.to_string_lossy().as_ref().into());
                        }
                    }
                    "save" => {
                        let path = w.get_path();
                        if path.is_empty() {
                            return Err("Choose an export path first.".into());
                        }
                        let data = (codec.encode_png)(&definitions).map_err(|e| e.to_string())?;
                        let parent = Path::new(path.as_str())
                            .parent()
                            .filter(|parent| !parent.as_os_str().is_empty())
                            .unwrap_or_else(|| Path::new("."));
                        let mut file =
                            tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
                        std::io::Write::write_all(&mut file, &data).map_err(|e| e.to_string())?;
                        file.persist(path.as_str()).map_err(|e| e.to_string())?;
                        w.set_review("PNG saved.".into());
                    }
                    "open" | "review" => {
                        let definitions = if action == "open" {
                            read_file(w.get_path().as_str(), &codec)?
                        } else {
                            (codec.decode_text)(w.get_text().as_str()).map_err(|e| e.to_string())?
                        };
                        let description = preview(definitions.clone())?;
                        w.set_review(description.into());
                        *pending.borrow_mut() = Some(definitions);
                        w.set_ready(true);
                    }
                    "accept" => {
                        let definitions = pending
                            .borrow()
                            .clone()
                            .ok_or_else(|| "Review a package before importing.".to_owned())?;
                        applied(definitions)?;
                        close();
                    }
                    _ => (),
                }
                Ok(())
            })();
            w.set_error(result.err().unwrap_or_default().into());
        }
    });
    w.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    w.show().map_err(|e| e.to_string())?;
    *slots.0.borrow_mut() = Some(w.clone_strong());
    Ok(w)
}
fn read_file<T>(path: &str, codec: &Codec<T>) -> Result<Vec<T>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((hydrus_downloader_exchange::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.starts_with(b"\x89PNG") {
        (codec.decode_png)(&bytes).map_err(|e| e.to_string())
    } else {
        (codec.decode_text)(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
}
/// Network menu import/export over a consistent full settings snapshot.
pub fn package(
    store: &Arc<Store>,
    slots: &Slots,
    importing: bool,
) -> Result<DownloaderExchangeWindow, String> {
    let draft = Draft::load(store).map_err(|e| e.to_string())?;
    let definitions = draft.definitions();
    let preview: Preview = Rc::new({
        let draft = draft.clone();
        move |definitions| {
            let mut next = draft.clone();
            next.import(definitions).map(|r| {
                r.text().replace(
                    "Changes are saved only when you apply the owning editor.",
                    "Click import to save this package.",
                )
            })
        }
    });
    let applied: Apply = Rc::new({
        let store = store.clone();
        move |definitions| {
            let mut next = draft.clone();
            next.import(definitions)?;
            next.save(&store).map_err(|e| e.to_string())
        }
    });
    open(slots, importing, definitions, preview, applied)
}
