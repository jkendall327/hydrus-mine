//! Shared downloader exchange window: bounded files, clipboard text, and
//! concrete review before its owner stages or atomically saves the package.
use crate::DownloaderExchangeWindow;
use hydrus_gui_model::downloader_interchange::{self as model, Definition, Draft};
use hydrus_store::Store;
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    io::Read,
    path::Path,
    rc::Rc,
    sync::Arc,
};

#[path = "router_import_window.rs"]
mod router_import;
pub use router_import::open as open_router_import;

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
    definitions: &[Definition],
    preview: Preview,
    applied: Apply,
) -> Result<DownloaderExchangeWindow, String> {
    open_with_actions(slots, importing, definitions, preview, applied, None)
}

fn open_with_actions(
    slots: &Slots,
    importing: bool,
    definitions: &[Definition],
    preview: Preview,
    applied: Apply,
    actions: Option<ExtraActions>,
) -> Result<DownloaderExchangeWindow, String> {
    open_objects(
        slots,
        importing,
        definitions,
        Some((preview, applied)),
        Codec {
            encode_text: model::encode_text,
            decode_text: model::decode_text,
            encode_png: model::encode_png,
            decode_png: model::decode_png,
            processing: false,
            actions,
        },
    )
}

/// Export complete subscriptions from their owning list draft. (Imports add
/// to the list directly, as the reference's list control does.)
pub fn open_subscriptions(
    store: &Arc<Store>,
    slots: &Slots,
    subscriptions: &[hydrus_downloader_exchange::subscriptions::Subscription],
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::subscriptions;
    let payload = (
        subscriptions::encode_text(subscriptions).map_err(|e| e.to_string())?,
        subscriptions.len(),
    );
    let window = open_objects(
        slots,
        false,
        subscriptions,
        None,
        Codec {
            encode_text: subscriptions::encode_text,
            decode_text: subscriptions::decode_text,
            encode_png: subscriptions::encode_png,
            decode_png: subscriptions::decode_png,
            processing: false,
            actions: None,
        },
    )?;
    window.set_json_enabled(true);
    window.set_window_title("export subscriptions".into());
    window.set_instructions(
        "Complete subscriptions include query settings and file/gallery histories.".into(),
    );
    {
        let (payload, count) = payload;
        let summary = hydrus_gui_model::png_export::object_payload_description_with_format(
            &payload,
            "Subscription Container",
            count,
            &hydrus_gui_model::gui_format::preferences(store),
        );
        attach_png(store, slots, &window, payload, summary);
    }
    Ok(window)
}

/// Exchange registered external calls from their detached Options list.
pub fn open_external_calls(
    store: &Arc<Store>,
    slots: &Slots,
    importing: bool,
    calls: &[hydrus_core::external_calls::Callable],
    preview: Preview<hydrus_core::external_calls::Callable>,
    applied: Apply<hydrus_core::external_calls::Callable>,
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::external_calls as codec;
    let export = if importing {
        None
    } else {
        Some(codec::encode_text(calls).map_err(|e| e.to_string())?)
    };
    let count = calls.len();
    let w = open_objects(
        slots,
        importing,
        calls,
        Some((preview, applied)),
        Codec {
            encode_text: codec::encode_text,
            decode_text: codec::decode_text,
            encode_png: codec::encode_png,
            decode_png: codec::decode_png,
            processing: false,
            actions: None,
        },
    )?;
    w.set_json_enabled(true);
    w.set_window_title(
        if importing {
            "import external calls"
        } else {
            "export external calls"
        }
        .into(),
    );
    w.set_instructions("Registered external calls stay staged until Options is applied. Inspect imported commands and parameters before running them.".into());
    if let Some(payload) = export {
        let summary = hydrus_gui_model::png_export::object_payload_description(
            &payload,
            "Executable Manager Callable",
            count,
        );
        attach_png(store, slots, &w, payload, summary);
    }
    Ok(w)
}

struct Codec<T> {
    encode_text: fn(&[T]) -> hydrus_downloader_exchange::Result<String>,
    decode_text: fn(&str) -> hydrus_downloader_exchange::Result<Vec<T>>,
    encode_png: fn(&[T]) -> hydrus_downloader_exchange::Result<Vec<u8>>,
    decode_png: fn(&[u8]) -> hydrus_downloader_exchange::Result<Vec<T>>,
    processing: bool,
    /// Owner-specific actions (the package window's domain prompt).
    actions: Option<ExtraActions>,
}
/// Handle an action the shared window does not know.
/// Redraw the package choices and text after a selection or addition.
type Refresh = Rc<dyn Fn(&DownloaderExchangeWindow, &str)>;
type ExtraActions = Rc<dyn Fn(&DownloaderExchangeWindow, &str) -> Result<(), String>>;

