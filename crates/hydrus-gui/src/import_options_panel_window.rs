//! A staged manager page owned by Options, with isolated modal descendants.
use crate::{ImportOptionsPanelWindow, ImportOptionsWindow, TableRow};
use hydrus_core::import_options::{CallerType, ImportOptionsSlice, UrlClassKind};
use hydrus_gui_model::import_options_panel::{
    Editor, List, Request, Reset, TLDR, Target, Value, WARNING,
};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
};

pub type Slot = Rc<RefCell<Option<ImportOptionsPanelWindow>>>;
pub type Applied = Rc<dyn Fn(Value) -> Result<(), String>>;
thread_local! { static LAST: RefCell<Option<Weak<Owner>>> = const {RefCell::new(None)}; }

pub fn last_opened() -> Option<ImportOptionsPanelWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(Weak::upgrade))
        .filter(|owner| owner.active.get())
        .and_then(|owner| owner.window.upgrade())
}
pub fn editing_window() -> Option<ImportOptionsWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(Weak::upgrade))
        .and_then(|owner| {
            let editor = owner.editor.borrow();
            editor.as_ref().map(slint::ComponentHandle::clone_strong)
        })
}
pub fn overwrite_window() -> Option<crate::ImportOptionsOverwriteWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(Weak::upgrade))
        .and_then(|owner| {
            let overwrite = owner.overwrite.borrow();
            overwrite.as_ref().map(slint::ComponentHandle::clone_strong)
        })
}
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_cancel();
    }
}

