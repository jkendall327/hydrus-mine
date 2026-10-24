//! Owned, reusable string PNG export panel with the reference title/width fields.
use crate::PngExportWindow;
use hydrus_gui_model::png_export as model;
use hydrus_store::{Store, settings};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    io::Read,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

/// An export child belongs to its log/editor and closes with that owner.
#[derive(Clone, Default)]
pub struct Slots(pub Rc<RefCell<Option<PngExportWindow>>>);
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PngSlots").field(&self.has_open()).finish()
    }
}
impl Drop for Slots {
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) == 1 {
            self.cancel();
        }
    }
}
impl Slots {
    pub fn has_open(&self) -> bool {
        self.0.borrow().is_some()
    }
    pub fn window(&self) -> Option<PngExportWindow> {
        self.0
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    }
    pub fn cancel(&self) {
        if let Some(w) = self.window() {
            w.invoke_action("close".into());
        }
    }
}
thread_local! { static LAST: RefCell<slint::Weak<PngExportWindow>> = RefCell::new(slint::Weak::default()); }
/// The most recently opened export panel, for native owner tests.
pub fn last() -> Option<PngExportWindow> {
    LAST.with(|s| s.borrow().upgrade())
}

/// Read only the bounded carrier file; cancelling the native picker is a no-op.
pub(crate) fn import_text() -> Result<Option<String>, String> {
    import_text_with_title("select the png with the sources")
}

pub(crate) fn import_text_with_title(title: &str) -> Result<Option<String>, String> {
    let Some(path) = crate::pick(crate::Pick::Files, title).into_iter().next() else {
        return Ok(None);
    };
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.take((hydrus_downloader_exchange::MAX_BYTES + 1) as u64)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    hydrus_downloader_exchange::text_png::decode(&data)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// Export a frozen text payload. Closing never imports or edits its owner.
pub fn open(
    slots: &Slots,
    store: &Arc<Store>,
    payload: String,
    closed: Rc<dyn Fn()>,
) -> Result<PngExportWindow, String> {
    let summary = model::payload_description_with_format(
        &payload,
        &hydrus_gui_model::gui_format::preferences(store),
    );
    open_with_summary(slots, store, payload, summary, closed)
}

/// Export a typed editor's frozen payload with its reference object summary.
pub fn open_with_summary(
    slots: &Slots,
    store: &Arc<Store>,
    payload: String,
    summary: String,
    closed: Rc<dyn Fn()>,
) -> Result<PngExportWindow, String> {
    let title = summary.clone();
    open_described(slots, store, payload, summary, title, closed)
}

/// Export a payload with its reference object summary and the title the
/// panel starts with (a named object's name, as `PNGExportPanel` takes it,
/// else the summary), which also names the file.
pub fn open_described(
    slots: &Slots,
    store: &Arc<Store>,
    payload: String,
    summary: String,
    title: String,
    closed: Rc<dyn Fn()>,
) -> Result<PngExportWindow, String> {
    if let Some(w) = slots.window() {
        return Ok(w);
    }
    let w = crate::app_title::new::<crate::PngExportWindow>().map_err(|e| e.to_string())?;
    w.set_payload_description(summary.into());
    let summary: slint::SharedString = title.into();
    w.set_png_title(summary.clone());
    let directory = store
        .read(settings::get::<model::Directory>)
        .map_err(|e| e.to_string())?;
    if let Some(directory) = directory.0 {
        let name: String = summary
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        w.set_path(
            Path::new(&directory)
                .join(format!("{name}.png"))
                .to_string_lossy()
                .as_ref()
                .into(),
        );
    }
    let alive = Rc::new(Cell::new(true));
    let close = Rc::new({
        let weak = w.as_weak();
        let slot = Rc::downgrade(&slots.0);
        let alive = alive.clone();
        move || {
            if !alive.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            closed();
        }
    });
    let update = Rc::new({
        let weak = w.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                match model::validate(
                    w.get_path().as_str(),
                    w.get_png_title().as_str(),
                    w.get_png_width(),
                ) {
                    Ok(()) => {
                        w.set_export_label("export".into());
                        w.set_can_export(true);
                    }
                    Err(error) => {
                        w.set_export_label(error.into());
                        w.set_can_export(false);
                    }
                }
            }
        }
    });
    w.on_action({
        let weak = w.as_weak();
        let store = store.clone();
        let alive = alive.clone();
        let close = close.clone();
        let update = update.clone();
        move |action| {
            if !alive.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let result = (|| -> Result<(), String> {
                match action.as_str() {
                    "close" => close(),
                    "update" => {
                        w.set_done(false);
                        update();
                    }
                    "browse" => {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("select a path")
                            .add_filter("PNG", &["png"])
                            .save_file()
                        {
                            w.set_path(path.to_string_lossy().as_ref().into());
                            w.set_done(false);
                            update();
                        }
                    }
                    "export" => {
                        let path = w.get_path();
                        let title = w.get_png_title();
                        model::validate(path.as_str(), title.as_str(), w.get_png_width())?;
                        let width = u32::try_from(w.get_png_width()).map_err(|e| e.to_string())?;
                        let data = model::encode_with_summary(
                            &payload,
                            width,
                            title.as_str(),
                            w.get_payload_description().as_str(),
                            w.get_description().as_str(),
                        )?;
                        let path = if path.ends_with(".png") {
                            PathBuf::from(path.as_str())
                        } else {
                            PathBuf::from(format!("{path}.png"))
                        };
                        let directory = path
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .unwrap_or_else(|| Path::new("."));
                        let mut file = tempfile::NamedTempFile::new_in(directory)
                            .map_err(|e| e.to_string())?;
                        std::io::Write::write_all(&mut file, &data).map_err(|e| e.to_string())?;
                        file.persist(&path).map_err(|e| e.to_string())?;
                        let setting =
                            model::Directory(Some(directory.to_string_lossy().into_owned()));
                        store
                            .write(move |ctx| settings::set(ctx.conn(), &setting))
                            .map_err(|e| e.to_string())?;
                        w.set_error("".into());
                        w.set_done(true);
                        let weak = w.as_weak();
                        let alive = alive.clone();
                        slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
                            if alive.get()
                                && let Some(w) = weak.upgrade()
                            {
                                w.set_done(false);
                            }
                        });
                    }
                    _ => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                w.set_error(error.into());
            }
        }
    });
    w.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    *slots.0.borrow_mut() = Some(w.clone_strong());
    LAST.with(|s| *s.borrow_mut() = w.as_weak());
    update();
    w.show().map_err(|e| e.to_string())?;
    Ok(w)
}