/// Import/export the shared processor editor's selected steps using reference
/// JSON, clipboard text or PNG. Applying appends only to the owner's draft.
pub fn open_steps(
    slots: &Slots,
    importing: bool,
    steps: &[hydrus_core::url::strings::ProcessingStep],
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
        Some((preview, applied)),
        Codec {
            encode_text: processing::encode_text,
            decode_text: processing::decode_text,
            encode_png: processing::encode_png,
            decode_png: processing::decode_png,
            processing: true,
            actions: None,
        },
    )
}

/// Share a bounded reference login script package from its owning list draft.
pub fn open_login_scripts(
    slots: &Slots,
    importing: bool,
    scripts: &[hydrus_parse::login::LoginScript],
    preview: Preview<hydrus_parse::login::LoginScript>,
    applied: Apply<hydrus_parse::login::LoginScript>,
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::logins;
    let window = open_objects(
        slots,
        importing,
        scripts,
        Some((preview, applied)),
        Codec {
            encode_text: logins::encode_text,
            decode_text: logins::decode_text,
            encode_png: logins::encode_png,
            decode_png: logins::decode_png,
            processing: false,
            actions: None,
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
    parsers: &[hydrus_parse::content::SubsidiaryPageParser],
    preview: Preview<hydrus_parse::content::SubsidiaryPageParser>,
    applied: Apply<hydrus_parse::content::SubsidiaryPageParser>,
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::subsidiaries;
    let window = open_objects(
        slots,
        importing,
        parsers,
        Some((preview, applied)),
        Codec {
            encode_text: subsidiaries::encode_text,
            decode_text: subsidiaries::decode_text,
            encode_png: subsidiaries::encode_png,
            decode_png: subsidiaries::decode_png,
            processing: false,
            actions: None,
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
    routers: &[hydrus_parse::sidecar::Router],
    preview: Preview<hydrus_parse::sidecar::Router>,
    applied: Apply<hydrus_parse::sidecar::Router>,
) -> Result<DownloaderExchangeWindow, String> {
    use hydrus_downloader_exchange::routers;
    let window = open_objects(
        slots,
        importing,
        routers,
        Some((preview, applied)),
        Codec {
            encode_text: routers::encode_text,
            decode_text: routers::decode_text,
            encode_png: routers::encode_png,
            decode_png: routers::decode_png,
            processing: false,
            actions: None,
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
    routers: &[hydrus_parse::sidecar::Router],
    preview: Preview<hydrus_parse::sidecar::Router>,
    applied: Apply<hydrus_parse::sidecar::Router>,
) -> Result<DownloaderExchangeWindow, String> {
    let payload = if importing {
        None
    } else {
        Some((
            hydrus_downloader_exchange::routers::encode_text(routers).map_err(|e| e.to_string())?,
            routers.len(),
        ))
    };
    let window = open_routers(slots, importing, routers, preview, applied)?;
    if let Some((payload, count)) = payload {
        let summary = hydrus_gui_model::png_export::object_payload_description_with_format(
            &payload,
            "Metadata Single File Router",
            count,
            &hydrus_gui_model::gui_format::preferences(store),
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
    parsers: &[hydrus_parse::content::SubsidiaryPageParser],
    preview: Preview<hydrus_parse::content::SubsidiaryPageParser>,
    applied: Apply<hydrus_parse::content::SubsidiaryPageParser>,
) -> Result<DownloaderExchangeWindow, String> {
    let payload = if importing {
        None
    } else {
        Some((
            hydrus_downloader_exchange::subsidiaries::encode_text(parsers)
                .map_err(|e| e.to_string())?,
            parsers.len(),
        ))
    };
    let window = open_subsidiaries(slots, importing, parsers, preview, applied)?;
    if let Some((payload, count)) = payload {
        let summary = hydrus_gui_model::png_export::object_payload_description_with_format(
            &payload,
            "Subsidiary Page Parser",
            count,
            &hydrus_gui_model::gui_format::preferences(store),
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
            if !window.get_active() || png.has_open() || !window.get_overwrite_question().is_empty()
            {
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
    definitions: &[T],
    // `None` for a window that only exports
    review: Option<(Preview<T>, Apply<T>)>,
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
            (codec.encode_text)(definitions)
                .map_err(|e| e.to_string())?
                .into(),
        );
    }
    let active = Rc::new(Cell::new(true));
    let pending = Rc::new(RefCell::new(None::<Vec<T>>));
    let overwrite = Rc::new(RefCell::new(None::<std::path::PathBuf>));
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
            if overwrite.borrow().is_some()
                && !matches!(action.as_str(), "cancel" | "yes-json" | "no-json")
            {
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
                    "copy" if !w.get_text().is_empty() => {
                        crate::copy_to_clipboard(w.get_text().as_str());
                    }
                    "import-jsons" | "import-pngs" if importing && w.get_json_enabled() => {
                        let png = action == "import-pngs";
                        let title = if png {
                            "select the png or pngs with the encoded data"
                        } else {
                            "select the json or jsons with the serialised data"
                        };
                        let paths =
                            crate::pick_exchange_files(title, if png { "png" } else { "json" });
                        if paths.is_empty() {
                            return Ok(());
                        }
                        let mut definitions = Vec::new();
                        let mut total_bytes = 0usize;
                        for path in paths {
                            let bytes = read_bytes(&path)?;
                            total_bytes = total_bytes.saturating_add(bytes.len());
                            if total_bytes > hydrus_downloader_exchange::MAX_BYTES {
                                return Err(hydrus_downloader_exchange::Error::Limit.to_string());
                            }
                            let mut incoming = if png {
                                (codec.decode_png)(&bytes).map_err(|e| e.to_string())?
                            } else {
                                (codec.decode_text)(
                                    std::str::from_utf8(&bytes).map_err(|e| e.to_string())?,
                                )
                                .map_err(|e| e.to_string())?
                            };
                            definitions.append(&mut incoming);
                            if definitions.len() > hydrus_downloader_exchange::MAX_OBJECTS {
                                return Err("Subscription package exceeds the object limit.".into());
                            }
                        }
                        w.set_review(review_of(review.as_ref())?.0(definitions.clone())?.into());
                        *pending.borrow_mut() = Some(definitions);
                        w.set_ready(true);
                    }
                    "browse-json" if !importing && w.get_json_enabled() => {
                        if let Some(path) = crate::pick_exchange_export() {
                            w.set_path(path.to_string_lossy().as_ref().into());
                        }
                    }
                    "save-json" if !importing && w.get_json_enabled() => {
                        let path = std::path::PathBuf::from(w.get_path().as_str());
                        if path.as_os_str().is_empty() {
                            return Err("Choose an export path first.".into());
                        }
                        if path.exists() {
                            w.set_overwrite_question(
                                format!(
                                    "The path \"{}\" already exists! Ok to overwrite?",
                                    path.display()
                                )
                                .into(),
                            );
                            *overwrite.borrow_mut() = Some(path);
                        } else {
                            save_json(&path, &export_text(&w, &codec)?)?;
                            w.set_review("JSON saved.".into());
                        }
                    }
                    "yes-json" => {
                        if let Some(path) = overwrite.borrow_mut().take() {
                            save_json(&path, &export_text(&w, &codec)?)?;
                            w.set_review("JSON saved.".into());
                        }
                        w.set_overwrite_question("".into());
                    }
                    "no-json" => {
                        overwrite.borrow_mut().take();
                        w.set_overwrite_question("".into());
                    }
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
                        let selected = (codec.decode_text)(w.get_text().as_str())
                            .map_err(|e| e.to_string())?;
                        let data = (codec.encode_png)(&selected).map_err(|e| e.to_string())?;
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
                        let description = review_of(review.as_ref())?.0(definitions.clone())?;
                        w.set_review(description.into());
                        *pending.borrow_mut() = Some(definitions);
                        w.set_ready(true);
                    }
                    "accept" => {
                        let definitions = pending
                            .borrow()
                            .clone()
                            .ok_or_else(|| "Review a package before importing.".to_owned())?;
                        review_of(review.as_ref())?.1(definitions)?;
                        close();
                    }
                    other => {
                        if let Some(actions) = &codec.actions {
                            actions(&w, other)?;
                        }
                    }
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
fn review_of<T>(
    review: Option<&(Preview<T>, Apply<T>)>,
) -> Result<&(Preview<T>, Apply<T>), String> {
    review.ok_or_else(|| "This window only exports.".to_owned())
}
fn export_text<T>(window: &DownloaderExchangeWindow, codec: &Codec<T>) -> Result<String, String> {
    let selected =
        (codec.decode_text)(window.get_text().as_str()).map_err(|error| error.to_string())?;
    (codec.encode_text)(&selected).map_err(|error| error.to_string())
}
fn save_json(path: &Path, text: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, text.as_bytes()).map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((hydrus_downloader_exchange::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > hydrus_downloader_exchange::MAX_BYTES {
        return Err(hydrus_downloader_exchange::Error::Limit.to_string());
    }
    Ok(bytes)
}
fn read_file<T>(path: &str, codec: &Codec<T>) -> Result<Vec<T>, String> {
    let bytes = read_bytes(Path::new(path))?;
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
    if let Some(window) = slots.0.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
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
    let export_draft = Rc::new(RefCell::new(draft.clone()));
    let applied: Apply = Rc::new({
        let store = store.clone();
        move |definitions| {
            let mut next = draft.clone();
            next.import(definitions)?;
            next.save(&store).map_err(|e| e.to_string())
        }
    });
    let selected = Rc::new(RefCell::new(
        (0..definitions.len()).collect::<BTreeSet<_>>(),
    ));
    let refresh: Refresh = Rc::new({
        let export_draft = export_draft.clone();
        let selected = selected.clone();
        move |window: &DownloaderExchangeWindow, notice: &str| {
            let draft = export_draft.borrow();
            let definitions = draft.definitions();
            let selected = selected.borrow();
            window.set_package_choices(slint::ModelRc::new(slint::VecModel::from(
                definitions
                    .iter()
                    .enumerate()
                    .map(|(index, definition)| crate::PackageChoice {
                        label: format!("{}: {}", model::category(definition), definition.name())
                            .into(),
                        included: selected.contains(&index),
                    })
                    .collect::<Vec<_>>(),
            )));
            let payload = draft.export(&selected);
            let mut review = format!("{} component(s) included with dependencies.", payload.len());
            if !notice.is_empty() {
                review = format!("{notice}\n\n{review}");
            }
            window.set_review(review.into());
            if payload.is_empty() {
                window.set_text("".into());
            } else {
                match model::encode_text(&payload) {
                    Ok(text) => window.set_text(text.into()),
                    Err(error) => window.set_error(error.to_string().into()),
                }
            }
        }
    });
    let window = open_with_actions(
        slots,
        importing,
        &definitions,
        preview,
        applied,
        (!importing).then(|| -> ExtraActions {
            let export_draft = export_draft.clone();
            let selected = selected.clone();
            let refresh = refresh.clone();
            Rc::new(
                move |window: &DownloaderExchangeWindow, action: &str| match action {
                    "add-domain" => {
                        window.set_domain_text("".into());
                        window.set_domain_prompt(true);
                        Ok(())
                    }
                    "domain-cancel" => {
                        window.set_domain_prompt(false);
                        Ok(())
                    }
                    "domain-ok" => {
                        window.set_domain_prompt(false);
                        let domain = window.get_domain_text().to_string();
                        let details = export_draft.borrow_mut().add_domain_exports(&domain)?;
                        let count = export_draft.borrow().definitions().len();
                        // (the new entries are the last ones)
                        for index in count - details.len()..count {
                            selected.borrow_mut().insert(index);
                        }
                        refresh(window, &details.join("\n\n"));
                        Ok(())
                    }
                    _ => Ok(()),
                },
            )
        }),
    )?;
    window.set_json_enabled(true);
    if !importing {
        window.set_domain_enabled(true);
        refresh(&window, "");
        window.set_instructions("Choose the registered components to share. Linked generators, URL classes and parsers are included automatically. Login scripts include their rules, but not saved domain credentials, sessions or activation. Headers and bandwidth rules can be added by domain.".into());
        window.on_package_chosen({
            let weak = window.as_weak();
            let refresh = refresh.clone();
            let export_draft = export_draft.clone();
            move |index, included| {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                if !window.get_active()
                    || window.get_png_child()
                    || !window.get_overwrite_question().is_empty()
                    || window.get_domain_prompt()
                {
                    return;
                }
                let choice_count = export_draft.borrow().definitions().len();
                {
                    let mut selected = selected.borrow_mut();
                    if index < 0 {
                        if included {
                            *selected = (0..choice_count).collect();
                        } else {
                            selected.clear();
                        }
                    } else if let Ok(index) = usize::try_from(index)
                        && index < choice_count
                    {
                        if included {
                            selected.insert(index);
                        } else {
                            selected.remove(&index);
                        }
                    }
                }
                refresh(&window, "");
            }
        });
    }
    Ok(window)
}