#[derive(Debug)]
enum Pending {
    Clear(Request),
    Delete(Request),
    Reset(Reset),
}
struct Owner {
    store: Arc<Store>,
    state: RefCell<Editor>,
    window: slint::Weak<ImportOptionsPanelWindow>,
    slot: Weak<RefCell<Option<ImportOptionsPanelWindow>>>,
    active: Cell<bool>,
    busy: Cell<bool>,
    pending: RefCell<Option<Pending>>,
    editor: Rc<RefCell<Option<ImportOptionsWindow>>>,
    overwrite: crate::import_options_overwrite_window::Slot,
    favourites: RefCell<Vec<Rc<crate::import_options_favourites_window::Controller>>>,
}
impl std::fmt::Debug for Owner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImportOptionsPanel")
            .field("active", &self.active.get())
            .field("busy", &self.busy.get())
            .finish_non_exhaustive()
    }
}
fn strings(values: Vec<String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        values.into_iter().map(Into::into).collect::<Vec<_>>(),
    ))
}
fn full(slice: &ImportOptionsSlice) -> bool {
    hydrus_gui_model::import_options_editor::Kind::ALL
        .into_iter()
        .all(|kind| kind.is_set(slice))
}
impl Owner {
    fn name(&self, key: &str) -> String {
        hex::decode(key)
            .ok()
            .and_then(|key| {
                self.store
                    .snapshot()
                    .services
                    .by_key(&hydrus_core::ServiceKey::new(key))
                    .ok()
                    .map(|service| service.name.clone())
            })
            .unwrap_or_else(|| "unknown service".into())
    }
    fn ready(&self) -> bool {
        self.active.get()
            && !self.busy.get()
            && self.pending.borrow().is_none()
            && self.window.upgrade().is_some_and(|window| {
                !window.get_resetting() && window.get_information().is_empty()
            })
    }
    fn show(&self) {
        if !self.active.get() {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let mut state = self.state.borrow_mut();
        let formatting = hydrus_gui_model::gui_format::preferences(&self.store);
        state.sync();
        let mut models = Vec::new();
        let mut any = Vec::new();
        let mut one = Vec::new();
        let mut columns = Vec::new();
        let mut ascending = Vec::new();
        for list in List::ALL {
            let rows = state.rows_with_format(list, &|key| self.name(key), &formatting);
            let selected = rows.iter().filter(|row| row.selected).count();
            any.push(selected > 0);
            one.push(selected == 1);
            let (column, up) = state.sort_state(list);
            columns.push(
                column
                    .and_then(|column| i32::try_from(column).ok())
                    .unwrap_or(-1),
            );
            ascending.push(up);
            models.push(ModelRc::new(VecModel::from(
                rows.into_iter()
                    .map(|row| TableRow {
                        cells: strings(row.cells),
                        selected: row.selected,
                    })
                    .collect::<Vec<_>>(),
            )));
        }
        window.set_defaults(models.remove(0));
        window.set_urls(models.remove(0));
        window.set_profiles(models.remove(0));
        window.set_any_selected(ModelRc::new(VecModel::from(any)));
        window.set_one_selected(ModelRc::new(VecModel::from(one)));
        window.set_sort_columns(ModelRc::new(VecModel::from(columns)));
        window.set_ascending(ModelRc::new(VecModel::from(ascending)));
        window.set_busy(self.busy.get());
        window.set_simple(state.ui.simple);
        drop(state);
        if let Some(controller) = self.favourites.borrow().first() {
            match controller.rows() {
                Ok(rows) => window.set_favourites(ModelRc::new(VecModel::from(rows))),
                Err(error) => window.set_error(error.into()),
            }
        }
    }
    fn information(&self, message: impl Into<String>) {
        if let Some(window) = self.window.upgrade() {
            window.set_information(message.into().into());
        }
    }
    fn error(&self, message: impl Into<String>) {
        if let Some(window) = self.window.upgrade() {
            window.set_error(message.into().into());
        }
    }
    fn targets(&self, list: List) -> Vec<Target> {
        self.state.borrow().selected(list, &|key| self.name(key))
    }
    fn one(&self, list: List) -> Option<Target> {
        self.state.borrow().one(list, &|key| self.name(key))
    }
    fn close(&self) {
        if !self.active.replace(false) {
            return;
        }
        for controller in self.favourites.borrow().iter() {
            controller.close();
        }
        let child = self.editor.borrow_mut().take();
        if let Some(child) = child {
            child.invoke_cancel();
        }
        let child = self.overwrite.borrow_mut().take();
        if let Some(child) = child {
            child.invoke_cancel();
        }
        self.pending.borrow_mut().take();
        if let Some(window) = self.window.upgrade() {
            let _ = window.hide();
        }
        if let Some(slot) = self.slot.upgrade() {
            slot.borrow_mut().take();
        }
    }
    fn ask(&self, pending: Pending, message: String, reset: bool) {
        *self.pending.borrow_mut() = Some(pending);
        if let Some(window) = self.window.upgrade() {
            window.set_question(message.into());
            window.set_yes_label(if reset { "let's do it" } else { "yes" }.into());
            window.set_no_label(if reset { "no, hold off" } else { "no" }.into());
        }
    }
    fn edit(self: &Rc<Self>, target: Option<Target>) -> Result<(), String> {
        let state = self.state.borrow();
        let caller = target
            .as_ref()
            .map_or(CallerType::Favourites, Target::caller);
        let own = target
            .as_ref()
            .and_then(|target| state.own(target))
            .unwrap_or_default();
        let name = match &target {
            Some(Target::Favourite(name)) => Some(name.clone()),
            None => Some("new import options favourite/profile".into()),
            _ => None,
        };
        let url_classes = match &target {
            Some(Target::Url(key)) => state
                .classes
                .iter()
                .find(|class| hex::encode(&class.key) == *key)
                .map(|class| {
                    vec![(
                        key.clone(),
                        if class.url_type == hydrus_core::url::UrlType::Watchable {
                            UrlClassKind::Watchable
                        } else {
                            UrlClassKind::Other
                        },
                    )]
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let configuration = crate::import_options_window::StagedOptions {
            manager: state.manager.clone(),
            simple: state.ui.simple,
            name,
            url_classes,
        };
        let description = target.as_ref().map(|target| state.description(target));
        drop(state);
        let applied = Rc::new({
            let owner = Rc::downgrade(self);
            move |name: String, value: ImportOptionsSlice| {
                let Some(owner) = owner.upgrade().filter(|owner| owner.active.get()) else {
                    return;
                };
                if target == Some(Target::Caller(CallerType::Global)) && !full(&value) {
                    owner.error("The global import options must contain every option type.");
                    return;
                }
                match &target {
                    Some(Target::Favourite(original)) => {
                        owner
                            .state
                            .borrow_mut()
                            .save_favourite(Some(original), &name, value);
                    }
                    Some(target) => {
                        owner.state.borrow_mut().set(target, value);
                    }
                    None => {
                        owner.state.borrow_mut().save_favourite(None, &name, value);
                    }
                }
                owner.show();
            }
        });
        let closed = Rc::new({
            let owner = Rc::downgrade(self);
            move || {
                if let Some(owner) = owner.upgrade() {
                    owner.busy.set(false);
                    owner.show();
                }
            }
        });
        let window = crate::import_options_window::open_staged(
            &self.store,
            caller,
            &own,
            &self.editor,
            configuration,
            applied,
            closed,
        )?;
        window.set_default_choice_enabled(caller != CallerType::Global);
        if let Some(description) = description {
            window.set_description(description.into());
        }
        window.set_window_title(
            if caller == CallerType::Favourites {
                "edit favourite/profile import options"
            } else {
                "edit import options"
            }
            .into(),
        );
        *self.editor.borrow_mut() = Some(window);
        self.busy.set(true);
        self.show();
        Ok(())
    }
    fn replace(
        &self,
        targets: &[Target],
        value: &ImportOptionsSlice,
        validate_global: bool,
    ) -> bool {
        if validate_global && targets.contains(&Target::Caller(CallerType::Global)) && !full(value)
        {
            self.information("Hey, the import options that was entered was not full, but the Global entry has to have something for everything. Please try again!");
            return false;
        }
        for target in targets {
            self.state.borrow_mut().set(target, value.clone());
        }
        self.show();
        true
    }
    fn custom(
        self: &Rc<Self>,
        targets: Vec<Target>,
        incoming: ImportOptionsSlice,
    ) -> Result<(), String> {
        let Some(target) = targets.first() else {
            return Ok(());
        };
        let state = self.state.borrow();
        let current = state.own(target).unwrap_or_default();
        let simple = state.ui.simple;
        drop(state);
        if targets.len() > 1 {
            self.information(match target {Target::Caller(_)=>"Hey, multiple items in the default list are selected. I am only going to do this on the topmost selected. If you need to do this to multiple entries, set up one exactly how you want and then copy/replace-paste to the rest.",Target::Url(_)=>"Hey, multiple items in the URL Class list are selected. I am only going to do this on the topmost selected. If you need to do this to multiple entries, set up one exactly how you want and then copy/replace-paste to the rest.",Target::Favourite(_)=>"Hey, multiple items in the favourites list are selected. I am only going to do this on the topmost selected."});
        }
        let caller = target.caller();
        let applied = Rc::new({
            let owner = Rc::downgrade(self);
            move |value: ImportOptionsSlice| {
                if let Some(owner) = owner.upgrade().filter(|owner| owner.active.get()) {
                    owner.replace(&targets, &value, true);
                }
            }
        });
        let closed = Rc::new({
            let owner = Rc::downgrade(self);
            move || {
                if let Some(owner) = owner.upgrade() {
                    owner.busy.set(false);
                    owner.show();
                }
            }
        });
        let child = crate::import_options_overwrite_window::open(
            &self.store,
            hydrus_gui_model::import_options_overwrite::Overwrite::new(
                caller, simple, current, incoming,
            ),
            &self.overwrite,
            applied,
            closed,
        )?;
        *self.overwrite.borrow_mut() = Some(child);
        self.busy.set(true);
        self.show();
        Ok(())
    }
    fn favourite(self: &Rc<Self>, list: List, action: i32, name: &str) -> Result<(), String> {
        if !matches!(list, List::Defaults | List::UrlClasses) {
            return Ok(());
        }
        if matches!(action, 0 | 1) {
            let targets = self.targets(list);
            let value = self
                .state
                .borrow()
                .manager
                .borrow()
                .favourites
                .iter()
                .find(|(entry, _)| entry == name)
                .map(|(_, value)| value.clone());
            if let Some(value) = value {
                if targets.is_empty() {
                    self.information(if list == List::Defaults {
                        "Hey, nothing is selected in the default list--select something and try loading again."
                    } else {
                        "Hey, nothing is selected in the URL Class list--select something and try loading again."
                    });
                } else if action == 0 {
                    self.replace(&targets, &value, true);
                } else {
                    self.custom(targets, value)?;
                }
            }
        } else {
            self.favourites.borrow()[list.index()].choose(action, name);
            self.show();
        }
        Ok(())
    }

    fn action(self: &Rc<Self>, list: List, action: &str) -> Result<(), String> {
        match action {
            "stack" => {
                if let Some(target) = self.one(list)
                    && let Some(message) = self.state.borrow().stack(&target)
                {
                    self.information(message);
                }
            }
            "edit" => {
                if let Some(target) = self.one(list) {
                    self.edit(Some(target))?;
                }
            }
            "add" if list == List::Favourites => self.edit(None)?,
            "clear" => {
                let result = self
                    .state
                    .borrow()
                    .clear_request(list, &|key| self.name(key));
                match result {
                    Ok(Some(request)) => {
                        let message = request.message.clone();
                        self.ask(Pending::Clear(request), message, false);
                    }
                    Err(message) => self.information(message),
                    Ok(None) => {}
                }
            }
            "delete" if list == List::Favourites => {
                let request = self.state.borrow().delete_request(&|key| self.name(key));
                if let Some(request) = request {
                    let message = request.message.clone();
                    self.ask(Pending::Delete(request), message, false);
                }
            }
            "reset" => {
                if let Some(window) = self.window.upgrade() {
                    window.set_resetting(true);
                }
            }
            "copy" => {
                if let Some(target) = self.one(list) {
                    if let Some(value) = self.state.borrow().own(&target) {
                        let text = hydrus_downloader_exchange::import_options::encode_text(&value)
                            .map_err(|error| error.to_string())?;
                        crate::to_clipboard(&crate::Clip::Text(text));
                    } else {
                        self.information("There are no import options set for this url class!");
                    }
                }
            }
            "refresh-favourites" => self.show(),
            "paste-custom" | "paste-merge" | "paste-fill" | "paste-replace" => {
                let targets = self.targets(list);
                if targets.is_empty() {
                    return Ok(());
                }
                let value = hydrus_downloader_exchange::import_options::decode_text(
                    &crate::from_clipboard()?,
                )
                .map_err(|error| error.to_string())?;
                if action == "paste-custom" {
                    self.custom(targets, value)?;
                } else {
                    for target in targets {
                        let mut edited = if action == "paste-replace" {
                            value.clone()
                        } else {
                            self.state.borrow().own(&target).unwrap_or_default()
                        };
                        if action == "paste-merge" {
                            for kind in hydrus_gui_model::import_options_editor::Kind::ALL {
                                if kind.is_set(&value) {
                                    kind.copy(&value, &mut edited);
                                }
                            }
                        } else if action == "paste-fill" {
                            edited.fill_in(&value);
                        }
                        if target == Target::Caller(CallerType::Global) && !full(&edited) {
                            self.information("Hey, you tried to paste a non-full import options container into the \"global\" entry. Did you mean to do a merge-paste instead?");
                        }
                        self.state.borrow_mut().set(&target, edited);
                    }
                    self.show();
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Open a child over a copy; acceptance returns the whole draft to its owner.
pub fn open(
    store: &Arc<Store>,
    value: &Value,
    slot: &Slot,
    applied: Applied,
) -> Result<ImportOptionsPanelWindow, String> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let classes = store
        .read(hydrus_store::settings::get::<hydrus_core::url::UrlClassSettings>)
        .map_err(|error| error.to_string())?;
    let window = crate::app_title::new::<crate::ImportOptionsPanelWindow>()
        .map_err(|error| error.to_string())?;
    window.set_warning(WARNING.into());
    window.set_reset_labels(strings(
        Reset::ALL
            .into_iter()
            .map(|reset| reset.label().into())
            .collect(),
    ));
    window.set_reset_descriptions(strings(
        Reset::ALL
            .into_iter()
            .map(|reset| reset.description().into())
            .collect(),
    ));
    let owner = Rc::new(Owner {
        store: store.clone(),
        state: RefCell::new(Editor::new(value, &classes.url_classes)),
        window: window.as_weak(),
        slot: Rc::downgrade(slot),
        active: Cell::new(true),
        busy: Cell::new(false),
        pending: RefCell::new(None),
        editor: Rc::default(),
        overwrite: Rc::default(),
        favourites: RefCell::new(Vec::new()),
    });
    for list in [List::Defaults, List::UrlClasses] {
        let current = Rc::new({
            let owner = Rc::downgrade(&owner);
            move || {
                let owner = owner.upgrade()?;
                let target = owner.one(list)?;
                let state = owner.state.borrow();
                state.own(&target)
            }
        });
        let apply = Rc::new({
            let owner = Rc::downgrade(&owner);
            move |value: ImportOptionsSlice| {
                if let Some(owner) = owner.upgrade().filter(|owner| owner.active.get()) {
                    owner.replace(&owner.targets(list), &value, true);
                }
            }
        });
        let error = Rc::new({
            let owner = Rc::downgrade(&owner);
            move |error| {
                if let Some(owner) = owner.upgrade() {
                    owner.error(error);
                }
            }
        });
        let busy = Rc::new({
            let owner = Rc::downgrade(&owner);
            move |busy| {
                if let Some(owner) = owner.upgrade() {
                    owner.busy.set(busy);
                    owner.show();
                }
            }
        });
        let controller = crate::import_options_favourites_window::Controller::new_staged(
            store.clone(),
            if list == List::Defaults {
                CallerType::SpecificImporter
            } else {
                CallerType::UrlClass
            },
            owner.state.borrow().manager.clone(),
            current,
            apply,
            error,
            busy,
        );
        controller.set_save_current_allowed(false);
        controller.set_simple_mode(value.ui.simple);
        owner.favourites.borrow_mut().push(controller);
    }
    owner.show();
    window.on_clicked({
        let owner = owner.clone();
        move |list, row, ctrl, shift| {
            if !owner.ready() {
                return;
            }
            if let (Ok(list), Ok(row)) = (usize::try_from(list), usize::try_from(row))
                && let Some(list) = List::from_index(list)
            {
                owner
                    .state
                    .borrow_mut()
                    .click(list, row, ctrl, shift, &|key| owner.name(key));
                owner.show();
            }
        }
    });
    window.on_activated({
        let owner = owner.clone();
        move |list, row| {
            if !owner.ready() {
                return;
            }
            if let (Ok(list), Ok(row)) = (usize::try_from(list), usize::try_from(row))
                && let Some(list) = List::from_index(list)
            {
                owner
                    .state
                    .borrow_mut()
                    .click(list, row, false, false, &|key| owner.name(key));
                if let Err(error) = owner.action(list, "edit") {
                    owner.error(error);
                }
            }
        }
    });
    window.on_sort({
        let owner = owner.clone();
        move |list, column, ascending| {
            if !owner.ready() {
                return;
            }
            if let (Ok(list), Ok(column)) = (usize::try_from(list), usize::try_from(column))
                && let Some(list) = List::from_index(list)
            {
                owner.state.borrow_mut().sort(list, column, ascending);
                owner.show();
            }
        }
    });
    window.on_simple_changed({
        let owner = owner.clone();
        move |simple| {
            if owner.ready() {
                owner.state.borrow_mut().ui.simple = simple;
                for controller in owner.favourites.borrow().iter() {
                    controller.set_simple_mode(simple);
                }
                owner.show();
            }
        }
    });
    window.on_action({
        let owner = owner.clone();
        move |list, action| {
            if !owner.active.get() {
                return;
            }
            if action == "dismiss-information" {
                if let Some(window) = owner.window.upgrade() {
                    window.set_information("".into());
                }
                return;
            }
            if !owner.ready() {
                return;
            }
            match action.as_str() {
                "tldr" => owner.information(TLDR),
                "help" => crate::launch(
                    "https://hydrusnetwork.github.io/hydrus/getting_started_import_options.html",
                ),
                _ => {
                    if let Ok(list) = usize::try_from(list)
                        && let Some(list) = List::from_index(list)
                        && let Err(error) = owner.action(list, &action)
                    {
                        owner.error(error);
                    }
                }
            }
        }
    });
    window.on_favourite({
        let owner = owner.clone();
        move |list, action, name| {
            if !owner.ready() {
                return;
            }
            let Some(list) = usize::try_from(list).ok().and_then(List::from_index) else {
                return;
            };
            if let Err(error) = owner.favourite(list, action, &name) {
                owner.error(error);
            }
        }
    });
    window.on_reset_chosen({
        let owner = owner.clone();
        move |index| {
            if !owner.active.get() || owner.busy.get() {
                return;
            }
            let Some(window) = owner.window.upgrade() else {
                return;
            };
            if !window.get_resetting() {
                return;
            }
            window.set_resetting(false);
            if let Some(reset) = usize::try_from(index)
                .ok()
                .and_then(|index| Reset::ALL.get(index).copied())
            {
                owner.ask(Pending::Reset(reset), reset.question(), true);
            }
        }
    });
    window.on_answered({
        let owner = owner.clone();
        move |yes| {
            if !owner.active.get() || owner.busy.get() {
                return;
            }
            let pending = owner.pending.borrow_mut().take();
            if let Some(window) = owner.window.upgrade() {
                window.set_question("".into());
            }
            if yes {
                match pending {
                    Some(Pending::Clear(request)) => {
                        owner.state.borrow_mut().clear(&request.targets);
                    }
                    Some(Pending::Delete(request)) => {
                        owner.state.borrow_mut().delete(&request.targets);
                    }
                    Some(Pending::Reset(reset)) => owner.state.borrow_mut().reset(reset),
                    None => {}
                }
            }
            owner.show();
        }
    });
    window.on_apply({
        let owner = owner.clone();
        move || {
            if !owner.ready() {
                return;
            }
            let value = owner.state.borrow().value();
            match applied(value) {
                Ok(()) => owner.close(),
                Err(error) => owner.error(error),
            }
        }
    });
    window.on_cancel({
        let owner = owner.clone();
        move || owner.close()
    });
    window.window().on_close_requested({
        let owner = owner.clone();
        move || {
            owner.close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    LAST.with(|last| *last.borrow_mut() = Some(Rc::downgrade(&owner)));
    window.show().map_err(|error| error.to_string())?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
