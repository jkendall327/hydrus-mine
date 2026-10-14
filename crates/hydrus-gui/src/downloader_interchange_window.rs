//! Shared downloader exchange window: bounded files, clipboard text, and
//! concrete review before its owner stages or atomically saves the package.
use crate::DownloaderExchangeWindow;
use hydrus_gui_model::downloader_interchange::{self as model, Definition, Draft};
use hydrus_gui_model::downloader_package as package;
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
            drops: false,
            warning: None,
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
            drops: false,
            warning: None,
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
            drops: false,
            warning: None,
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
    /// The reference's drop target and image paste: files dropped on the
    /// window, and a bitmap on the clipboard, load as PNG or JSON packages.
    drops: bool,
    /// The reference's warning about a package's objects this list does not
    /// take (from text or PNG bytes), shown above the review.
    warning: Option<Warning>,
    /// Owner-specific actions (the package window's domain prompt).
    actions: Option<ExtraActions>,
}
/// A package's warning from its text or PNG bytes.
type Warning = fn(&[u8]) -> Option<String>;
/// See an action before the shared window does: `None` when handled, else
/// the action the shared window then does (the same one, or another).
type ExtraActions = Rc<dyn Fn(&DownloaderExchangeWindow, &str) -> Result<Option<String>, String>>;

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
            drops: false,
            warning: Some(processing::warning),
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
            drops: false,
            warning: None,
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
            drops: false,
            warning: None,
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
            drops: false,
            warning: None,
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
    let w =
        crate::app_title::new::<crate::DownloaderExchangeWindow>().map_err(|e| e.to_string())?;
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
    if !importing && !definitions.is_empty() {
        w.set_text(
            (codec.encode_text)(definitions)
                .map_err(|e| e.to_string())?
                .into(),
        );
    }
    let codec = Rc::new(codec);
    let review = Rc::new(review);
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
        let codec = codec.clone();
        let review = review.clone();
        let pending = pending.clone();
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
                let action = match &codec.actions {
                    Some(actions) => match actions(&w, action.as_str())? {
                        Some(action) => action,
                        None => return Ok(()),
                    },
                    None => action.to_string(),
                };
                match action.as_str() {
                    "cancel" => close(),
                    "back" => {
                        pending.borrow_mut().take();
                        w.set_ready(false);
                        w.set_review("".into());
                    }
                    "paste" if importing && codec.drops => {
                        // (the reference's IngestClipboard: a bitmap is a hydrus PNG)
                        if let Some(png) = crate::exchange_bitmap::clipboard_png() {
                            load_sources(
                                &w,
                                &codec,
                                review.as_ref().as_ref(),
                                &pending,
                                vec![Source::Png(
                                    png?,
                                    "Problem pasting image! Full error written to log; ".into(),
                                )],
                            )
                            .map_err(|e| pasted_error(&e))?;
                        } else {
                            w.set_text(crate::from_clipboard()?.into());
                        }
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
                        let mut sources = Vec::new();
                        for path in paths {
                            let bytes = read_bytes(&path)?;
                            sources.push(if png {
                                Source::Png(bytes, String::new())
                            } else {
                                Source::Text(bytes)
                            });
                        }
                        load_sources(&w, &codec, review.as_ref().as_ref(), &pending, sources)?;
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
                        let bytes = if action == "open" {
                            read_bytes(Path::new(w.get_path().as_str()))?
                        } else {
                            w.get_text().as_bytes().to_vec()
                        };
                        let warnings: Vec<String> = codec
                            .warning
                            .and_then(|warn| warn(&bytes))
                            .into_iter()
                            .collect();
                        // (nothing taken: the reference's warning is all it says)
                        let definitions = if action == "open" {
                            read_file(w.get_path().as_str(), &codec)
                        } else {
                            (codec.decode_text)(w.get_text().as_str()).map_err(|e| e.to_string())
                        }
                        .map_err(|e| warnings.first().cloned().unwrap_or(e))?;
                        let description =
                            review_of(review.as_ref().as_ref())?.0(definitions.clone())?;
                        w.set_review(warned(&warnings, description).into());
                        *pending.borrow_mut() = Some(definitions);
                        w.set_ready(true);
                    }
                    "accept" => {
                        let definitions = pending
                            .borrow()
                            .clone()
                            .ok_or_else(|| "Review a package before importing.".to_owned())?;
                        review_of(review.as_ref().as_ref())?.1(definitions)?;
                        close();
                    }
                    _ => (),
                }
                Ok(())
            })();
            w.set_error(result.err().unwrap_or_default().into());
        }
    });
    if importing && codec.drops {
        // (files dropped on the window load as the reference's import loads
        // its paths)
        crate::drops::on_files_dropped(w.window(), {
            let weak = w.as_weak();
            let codec = codec.clone();
            let review = review.clone();
            let pending = pending.clone();
            let active = active.clone();
            move |paths| {
                let Some(w) = weak.upgrade() else {
                    return;
                };
                if !active.get() || w.get_png_child() || !w.get_overwrite_question().is_empty() {
                    return;
                }
                let result = sources_of_paths(&paths)
                    .and_then(|sources| {
                        load_sources(&w, &codec, review.as_ref().as_ref(), &pending, sources)
                    })
                    .map_err(|e| pasted_error(&e));
                w.set_error(result.err().unwrap_or_default().into());
            }
        });
    }
    w.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    w.show().map_err(|e| e.to_string())?;
    *slots.0.borrow_mut() = Some(w.clone_strong());
    Ok(w)
}
/// Something an import loads packages from.
enum Source {
    /// A hydrus PNG's bytes, and what the reference says first of one that
    /// fails to load (nothing, for a picked file).
    Png(Vec<u8>, String),
    /// A JSON file's (or the clipboard's) text.
    Text(Vec<u8>),
}