/// The "to pngs" panel's window, while it is open.
#[derive(Clone, Default)]
pub struct PngsSlots(pub Rc<RefCell<Option<crate::PngsExportWindow>>>);
impl std::fmt::Debug for PngsSlots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PngsSlots").field(&self.has_open()).finish()
    }
}
impl PngsSlots {
    pub fn has_open(&self) -> bool {
        self.0.borrow().is_some()
    }
    pub fn window(&self) -> Option<crate::PngsExportWindow> {
        self.0
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    }
    pub fn cancel(&self) {
        if let Some(w) = self.window() {
            w.invoke_action("close".into());
        }
    }
}

/// One object of a "to pngs" export: its name (the png's title and file
/// name), its payload description and its serialised text.
#[derive(Debug, Clone)]
pub struct Named {
    pub name: String,
    pub summary: String,
    pub payload: String,
}

/// Export named objects each to its own png in a directory, as the
/// reference's `PNGsExportPanel` does: `{directory}/{name}.png`, titled by
/// the name, with no description; the directory is remembered as the last
/// png export folder, and the panel starts there.
pub fn open_many(
    slots: &PngsSlots,
    store: &Arc<Store>,
    objects: Vec<Named>,
) -> Result<crate::PngsExportWindow, String> {
    if let Some(w) = slots.window() {
        return Ok(w);
    }
    let w = crate::app_title::new::<crate::PngsExportWindow>().map_err(|e| e.to_string())?;
    let directory = store
        .read(settings::get::<model::Directory>)
        .map_err(|e| e.to_string())?;
    if let Some(directory) = directory.0 {
        w.set_path(directory.into());
    }
    let alive = Rc::new(Cell::new(true));
    let close = Rc::new({
        let weak = w.as_weak();
        let slot = Rc::downgrade(&slots.0);
        let alive = alive.clone();
        move || {
            if !alive.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
        }
    });
    let update = Rc::new({
        let weak = w.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                // (`_Update`: only a path is needed)
                if w.get_path().is_empty() {
                    w.set_export_label("select a path".into());
                    w.set_can_export(false);
                } else {
                    w.set_export_label("export".into());
                    w.set_can_export(true);
                }
            }
        }
    });
    w.on_action({
        let weak = w.as_weak();
        let store = store.clone();
        let alive = alive.clone();
        let close = close.clone();
        let update = update.clone();
        move |action| {
            if !alive.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let result = (|| -> Result<(), String> {
                match action.as_str() {
                    "close" => close(),
                    "update" => {
                        w.set_done(false);
                        update();
                    }
                    "browse" => {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_title("select a path")
                            .pick_folder()
                        {
                            w.set_path(path.to_string_lossy().as_ref().into());
                            w.set_done(false);
                            update();
                        }
                    }
                    "export" => {
                        let directory = PathBuf::from(w.get_path().as_str());
                        if directory.as_os_str().is_empty() {
                            return Err("select a path".into());
                        }
                        let width = u32::try_from(w.get_png_width()).map_err(|e| e.to_string())?;
                        let setting =
                            model::Directory(Some(directory.to_string_lossy().into_owned()));
                        store
                            .write(move |ctx| settings::set(ctx.conn(), &setting))
                            .map_err(|e| e.to_string())?;
                        for object in &objects {
                            let data = model::encode_with_summary(
                                &object.payload,
                                width,
                                &object.name,
                                &object.summary,
                                "",
                            )?;
                            let mut path = directory.join(&object.name).into_os_string();
                            if !path.to_string_lossy().ends_with(".png") {
                                path.push(".png");
                            }
                            std::fs::write(&path, &data).map_err(|e| e.to_string())?;
                        }
                        w.set_error("".into());
                        w.set_done(true);
                        let weak = w.as_weak();
                        let alive = alive.clone();
                        slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
                            if alive.get()
                                && let Some(w) = weak.upgrade()
                            {
                                w.set_done(false);
                            }
                        });
                    }
                    _ => {}
                }
                Ok(())
            })();
            if let Err(error) = result {
                w.set_error(error.into());
            }
        }
    });
    w.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    *slots.0.borrow_mut() = Some(w.clone_strong());
    update();
    w.show().map_err(|e| e.to_string())?;
    Ok(w)
}
