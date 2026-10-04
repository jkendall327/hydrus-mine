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
pub struct Slots(pub Rc<RefCell<Option<DownloaderExchangeWindow>>>);
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
    }
}
/// Validate a package without mutations and describe its concrete decisions.
pub type Preview = Rc<dyn Fn(Vec<Definition>) -> Result<String, String>>;
/// Stage or save the package after the user accepts that review.
pub type Apply = Rc<dyn Fn(Vec<Definition>) -> Result<(), String>>;

/// Open a scoped editor's exchange child. Export payloads come from its draft.
pub fn open(
    slots: &Slots,
    importing: bool,
    definitions: Vec<Definition>,
    preview: Preview,
    applied: Apply,
) -> Result<DownloaderExchangeWindow, String> {
    if let Some(w) = slots.0.borrow().as_ref() {
        return Ok(w.clone_strong());
    }
    let w = DownloaderExchangeWindow::new().map_err(|e| e.to_string())?;
    w.set_importing(importing);
    if !importing {
        w.set_text(
            model::encode_text(&definitions)
                .map_err(|e| e.to_string())?
                .into(),
        );
    }
    let active = Rc::new(Cell::new(true));
    let pending = Rc::new(RefCell::new(None::<Vec<Definition>>));
    let close: Rc<dyn Fn()> = Rc::new({
        let active = active.clone();
        let slot = slots.0.clone();
        let weak = w.as_weak();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
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
                        let data = model::encode_png(&definitions).map_err(|e| e.to_string())?;
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
                            read_file(w.get_path().as_str())?
                        } else {
                            model::decode_text(w.get_text().as_str()).map_err(|e| e.to_string())?
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
fn read_file(path: &str) -> Result<Vec<Definition>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((hydrus_downloader_exchange::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.starts_with(b"\x89PNG") {
        model::decode_png(&bytes).map_err(|e| e.to_string())
    } else {
        model::decode_text(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
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
