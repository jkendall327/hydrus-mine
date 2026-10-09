//! Reference favourites popup actions, staged child editors and durable saves.
use crate::{ImportFavouritePromptWindow, ImportFavouriteRow, ImportOptionsWindow};
use hydrus_core::import_options::{CallerType, ImportOptionsManager, ImportOptionsSlice};
use hydrus_gui_model::import_options_overwrite::{Overwrite, save_favourite};
use hydrus_store::{Store, settings};
use slint::ComponentHandle as _;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

fn menu_label(text: &str) -> String {
    let text = text.replace('&', "&&");
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() > 128 {
        format!(
            "{}…{}",
            chars[..111].iter().collect::<String>(),
            chars[chars.len() - 16..].iter().collect::<String>()
        )
    } else {
        text
    }
}

/// One owning popup's lifetime and its modal children.
pub struct Controller {
    store: Arc<Store>,
    caller: CallerType,
    manager: Option<Rc<RefCell<ImportOptionsManager>>>,
    simple: Cell<bool>,
    current: Rc<dyn Fn() -> Option<ImportOptionsSlice>>,
    applied: Rc<dyn Fn(ImportOptionsSlice)>,
    error: Rc<dyn Fn(String)>,
    busy_changed: Rc<dyn Fn(bool)>,
    active: Cell<bool>,
    busy: Cell<bool>,
    save_current_allowed: Cell<bool>,
    editor: Rc<RefCell<Option<ImportOptionsWindow>>>,
    prompt: Rc<RefCell<Option<ImportFavouritePromptWindow>>>,
    overwrite: crate::import_options_overwrite_window::Slot,
}

impl std::fmt::Debug for Controller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportOptionsFavourites")
            .field("caller", &self.caller)
            .field("active", &self.active.get())
            .field("busy", &self.busy.get())
            .finish_non_exhaustive()
    }
}

impl Controller {
    pub fn new(
        store: Arc<Store>,
        caller: CallerType,
        current: Rc<dyn Fn() -> Option<ImportOptionsSlice>>,
        applied: Rc<dyn Fn(ImportOptionsSlice)>,
        error: Rc<dyn Fn(String)>,
        busy_changed: Rc<dyn Fn(bool)>,
    ) -> Rc<Self> {
        Self::new_inner(store, caller, None, current, applied, error, busy_changed)
    }

    /// Options-page profiles belong to its manager draft until Options Apply.
    pub fn new_staged(
        store: Arc<Store>,
        caller: CallerType,
        manager: Rc<RefCell<ImportOptionsManager>>,
        current: Rc<dyn Fn() -> Option<ImportOptionsSlice>>,
        applied: Rc<dyn Fn(ImportOptionsSlice)>,
        error: Rc<dyn Fn(String)>,
        busy_changed: Rc<dyn Fn(bool)>,
    ) -> Rc<Self> {
        Self::new_inner(
            store,
            caller,
            Some(manager),
            current,
            applied,
            error,
            busy_changed,
        )
    }

    pub(crate) fn new_inner(
        store: Arc<Store>,
        caller: CallerType,
        manager: Option<Rc<RefCell<ImportOptionsManager>>>,
        current: Rc<dyn Fn() -> Option<ImportOptionsSlice>>,
        applied: Rc<dyn Fn(ImportOptionsSlice)>,
        error: Rc<dyn Fn(String)>,
        busy_changed: Rc<dyn Fn(bool)>,
    ) -> Rc<Self> {
        Rc::new(Self {
            store,
            caller,
            manager,
            simple: Cell::new(true),
            current,
            applied,
            error,
            busy_changed,
            active: Cell::new(true),
            busy: Cell::new(false),
            save_current_allowed: Cell::new(true),
            editor: Rc::default(),
            prompt: Rc::default(),
            overwrite: Rc::default(),
        })
    }

    pub fn busy(&self) -> bool {
        self.busy.get()
    }

    /// Subscription lists have no singular current value to save as a profile.
    pub fn set_save_current_allowed(&self, allowed: bool) {
        self.save_current_allowed.set(allowed);
    }

    pub fn set_simple_mode(&self, simple: bool) {
        self.simple.set(simple);
    }

    fn manager(&self) -> Result<ImportOptionsManager, String> {
        self.manager.as_ref().map_or_else(
            || self.store.read(settings::get).map_err(|e| e.to_string()),
            |manager| Ok(manager.borrow().clone()),
        )
    }

    pub fn editing_window(&self) -> Option<ImportOptionsWindow> {
        self.editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    }

    pub fn prompt_window(&self) -> Option<ImportFavouritePromptWindow> {
        self.prompt
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    }

