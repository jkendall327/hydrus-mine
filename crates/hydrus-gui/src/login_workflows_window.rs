//! Login script management and credential consumers, with parent-owned drafts.
use crate::{LoginScriptWindow, LoginScriptsWindow, TableRow};
use hydrus_gui_model::{
    favourites::non_dupe_name,
    list_selection::ListSelection,
    login_workflows::{ScriptsEditor, script_warning},
};
use hydrus_parse::login::{CredentialDefinition, LoginScript};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};

/// All login editor descendants, canceled when their owner closes.
#[derive(Clone, Default)]
pub struct Slots {
    pub scripts: Rc<RefCell<Option<LoginScriptsWindow>>>,
    pub script: Rc<RefCell<Option<LoginScriptWindow>>>,
    pub definition: crate::login_credential_window::DefinitionSlot,
    pub credentials: crate::login_credential_window::CredentialsSlot,
    pub strings: crate::string_processor_window::Slots,
    pub exchange: crate::downloader_interchange_window::Slots,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginSlots")
            .field("scripts", &self.scripts.borrow().is_some())
            .field("script", &self.script.borrow().is_some())
            .finish_non_exhaustive()
    }
}
impl Slots {
    pub fn cancel(&self) {
        let scripts = self
            .scripts
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = scripts {
            window.invoke_action("cancel".into());
        }
        let script = self
            .script
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = script {
            window.invoke_action("cancel".into());
        }
        crate::login_credential_window::cancel_definition(&self.definition);
        crate::login_credential_window::cancel_credentials(&self.credentials);
        self.strings.cancel_all();
        self.exchange.cancel();
    }
}
fn strings(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        values.into_iter().map(Into::into).collect::<Vec<_>>(),
    ))
}
fn row(values: Vec<String>, selected: bool) -> TableRow {
    TableRow {
        cells: strings(values),
        selected,
    }
}
fn show_scripts(window: &LoginScriptsWindow, editor: &ScriptsEditor) {
    let selected = editor.selected();
    let rows = editor
        .order()
        .into_iter()
        .map(|index| {
            let script = &editor.draft.scripts[index];
            let mut domains = script
                .examples
                .iter()
                .map(|example| example.domain.as_str())
                .collect::<Vec<_>>();
            domains.sort_unstable();
            row(
                vec![script.name.clone(), domains.join(", ")],
                selected.contains(&index),
            )
        })
        .collect::<Vec<_>>();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_one_selected(selected.len() == 1);
    window.set_any_selected(!selected.is_empty());
}
struct ScriptState {
    script: LoginScript,
    selection: ListSelection<usize>,
    credentials: BTreeMap<String, String>,
}
impl ScriptState {
    fn order(&self) -> Vec<usize> {
        let mut order = (0..self.script.credentials.len()).collect::<Vec<_>>();
        order.sort_by(|&a, &b| {
            self.script.credentials[a]
                .name
                .cmp(&self.script.credentials[b].name)
        });
        order
    }
}
fn show_script(window: &LoginScriptWindow, state: &ScriptState) {
    let order = state.order();
    let selected = state.selection.in_order(&order);
    window.set_credentials(ModelRc::new(VecModel::from(
        order
            .into_iter()
            .map(|index| {
                let credential = &state.script.credentials[index];
                row(
                    vec![
                        credential.name.clone(),
                        credential.kind.label().into(),
                        credential.string_match.describe(false, false),
                    ],
                    selected.contains(&index),
                )
            })
            .collect::<Vec<_>>(),
    )));
    window.set_one_credential(selected.len() == 1);
    window.set_any_credential(!selected.is_empty());
    window.set_steps(ModelRc::new(VecModel::from(
        state
            .script
            .steps
            .iter()
            .map(|step| row(vec![step.name.clone()], false))
            .collect::<Vec<_>>(),
    )));
    window.set_cookies(ModelRc::new(VecModel::from(
        state
            .script
            .required_cookies
            .iter()
            .map(|cookie| {
                row(
                    vec![
                        cookie.name.describe(false, false),
                        cookie.value.describe(false, false),
                    ],
                    false,
                )
            })
            .collect::<Vec<_>>(),
    )));
    window.set_examples(ModelRc::new(VecModel::from(
        state
            .script
            .examples
            .iter()
            .map(|example| {
                row(
                    vec![
                        example.domain.clone(),
                        example.access.label().into(),
                        example.description.clone(),
                    ],
                    false,
                )
            })
            .collect::<Vec<_>>(),
    )));
}
/// Script accepted by its list owner; persistence waits for list Apply.
pub type ScriptApplied = Rc<dyn Fn(LoginScript) -> Result<(), String>>;
/// Edit script identity and credential definitions, preserving remaining fields.
pub fn open_script(
    store: &Arc<Store>,
    script: &LoginScript,
    slots: &Slots,
    applied: ScriptApplied,
) -> Result<LoginScriptWindow, slint::PlatformError> {
    if let Some(window) = slots.script.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = LoginScriptWindow::new()?;
    window.set_name(script.name.as_str().into());
    let state = Rc::new(RefCell::new(ScriptState {
        script: script.clone(),
        selection: ListSelection::default(),
        credentials: BTreeMap::new(),
    }));
    show_script(&window, &state.borrow());
    let active = Rc::new(Cell::new(true));
    let deleting = Rc::new(Cell::new(false));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&slots.script);
        let definition = slots.definition.clone();
        let credentials = slots.credentials.clone();
        let strings = slots.strings.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            crate::login_credential_window::cancel_definition(&definition);
            crate::login_credential_window::cancel_credentials(&credentials);
            strings.cancel_all();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    });
    let edit: Rc<dyn Fn(Option<usize>)> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let definition = slots.definition.clone();
        let strings = slots.strings.clone();
        let active = active.clone();
        move |index| {
            if !active.get() || definition.borrow().is_some() {
                return;
            }
            let value = index
                .and_then(|i| state.borrow().script.credentials.get(i).cloned())
                .unwrap_or_default();
            let accepted: crate::login_credential_window::DefinitionApplied = Rc::new({
                let weak = weak.clone();
                let state = state.clone();
                let active = active.clone();
                move |mut value| {
                    if !active.get() {
                        return Err("The login script editor has closed.".into());
                    }
                    let mut state = state.borrow_mut();
                    value.name = non_dupe_name(&value.name, &|name| {
                        state
                            .script
                            .credentials
                            .iter()
                            .enumerate()
                            .any(|(i, old)| Some(i) != index && old.name == name)
                    });
                    let i = if let Some(i) = index {
                        state.script.credentials[i] = value;
                        i
                    } else {
                        state.script.credentials.push(value);
                        state.script.credentials.len() - 1
                    };
                    state.selection.select_only(Some(i));
                    if let Some(window) = weak.upgrade() {
                        show_script(&window, &state);
                    }
                    Ok(())
                }
            });
            match crate::login_credential_window::open_definition(
                &store,
                &value,
                &definition,
                &strings,
                accepted,
            ) {
                Ok(child) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_child_open(true);
                        let weak = weak.clone();
                        child.on_closed(move || {
                            if let Some(window) = weak.upgrade() {
                                window.set_child_open(false);
                            }
                        });
                    }
                }
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(error.to_string().into());
                    }
                }
            }
        }
    });
    window.on_credential_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        let active = active.clone();
        move |index, ctrl, shift| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_child_open() || !window.get_question().is_empty() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                let mut state = state.borrow_mut();
                let order = state.order();
                state.selection.click(&order, index, ctrl, shift);
                show_script(&window, &state);
            }
        }
    });
    window.on_credential_activated({
        let weak = window.as_weak();
        let state = state.clone();
        let edit = edit.clone();
        let active = active.clone();
        move |index| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_child_open() || !window.get_question().is_empty() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                let chosen = {
                    let mut state = state.borrow_mut();
                    let order = state.order();
                    state.selection.click(&order, index, false, false);
                    state.selection.one()
                };
                edit(chosen);
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let state = state.clone();
        let credentials_slot = slots.credentials.clone();
        let active = active.clone();
        let close = close.clone();
        let edit = edit.clone();
        let deleting = deleting.clone();
        move |action| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if action == "cancel" {
                close();
                return;
            }
            if window.get_child_open() {
                return;
            }
            if !window.get_question().is_empty() && !matches!(action.as_str(), "confirm" | "back") {
                return;
            }
            match action.as_str() {
                "add-credential" => edit(None),
                "edit-credential" => {
                    let selected = state.borrow().selection.one();
                    if let Some(index) = selected {
                        edit(Some(index));
                    }
                }
                "delete-credential" => {
                    if !state.borrow().selection.is_empty() {
                        deleting.set(true);
                        window.set_question("Remove all selected?".into());
                    }
                }
                "back" => {
                    deleting.set(false);
                    window.set_question("".into());
                }
                "confirm" if deleting.get() => {
                    let mut state = state.borrow_mut();
                    let mut selected = state.selection.in_order(&state.order());
                    selected.sort_unstable();
                    for index in selected.into_iter().rev() {
                        state.script.credentials.remove(index);
                    }
                    state.selection.select_only(None);
                    deleting.set(false);
                    window.set_question("".into());
                    show_script(&window, &state);
                }
                "apply" | "confirm" => {
                    let mut script = state.borrow().script.clone();
                    script.name = window.get_name().to_string();
                    if action == "apply"
                        && let Some(question) = script_warning(&script)
                    {
                        window.set_question(question.into());
                        return;
                    }
                    match applied(script) {
                        Ok(()) => close(),
                        Err(error) => window.set_error(error.into()),
                    }
                }
                "check-credentials" => {
                    let accepted: crate::login_credential_window::CredentialsApplied = Rc::new({
                        let weak = weak.clone();
                        let state = state.clone();
                        let active = active.clone();
                        move |credentials| {
                            if !active.get() {
                                return Err("The script editor has closed.".into());
                            }
                            let result = state.borrow().script.check_credentials(&credentials);
                            if let Some(window) = weak.upgrade() {
                                window.set_check_result(
                                    result
                                        .map_or_else(
                                            |error| error,
                                            |()| "Credentials are valid for this script.".into(),
                                        )
                                        .into(),
                                );
                            }
                            state.borrow_mut().credentials = credentials;
                            Ok(())
                        }
                    });
                    let result = crate::login_credential_window::open_credentials(
                        &state.borrow().script.credentials,
                        &state.borrow().credentials,
                        &credentials_slot,
                        accepted,
                    );
                    match result {
                        Ok(child) => {
                            window.set_child_open(true);
                            let weak = weak.clone();
                            child.on_closed(move || {
                                if let Some(window) = weak.upgrade() {
                                    window.set_child_open(false);
                                }
                            });
                        }
                        Err(error) => window.set_error(error.to_string().into()),
                    }
                }
                _ => {}
            }
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    *slots.script.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
/// Open the sorted login scripts draft from preserved or saved native settings.
pub fn open_scripts(store: &Arc<Store>, slots: &Slots) -> Result<LoginScriptsWindow, String> {
    if let Some(window) = slots.scripts.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = LoginScriptsWindow::new().map_err(|error| error.to_string())?;
    let editor = Rc::new(RefCell::new(ScriptsEditor::new(
        store
            .read(hydrus_store::logins::load)
            .map_err(|error| error.to_string())?,
    )));
    show_scripts(&window, &editor.borrow());
    let active = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&slots.scripts);
        let child = slots.script.clone();
        let exchange = slots.exchange.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let child = child
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(child) = child {
                child.invoke_action("cancel".into());
            }
            exchange.cancel();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    });
    let children = Slots {
        scripts: Rc::default(),
        script: slots.script.clone(),
        definition: slots.definition.clone(),
        credentials: slots.credentials.clone(),
        strings: slots.strings.clone(),
        exchange: slots.exchange.clone(),
    };
    let edit: Rc<dyn Fn(Option<usize>)> = Rc::new({
        let weak = window.as_weak();
        let editor = editor.clone();
        let store = store.clone();
        let children = children.clone();
        let active = active.clone();
        move |index| {
            if !active.get() || children.script.borrow().is_some() || children.exchange.has_open() {
                return;
            }
            let script = index
                .and_then(|i| editor.borrow().draft.scripts.get(i).cloned())
                .unwrap_or_default();
            let accepted: ScriptApplied = Rc::new({
                let weak = weak.clone();
                let editor = editor.clone();
                let active = active.clone();
                move |script| {
                    if !active.get() {
                        return Err("The script list has closed.".into());
                    }
                    editor.borrow_mut().put(index, script);
                    if let Some(window) = weak.upgrade() {
                        show_scripts(&window, &editor.borrow());
                    }
                    Ok(())
                }
            });
            match open_script(&store, &script, &children, accepted) {
                Ok(child) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_child_open(true);
                        let weak = weak.clone();
                        child.on_closed(move || {
                            if let Some(window) = weak.upgrade() {
                                window.set_child_open(false);
                            }
                        });
                    }
                }
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(error.to_string().into());
                    }
                }
            }
        }
    });
    window.on_row_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move |index, ctrl, shift| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_child_open() || window.get_deleting() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                editor.borrow_mut().click(index, ctrl, shift);
                show_scripts(&window, &editor.borrow());
            }
        }
    });
    window.on_row_activated({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        let edit = edit.clone();
        move |index| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_child_open() || window.get_deleting() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                editor.borrow_mut().click(index, false, false);
                edit(editor.borrow().editing());
            }
        }
    });
    window.on_action({ let weak = window.as_weak(); let editor = editor.clone(); let store = store.clone(); let active = active.clone(); let close = close.clone(); let edit = edit.clone(); let exchange = slots.exchange.clone(); move |action| {
        if !active.get() { return; } let Some(window) = weak.upgrade() else { return; };
        if action == "cancel" { close(); return; } if window.get_child_open() { return; }
        if window.get_deleting() && !matches!(action.as_str(), "confirm-delete" | "back") { return; }
        match action.as_str() {
            "add" => edit(None), "edit" => { let selected = editor.borrow().editing(); if let Some(index) = selected { edit(Some(index)); } }
            "delete" => window.set_deleting(true), "back" => window.set_deleting(false),
            "confirm-delete" => { editor.borrow_mut().delete(); window.set_deleting(false); show_scripts(&window, &editor.borrow()); }
            "apply" => match editor.borrow().save(&store) { Ok(()) => close(), Err(error) => window.set_error(error.to_string().into()) },
            "import" | "export" => {
                let importing = action == "import"; let scripts = editor.borrow().export();
                let preview: crate::downloader_interchange_window::Preview<LoginScript> = Rc::new(|scripts| Ok(format!("{} login script(s) to add. Names and keys are made unique on acceptance.\n\n{}", scripts.len(), scripts.iter().map(|script| script.name.as_str()).collect::<Vec<_>>().join("\n"))));
                let accepted: crate::downloader_interchange_window::Apply<LoginScript> = Rc::new({ let weak = weak.clone(); let editor = editor.clone(); let active = active.clone(); move |scripts| { if !active.get() { return Err("The script list has closed.".into()); } for script in scripts { editor.borrow_mut().put(None, script); } if let Some(window) = weak.upgrade() { show_scripts(&window, &editor.borrow()); } Ok(()) } });
                match crate::downloader_interchange_window::open_login_scripts(&exchange, importing, scripts, preview, accepted) { Ok(child) => { window.set_child_open(true); let weak = weak.clone(); child.on_closed(move || { if let Some(window) = weak.upgrade() { window.set_child_open(false); } }); } Err(error) => window.set_error(error.into()) }
            }
            _ => {}
        }
    } });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|error| error.to_string())?;
    *slots.scripts.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