/// The reference's `IngestPaths`: a PNG is read as one, anything else as JSON.
fn sources_of_paths(paths: &[String]) -> Result<Vec<Source>, String> {
    paths
        .iter()
        .map(|path| {
            let bytes = read_bytes(Path::new(path))?;
            Ok(if bytes.starts_with(b"\x89PNG") {
                Source::Png(
                    bytes,
                    format!("Problem loading PNG at path \"{path}\"! Full error written to log; "),
                )
            } else {
                Source::Text(bytes)
            })
        })
        .collect()
}

/// How the package import tells of a failed paste or drop.
fn pasted_error(error: &str) -> String {
    format!("Sorry, there was a problem importing those paths! Error was: {error}")
}

/// Read each source into one package for the review, stopping at the first
/// that will not load (the review is changed only when all do).
fn load_sources<T: Clone>(
    w: &DownloaderExchangeWindow,
    codec: &Codec<T>,
    review: Option<&(Preview<T>, Apply<T>)>,
    pending: &RefCell<Option<Vec<T>>>,
    sources: Vec<Source>,
) -> Result<(), String> {
    let mut definitions = Vec::new();
    let mut warnings = Vec::new();
    let mut total_bytes = 0usize;
    for source in sources {
        let (png, bytes, said) = match source {
            Source::Png(bytes, said) => (true, bytes, said),
            Source::Text(bytes) => (false, bytes, String::new()),
        };
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > hydrus_downloader_exchange::MAX_BYTES {
            return Err(hydrus_downloader_exchange::Error::Limit.to_string());
        }
        let warning = codec.warning.and_then(|warn| warn(&bytes));
        let mut incoming = if png {
            (codec.decode_png)(&bytes).map_err(|e| e.to_string())
        } else {
            (codec.decode_text)(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        }
        .map_err(|e| {
            warning.clone().unwrap_or_else(|| {
                format!(
                    "{said}{}",
                    crate::exchange_bitmap::not_a_hydrus_png(&e)
                        .filter(|_| png && !said.is_empty())
                        .unwrap_or(e)
                )
            })
        })?;
        warnings.extend(warning);
        definitions.append(&mut incoming);
        if definitions.len() > hydrus_downloader_exchange::MAX_OBJECTS {
            return Err("Subscription package exceeds the object limit.".into());
        }
    }
    let description = review_of(review)?.0(definitions.clone())?;
    w.set_review(warned(&warnings, description).into());
    *pending.borrow_mut() = Some(definitions);
    w.set_ready(true);
    Ok(())
}

/// A review, after the reference's warnings about what is left out.
fn warned(warnings: &[String], description: String) -> String {
    if warnings.is_empty() {
        description
    } else {
        format!("{}\n\n{description}", warnings.join("\n\n"))
    }
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
/// A chooser open over the package window.
enum Chooser {
    /// An export "add" button's.
    Export(package::Add, Vec<package::Choice>),
    /// The import's "select objects to add".
    Import(package::Import, Vec<package::Choice>),
}

/// A question or notice open over the package window.
enum Asking {
    /// "export to png"'s summary: yes saves the PNG.
    ExportPng,
    /// The import's domain metadata, one notice at a time, then its final
    /// question.
    Details {
        details: std::collections::VecDeque<String>,
        question: String,
        import: package::Import,
        chosen: Vec<String>,
    },
    /// The import's final question: yes imports.
    Confirm {
        import: package::Import,
        chosen: Vec<String>,
    },
    /// A notice; the window closes after it if the import is done.
    Notice { done: bool },
}

#[derive(Default)]
struct PackageState {
    list: RefCell<package::ExportList>,
    rows: RefCell<hydrus_gui_model::list_selection::ListSelection<usize>>,
    chooser: RefCell<Option<Chooser>>,
    asking: RefCell<Option<Asking>>,
    /// The package last reviewed, for the import.
    reviewed: RefCell<Option<Vec<Definition>>>,
}

fn show_chooser(window: &DownloaderExchangeWindow, title: &str, choices: &[package::Choice]) {
    window.set_chooser_title(title.into());
    window.set_chooser_choices(slint::ModelRc::new(slint::VecModel::from(
        choices
            .iter()
            .map(|c| crate::PackageChoice {
                label: c.label.as_str().into(),
                included: c.checked,
            })
            .collect::<Vec<_>>(),
    )));
}

fn ask(window: &DownloaderExchangeWindow, text: &str, asks: bool) {
    window.set_question(text.into());
    window.set_question_asks(asks);
}

/// The export list's rows and the package text it makes.
fn show_export(window: &DownloaderExchangeWindow, state: &PackageState, notice: &str) {
    let list = state.list.borrow();
    let rows = state.rows.borrow();
    window.set_export_rows(slint::ModelRc::new(slint::VecModel::from(
        list.items()
            .iter()
            .enumerate()
            .map(|(row, definition)| crate::TableRow {
                cells: slint::ModelRc::new(slint::VecModel::from(vec![
                    slint::SharedString::from(package::row_name(definition)),
                    package::type_name(definition).into(),
                ])),
                selected: rows.is_selected(row),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_review(notice.into());
    if list.items().is_empty() {
        window.set_text("".into());
    } else {
        match model::encode_text(list.items()) {
            Ok(text) => window.set_text(text.into()),
            Err(error) => window.set_error(error.to_string().into()),
        }
    }
}

/// The downloader sharing window: the reference's export panel (its list,
/// filled by its add buttons, asked about before it becomes a PNG) or its
/// import (the package reviewed, then, on import, the reference's chooser,
/// domain metadata notices and final question).
pub fn package(
    store: &Arc<Store>,
    slots: &Slots,
    importing: bool,
) -> Result<DownloaderExchangeWindow, String> {
    if let Some(window) = slots.0.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let draft = Draft::load(store).map_err(|e| e.to_string())?;
    let state = Rc::new(PackageState::default());
    let preview: Preview = Rc::new({
        let draft = draft.clone();
        let state = state.clone();
        move |definitions| {
            let mut next = draft.clone();
            let text = next.import(definitions.clone()).map(|r| {
                r.text().replace(
                    "Changes are saved only when you apply the owning editor.",
                    "Click import to save this package.",
                )
            })?;
            *state.reviewed.borrow_mut() = Some(definitions);
            Ok(text)
        }
    });
    // (the import goes through the questions below instead)
    let applied: Apply = Rc::new(|_| Ok(()));
    let actions: ExtraActions = Rc::new({
        let state = state.clone();
        let store = store.clone();
        move |window: &DownloaderExchangeWindow, action: &str| {
            let add = match action {
                "add-gug" => Some(package::Add::Gug),
                "add-url-class" => Some(package::Add::UrlClass),
                "add-parser" => Some(package::Add::Parser),
                "add-login-script" => Some(package::Add::LoginScript),
                _ => None,
            };
            if let Some(add) = add {
                let choices = state.list.borrow().choices(&draft, add);
                show_chooser(window, add.title(), &choices);
                *state.chooser.borrow_mut() = Some(Chooser::Export(add, choices));
                return Ok(None);
            }
            match action {
                "chooser-cancel" => {
                    state.chooser.borrow_mut().take();
                    window.set_chooser_title("".into());
                }
                "chooser-ok" => {
                    let chooser = state.chooser.borrow_mut().take();
                    window.set_chooser_title("".into());
                    match chooser {
                        Some(Chooser::Export(add, choices)) => {
                            let chosen: Vec<Definition> = choices
                                .into_iter()
                                .filter(|c| c.checked)
                                .map(|c| c.definition)
                                .collect();
                            state.list.borrow_mut().add(&draft, add, &chosen);
                            show_export(window, &state, "");
                        }
                        Some(Chooser::Import(import, choices)) => {
                            let chosen: Vec<String> = choices
                                .into_iter()
                                .filter(|c| c.checked)
                                .map(|c| c.label)
                                .collect();
                            let step = import.confirm(&draft, &chosen)?;
                            confirm(window, &state, import, chosen, step);
                        }
                        None => (),
                    }
                }
                "add-domain" => {
                    window.set_domain_text("".into());
                    window.set_domain_prompt(true);
                }
                "domain-cancel" => window.set_domain_prompt(false),
                "domain-ok" => {
                    window.set_domain_prompt(false);
                    let domain = window.get_domain_text().to_string();
                    let details = state.list.borrow_mut().add_domain(&draft, &domain)?;
                    show_export(window, &state, &details.join("\n\n"));
                }
                "delete" if !importing => {
                    let rows: BTreeSet<usize> = state
                        .rows
                        .borrow()
                        .selected_order()
                        .iter()
                        .copied()
                        .collect();
                    state.list.borrow_mut().remove(&rows);
                    *state.rows.borrow_mut() =
                        hydrus_gui_model::list_selection::ListSelection::default();
                    show_export(window, &state, "");
                }
                "save" if !importing => {
                    if window.get_path().is_empty() {
                        return Ok(Some(action.to_owned()));
                    }
                    ask(window, &state.list.borrow().question(), true);
                    *state.asking.borrow_mut() = Some(Asking::ExportPng);
                }
                "accept" if importing => {
                    let definitions = state
                        .reviewed
                        .borrow()
                        .clone()
                        .ok_or_else(|| "Review a package before importing.".to_owned())?;
                    let import = package::Import::new(&draft, definitions)?;
                    match import.start(&draft, window.get_select_from_list())? {
                        package::Step::NothingNew(notice) => {
                            ask(window, &notice, false);
                            *state.asking.borrow_mut() = Some(Asking::Notice { done: false });
                        }
                        package::Step::Choose(choices) => {
                            show_chooser(window, "select objects to add", &choices);
                            *state.chooser.borrow_mut() = Some(Chooser::Import(import, choices));
                        }
                        step @ package::Step::Confirm { .. } => {
                            let chosen = import.all();
                            confirm(window, &state, import, chosen, step);
                        }
                    }
                }
                "yes" | "no" | "ok" => {
                    let asking = state.asking.borrow_mut().take();
                    ask(window, "", false);
                    match (asking, action) {
                        (Some(Asking::ExportPng), "yes") => {
                            // (asked and answered: the PNG is saved)
                            return Ok(Some("save".into()));
                        }
                        (
                            Some(Asking::Details {
                                mut details,
                                question,
                                import,
                                chosen,
                            }),
                            "ok",
                        ) => {
                            if let Some(detail) = details.pop_front() {
                                ask(window, &detail, false);
                                *state.asking.borrow_mut() = Some(Asking::Details {
                                    details,
                                    question,
                                    import,
                                    chosen,
                                });
                            } else {
                                ask(window, &question, true);
                                *state.asking.borrow_mut() =
                                    Some(Asking::Confirm { import, chosen });
                            }
                        }
                        (Some(Asking::Confirm { import, chosen }), "yes") => {
                            let mut next = draft.clone();
                            let notice = import.apply(&mut next, &chosen)?;
                            next.save(&store).map_err(|e| e.to_string())?;
                            default_downloader(&store);
                            ask(window, &notice, false);
                            *state.asking.borrow_mut() = Some(Asking::Notice { done: true });
                        }
                        (Some(Asking::Notice { done: true }), "ok") => {
                            return Ok(Some("cancel".into()));
                        }
                        _ => (),
                    }
                }
                _ => return Ok(Some(action.to_owned())),
            }
            Ok(None)
        }
    });
    // (a package's other objects are skipped, as the reference's import
    // skips them, and one with nothing else is warned about)
    let window = open_objects(
        slots,
        importing,
        &[],
        Some((preview, applied)),
        Codec {
            encode_text: model::encode_text,
            decode_text: hydrus_downloader_exchange::decode_package_definitions,
            encode_png: model::encode_png,
            decode_png: hydrus_downloader_exchange::decode_package_png,
            processing: false,
            drops: true,
            warning: Some(hydrus_downloader_exchange::package_warning),
            actions: Some(actions),
        },
    )?;
    window.set_json_enabled(true);
    if importing {
        window.set_select_from_list_shown(true);
        window.set_select_from_list(
            store
                .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                .is_ok_and(|advanced| advanced.0),
        );
    } else {
        window.set_export_list(true);
        window.set_export_columns(slint::ModelRc::new(slint::VecModel::from(vec![
            crate::TableColumn {
                title: "name".into(),
                width: 380.0,
                stretch: true,
            },
            crate::TableColumn {
                title: "type".into(),
                width: 220.0,
                stretch: false,
            },
        ])));
        show_export(&window, &state, "");
        window.set_instructions("Add the downloaders, URL classes, parsers, login scripts and headers/bandwidth rules to share; a downloader brings its URL classes, parsers and domains' rules, and a URL class its API links and parser. Login scripts include their rules, but not saved domain credentials, sessions or activation.".into());
        window.on_export_row_clicked({
            let weak = window.as_weak();
            let state = state.clone();
            move |row, ctrl, shift| {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let Ok(row) = usize::try_from(row) else {
                    return;
                };
                let order: Vec<usize> = (0..state.list.borrow().items().len()).collect();
                state.rows.borrow_mut().click(&order, row, ctrl, shift);
                show_export(&window, &state, "");
            }
        });
    }
    window.on_chooser_toggled({
        let weak = window.as_weak();
        let state = state.clone();
        move |row, checked| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut chooser = state.chooser.borrow_mut();
            let (title, choices) = match chooser.as_mut() {
                Some(Chooser::Export(add, choices)) => (add.title(), choices),
                Some(Chooser::Import(_, choices)) => ("select objects to add", choices),
                None => return,
            };
            if let Some(choice) = usize::try_from(row).ok().and_then(|r| choices.get_mut(r)) {
                choice.checked = checked;
            }
            show_chooser(&window, title, choices);
        }
    });
    Ok(window)
}

/// With no default downloader, the first by name becomes it, as the
/// reference's import leaves it.
fn default_downloader(store: &Store) {
    let result = store.write(|ctx| {
        let conn = ctx.conn();
        let mut defaults: hydrus_core::subscriptions::GalleryDefaults =
            hydrus_store::settings::get(conn)?;
        if defaults.gug.is_some() {
            return Ok(());
        }
        let downloaders: hydrus_parse::Downloaders = hydrus_store::settings::get(conn)?;
        let first = downloaders
            .gugs
            .gugs
            .iter()
            .min_by(|a, b| a.name().cmp(b.name()));
        if let Some(first) = first {
            defaults.gug = Some((first.key().to_owned(), first.name().to_owned()));
            hydrus_store::settings::set(conn, &defaults)?;
        }
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("could not set the default downloader: {e}");
    }
}

/// The import's domain metadata notices, then its final question.
fn confirm(
    window: &DownloaderExchangeWindow,
    state: &PackageState,
    import: package::Import,
    chosen: Vec<String>,
    step: package::Step,
) {
    let package::Step::Confirm { details, question } = step else {
        return;
    };
    let mut details: std::collections::VecDeque<String> = details.into();
    if let Some(detail) = details.pop_front() {
        ask(window, &detail, false);
        *state.asking.borrow_mut() = Some(Asking::Details {
            details,
            question,
            import,
            chosen,
        });
    } else {
        ask(window, &question, true);
        *state.asking.borrow_mut() = Some(Asking::Confirm { import, chosen });
    }
}