    pub fn overwrite_window(&self) -> Option<crate::ImportOptionsOverwriteWindow> {
        self.overwrite
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    }

    /// Menu data is reread each time it opens, including other owners' saves.
    pub fn rows(&self) -> Result<Vec<ImportFavouriteRow>, String> {
        let manager = self.manager()?;
        let formatting = hydrus_gui_model::gui_format::preferences(&self.store);
        let snapshot = self.store.snapshot();
        let name = |key: &str| {
            hex::decode(key)
                .ok()
                .and_then(|key| {
                    snapshot
                        .services
                        .by_key(&hydrus_core::ServiceKey::new(key))
                        .ok()
                })
                .map_or_else(|| "unknown service".into(), |service| service.name.clone())
        };
        let mut entries = manager.favourites;
        entries.sort_by_key(|(name, _)| hydrus_core::sort::human_sort_key(name));
        Ok(entries
            .into_iter()
            .map(|(entry, options)| {
                let summary =
                    hydrus_gui_model::import_options_editor::container_summary_with_format(
                        &options,
                        &name,
                        &formatting,
                    );
                ImportFavouriteRow {
                    name: entry.clone().into(),
                    label: menu_label(&format!("{entry} - {summary}")).into(),
                    edit_label: menu_label(&format!(
                        "{entry} - {}",
                        if summary.is_empty() {
                            "all default"
                        } else {
                            &summary
                        }
                    ))
                    .into(),
                }
            })
            .collect())
    }

    fn set_busy(&self, busy: bool) {
        self.busy.set(busy);
        (self.busy_changed)(busy);
    }

    fn save(
        &self,
        original: Option<String>,
        name: String,
        options: ImportOptionsSlice,
    ) -> Result<(), String> {
        if let Some(manager) = &self.manager {
            save_favourite(
                &mut manager.borrow_mut(),
                original.as_deref(),
                &name,
                options,
            );
            return Ok(());
        }
        self.store
            .write(move |tx| {
                let mut manager: ImportOptionsManager = settings::get(tx.conn())?;
                save_favourite(&mut manager, original.as_deref(), &name, options);
                settings::set(tx.conn(), &manager)
            })
            .map_err(|e| e.to_string())
    }

    fn delete(&self, name: &str) -> Result<(), String> {
        if let Some(manager) = &self.manager {
            manager
                .borrow_mut()
                .favourites
                .retain(|(entry, _)| entry != name);
            return Ok(());
        }
        let name = name.to_owned();
        self.store
            .write(move |tx| {
                let mut manager: ImportOptionsManager = settings::get(tx.conn())?;
                manager.favourites.retain(|(entry, _)| entry != &name);
                settings::set(tx.conn(), &manager)
            })
            .map_err(|e| e.to_string())
    }

    /// Actions use the current manager and never act after the owner closes.
    pub fn choose(self: &Rc<Self>, action: i32, name: &str) {
        if !self.active.get() || self.busy.get() {
            return;
        }
        let result = self.choose_inner(action, name);
        if let Err(error) = result {
            self.set_busy(false);
            (self.error)(error);
        }
    }

    fn choose_inner(self: &Rc<Self>, action: i32, name: &str) -> Result<(), String> {
        let manager = self.manager()?;
        let favourite = manager
            .favourites
            .into_iter()
            .find(|(entry, _)| entry == name)
            .map(|(_, options)| options);
        match action {
            0 | 1 | 2 | 3 | 6 if favourite.is_none() => return Ok(()),
            3..=6 if self.caller == CallerType::Favourites => return Ok(()),
            _ => {}
        }
        match action {
            0 => (self.applied)(favourite.expect("checked entry")),
            1 => {
                let Some(current) = (self.current)() else {
                    return Ok(());
                };
                let weak = Rc::downgrade(self);
                let applied = Rc::new(move |options| {
                    if let Some(owner) = weak.upgrade().filter(|owner| owner.active.get()) {
                        (owner.applied)(options);
                    }
                });
                let weak = Rc::downgrade(self);
                let closed = Rc::new(move || {
                    if let Some(owner) = weak.upgrade() {
                        owner.set_busy(false);
                    }
                });
                let child = crate::import_options_overwrite_window::open(
                    &self.store,
                    Overwrite::new(
                        self.caller,
                        self.simple.get(),
                        current,
                        favourite.expect("checked entry"),
                    ),
                    &self.overwrite,
                    applied,
                    closed,
                )?;
                *self.overwrite.borrow_mut() = Some(child);
                self.set_busy(true);
            }
            2 => {
                let text = hydrus_downloader_exchange::import_options::encode_text(
                    &favourite.expect("checked entry"),
                )
                .map_err(|e| e.to_string())?;
                crate::to_clipboard(&crate::Clip::Text(text));
            }
            3 | 4 => {
                let original = (action == 3).then(|| name.to_owned());
                let initial_name = if action == 3 {
                    name
                } else {
                    "new import options favourite"
                };
                let options = favourite.unwrap_or_default();
                let weak = Rc::downgrade(self);
                let applied = Rc::new(move |name, options| {
                    if let Some(owner) = weak.upgrade().filter(|owner| owner.active.get())
                        && let Err(error) = owner.save(original.clone(), name, options)
                    {
                        (owner.error)(error);
                    }
                });
                let weak = Rc::downgrade(self);
                let closed = Rc::new(move || {
                    if let Some(owner) = weak.upgrade() {
                        owner.set_busy(false);
                    }
                });
                let child = if let Some(manager) = &self.manager {
                    crate::import_options_window::open_staged(
                        &self.store,
                        CallerType::Favourites,
                        &options,
                        &self.editor,
                        crate::import_options_window::StagedOptions {
                            manager: manager.clone(),
                            simple: self.simple.get(),
                            name: Some(initial_name.to_owned()),
                            url_classes: Vec::new(),
                        },
                        applied,
                        closed,
                    )?
                } else {
                    crate::import_options_window::open_named(
                        &self.store,
                        &options,
                        &self.editor,
                        initial_name,
                        applied,
                        closed,
                    )?
                };
                *self.editor.borrow_mut() = Some(child);
                self.set_busy(true);
            }
            5 => {
                if !self.save_current_allowed.get() {
                    return Ok(());
                }
                let Some(options) = (self.current)() else {
                    return Ok(());
                };
                self.open_prompt(
                    true,
                    "Please enter a name for the new favourite.",
                    Rc::new({
                        let weak = Rc::downgrade(self);
                        move |name| {
                            if let Some(owner) = weak.upgrade().filter(|owner| owner.active.get()) {
                                owner.save(None, name, options.clone())
                            } else {
                                Ok(())
                            }
                        }
                    }),
                )?;
            }
            6 => {
                let name = name.to_owned();
                self.open_prompt(
                    false,
                    &format!("Delete the favourite named \"{name}\"?"),
                    Rc::new({
                        let weak = Rc::downgrade(self);
                        move |_| {
                            weak.upgrade()
                                .filter(|owner| owner.active.get())
                                .map_or(Ok(()), |owner| owner.delete(&name))
                        }
                    }),
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn open_prompt(
        self: &Rc<Self>,
        asking_name: bool,
        message: &str,
        accepted: Rc<dyn Fn(String) -> Result<(), String>>,
    ) -> Result<(), String> {
        let window = crate::app_title::new::<crate::ImportFavouritePromptWindow>()
            .map_err(|e| e.to_string())?;
        window.set_asking_name(asking_name);
        window.set_message(message.into());
        let live = Rc::new(Cell::new(true));
        let close = Rc::new({
            let weak_owner = Rc::downgrade(self);
            let weak = window.as_weak();
            let live = live.clone();
            move || {
                if !live.replace(false) {
                    return;
                }
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
                if let Some(owner) = weak_owner.upgrade() {
                    owner.prompt.borrow_mut().take();
                    owner.set_busy(false);
                }
            }
        });
        window.on_accepted({
            let weak_owner = Rc::downgrade(self);
            let weak = window.as_weak();
            let close = close.clone();
            move |name| {
                if !live.get() || !weak_owner.upgrade().is_some_and(|owner| owner.active.get()) {
                    return;
                }
                match accepted(name.to_string()) {
                    Ok(()) => close(),
                    Err(error) => {
                        if let Some(window) = weak.upgrade() {
                            window.set_error(error.into());
                        }
                    }
                }
            }
        });
        window.on_cancelled({
            let close = close.clone();
            move || close()
        });
        window.window().on_close_requested(move || {
            close();
            slint::CloseRequestResponse::HideWindow
        });
        window.show().map_err(|e| e.to_string())?;
        *self.prompt.borrow_mut() = Some(window);
        self.set_busy(true);
        Ok(())
    }

    /// Cancel descendants and invalidate handles retained by native callers.
    pub fn close(&self) {
        if !self.active.replace(false) {
            return;
        }
        let editor = self.editor.borrow_mut().take();
        if let Some(editor) = editor {
            editor.invoke_cancel();
        }
        let prompt = self.prompt.borrow_mut().take();
        if let Some(prompt) = prompt {
            prompt.invoke_cancelled();
        }
        let overwrite = self.overwrite.borrow_mut().take();
        if let Some(overwrite) = overwrite {
            overwrite.invoke_cancel();
        }
        self.set_busy(false);
    }
}
