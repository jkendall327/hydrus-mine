//! Login request identity and named response parsers owned by a script draft.
use crate::{LoginStepWindow, TableRow};
use hydrus_core::url::strings::StringMatch;
use hydrus_gui_model::{
    formula_editors::FormulaTestData,
    login_workflows::{ArgumentKind, StepEditor},
};
use hydrus_parse::{
    content::{ContentKind, ContentParser},
    login::{CookieRequirement, LoginStep},
};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
/// Step and response parser descendants.
#[derive(Clone, Default)]
pub struct Slots {
    pub step: Rc<RefCell<Option<LoginStepWindow>>>,
    pub parsers: crate::parser_editors_window::Slots,
    pub exchange: crate::downloader_interchange_window::Slots,
    pub cookies: crate::login_cookies_window::Slots,
    pub argument: crate::login_test_window::DomainSlot,
}
/// Accepted step; script persistence still waits for the owner.
pub type Applied = Rc<dyn Fn(LoginStep) -> Result<(), String>>;
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginStepSlots")
            .field("open", &self.step.borrow().is_some())
            .finish_non_exhaustive()
    }
}
impl Slots {
    pub fn cancel(&self) {
        let window = self
            .step
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = window {
            window.invoke_action("cancel".into());
        }
    }
}
fn row(values: Vec<String>, selected: bool) -> TableRow {
    TableRow {
        cells: ModelRc::new(VecModel::from(
            values
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )),
        selected,
    }
}
fn show(window: &LoginStepWindow, editor: &StepEditor) {
    let selected = editor.selection.in_order(&editor.order());
    window.set_content(ModelRc::new(VecModel::from(
        editor
            .order()
            .into_iter()
            .map(|i| {
                let parser = &editor.step.content_parsers[i];
                let produces = match &parser.kind {
                    ContentKind::Variable { name } => format!("temp variable \"{name}\""),
                    ContentKind::Veto { .. } => format!("veto: {}", parser.name),
                    _ => "unsupported content type".into(),
                };
                row(vec![parser.name.clone(), produces], selected.contains(&i))
            })
            .collect::<Vec<_>>(),
    )));
    window.set_one_selected(selected.len() == 1);
    window.set_any_selected(!selected.is_empty());
    let mut variables = Vec::new();
    for kind in [
        ArgumentKind::Credential,
        ArgumentKind::Static,
        ArgumentKind::Temporary,
    ] {
        let selected = editor.selected_arguments(kind);
        let rows = editor
            .arguments(kind)
            .iter()
            .map(|(key, value)| row(vec![key.clone(), value.clone()], selected.contains(key)))
            .collect::<Vec<_>>();
        variables.extend(editor.arguments(kind).iter().map(|(key, value)| {
            row(
                vec![
                    match kind {
                        ArgumentKind::Credential => "credential",
                        ArgumentKind::Static => "static",
                        ArgumentKind::Temporary => "temporary",
                    }
                    .into(),
                    key.clone(),
                    value.clone(),
                ],
                selected.contains(key),
            )
        }));
        let one = selected.len() == 1;
        let any = !selected.is_empty();
        let rows = ModelRc::new(VecModel::from(rows));
        match kind {
            ArgumentKind::Credential => {
                window.set_credential_variables(rows);
                window.set_credential_one(one);
                window.set_credential_any(any);
            }
            ArgumentKind::Static => {
                window.set_static_variables(rows);
                window.set_static_one(one);
                window.set_static_any(any);
            }
            ArgumentKind::Temporary => {
                window.set_temporary_variables(rows);
                window.set_temporary_one(one);
                window.set_temporary_any(any);
            }
        }
    }
    window.set_variables(ModelRc::new(VecModel::from(variables)));
    let selected = editor.cookies.selection.in_order(&editor.cookies.order());
    window.set_cookies(ModelRc::new(VecModel::from(
        editor
            .cookies
            .order()
            .into_iter()
            .map(|i| {
                let cookie = &editor.cookies.rows[i];
                row(
                    vec![
                        cookie.name.describe(false, false),
                        cookie.value.describe(false, false),
                    ],
                    selected.contains(&i),
                )
            })
            .collect::<Vec<_>>(),
    )));
    window.set_cookie_one(selected.len() == 1);
    window.set_cookie_any(!selected.is_empty());
}
fn edit_cookie(
    window: &LoginStepWindow,
    store: &Arc<Store>,
    editor: &Rc<RefCell<StepEditor>>,
    active: &Rc<Cell<bool>>,
    strings: &crate::string_processor_window::Slots,
    index: Option<usize>,
) {
    let cookie = index
        .and_then(|i| editor.borrow().cookies.rows.get(i).cloned())
        .unwrap_or_else(|| CookieRequirement {
            name: StringMatch::any(),
            value: StringMatch::any(),
            reference_auxiliary: None,
        });
    window.set_child_open(true);
    crate::string_processor_window::open_match(
        store,
        &cookie.name,
        strings,
        Rc::new({
            let weak = window.as_weak();
            let active = active.clone();
            let editor = editor.clone();
            let store = store.clone();
            let strings = strings.clone();
            let value = cookie.value;
            move |name| {
                if !active.get() {
                    return;
                }
                let Some(window) = weak.upgrade() else {
                    return;
                };
                window.set_child_open(true);
                crate::string_processor_window::open_match(
                    &store,
                    &value,
                    &strings,
                    Rc::new({
                        let weak = weak.clone();
                        let active = active.clone();
                        let editor = editor.clone();
                        move |value| {
                            if !active.get() {
                                return;
                            }
                            editor.borrow_mut().cookies.put(
                                index,
                                CookieRequirement {
                                    name: name.clone(),
                                    value,
                                    reference_auxiliary: None,
                                },
                            );
                            if let Some(window) = weak.upgrade() {
                                show(&window, &editor.borrow());
                            }
                        }
                    }),
                );
                own_cookie_matcher(&window, &strings, "edit match");
            }
        }),
    );
    own_cookie_matcher(window, strings, "edit cookie name");
}
fn own_cookie_matcher(
    window: &LoginStepWindow,
    strings: &crate::string_processor_window::Slots,
    title: &str,
) {
    let child = strings
        .step
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(child) = child {
        child.set_window_title(title.into());
        let weak = window.as_weak();
        child.on_closed(move || {
            if let Some(window) = weak.upgrade() {
                window.set_child_open(false);
            }
        });
    } else {
        window.set_child_open(false);
    }
}

fn edit_argument(
    window: &LoginStepWindow,
    editor: &Rc<RefCell<StepEditor>>,
    active: &Rc<Cell<bool>>,
    slot: &crate::login_test_window::DomainSlot,
    kind: ArgumentKind,
    old: Option<&str>,
) {
    let initial_value = old
        .and_then(|key| editor.borrow().arguments(kind).get(key).cloned())
        .unwrap_or_default();
    let verb = if old.is_some() { "edit" } else { "enter" };
    let accepted: crate::login_test_window::DomainAccepted = Rc::new({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        let slot = slot.clone();
        let old = old.map(str::to_owned);
        move |key| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Some(key) = key else {
                window.set_child_open(false);
                return;
            };
            if old.as_deref() != Some(key.as_str())
                && editor.borrow().arguments(kind).contains_key(&key)
            {
                window.set_child_open(false);
                window.set_error(format!("That {} already exists!", kind.key_name()).into());
                return;
            }
            let value_name = if kind == ArgumentKind::Static {
                "value"
            } else {
                "parameter name"
            };
            let accepted: crate::login_test_window::DomainAccepted = Rc::new({
                let weak = weak.clone();
                let editor = editor.clone();
                let active = active.clone();
                let old = old.clone();
                move |value| {
                    if !active.get() {
                        return;
                    }
                    let Some(window) = weak.upgrade() else {
                        return;
                    };
                    window.set_child_open(false);
                    let Some(value) = value else {
                        return;
                    };
                    let result =
                        editor
                            .borrow_mut()
                            .set_argument(kind, old.as_deref(), key.clone(), value);
                    match result {
                        Ok(()) => {
                            let index = editor
                                .borrow()
                                .arguments(kind)
                                .keys()
                                .position(|value| value == &key)
                                .unwrap();
                            editor
                                .borrow_mut()
                                .select_arguments(kind, index, false, false);
                            window.set_error("".into());
                            show(&window, &editor.borrow());
                        }
                        Err(error) => window.set_error(error.into()),
                    }
                }
            });
            if let Err(error) = crate::login_test_window::open_text(
                &format!("{verb} the {value_name}"),
                &initial_value,
                true,
                &slot,
                accepted,
            ) {
                window.set_child_open(false);
                window.set_error(error.to_string().into());
            }
        }
    });
    if let Err(error) = crate::login_test_window::open_text(
        &format!("{verb} the {}", kind.key_name()),
        old.unwrap_or_default(),
        false,
        slot,
        accepted,
    ) {
        window.set_child_open(false);
        window.set_error(error.to_string().into());
    }
}
/// Open a detached step editor with VARIABLE/VETO-only content children.
pub fn open(
    store: &Arc<Store>,
    step: &LoginStep,
    slots: &Slots,
    applied: Applied,
) -> Result<LoginStepWindow, slint::PlatformError> {
    if let Some(window) = slots.step.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = crate::app_title::new::<crate::LoginStepWindow>()?;
    window.set_name(step.name.as_str().into());
    window.set_scheme(i32::from(step.scheme == "https"));
    window.set_method(i32::from(step.method == "POST"));
    window.set_has_subdomain(step.subdomain.is_some());
    window.set_subdomain(step.subdomain.as_deref().unwrap_or_default().into());
    window.set_path(step.path.as_str().into());
    let editor = Rc::new(RefCell::new(StepEditor::new(step)));
    show(&window, &editor.borrow());
    let active = Rc::new(Cell::new(true));
    let selected_argument = Rc::new(RefCell::new(None::<(ArgumentKind, String)>));
    let deleting_argument = Rc::new(Cell::new(false));
    let deleting_cookie = Rc::new(Cell::new(false));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&slots.step);
        let parsers = slots.parsers.clone();
        let exchange = slots.exchange.clone();
        let cookies = slots.cookies.clone();
        let argument = slots.argument.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            exchange.cancel();
            parsers.cancel();
            cookies.cancel();
            crate::login_test_window::cancel_domain(&argument);
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
        let editor = editor.clone();
        let parsers = slots.parsers.clone();
        let store = store.clone();
        let active = active.clone();
        move |index| {
            if !active.get() || parsers.content.borrow().is_some() {
                return;
            }
            let parser = index
                .and_then(|i| editor.borrow().step.content_parsers.get(i).cloned())
                .unwrap_or_else(|| {
                    let mut parser = hydrus_gui_model::parser_editors::new_content();
                    parser.kind = ContentKind::Variable {
                        name: String::new(),
                    };
                    parser
                });
            let accepted = Rc::new({
                let weak = weak.clone();
                let editor = editor.clone();
                let active = active.clone();
                move |parser: ContentParser| {
                    if !active.get() {
                        return Err("The login step editor has closed.".into());
                    }
                    editor.borrow_mut().put(index, parser)?;
                    if let Some(window) = weak.upgrade() {
                        show(&window, &editor.borrow());
                    }
                    Ok(())
                }
            });
            match crate::parser_editors_window::open_content(
                &store,
                &parser,
                FormulaTestData::default(),
                &parsers,
                &[7, 8],
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
    window.on_variable_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        let selected = selected_argument.clone();
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
            let row = usize::try_from(index)
                .ok()
                .and_then(|i| editor.borrow().argument_rows().get(i).cloned());
            if let Some((kind, key, _)) = row {
                let index = editor
                    .borrow()
                    .arguments(kind)
                    .keys()
                    .position(|value| value == &key)
                    .unwrap();
                editor
                    .borrow_mut()
                    .select_arguments(kind, index, false, false);
                window.set_variable_kind(kind.index());
                *selected.borrow_mut() = Some((kind, key));
            } else {
                *selected.borrow_mut() = None;
            }
            window.set_variable_selected(selected.borrow().is_some());
            window.set_variable_index(index);
            show(&window, &editor.borrow());
        }
    });
    window.on_argument_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        let selected = selected_argument.clone();
        move |kind, index, ctrl, shift| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_child_open() || window.get_deleting() {
                return;
            }
            let kind = ArgumentKind::from_index(kind);
            if let Ok(index) = usize::try_from(index) {
                editor
                    .borrow_mut()
                    .select_arguments(kind, index, ctrl, shift);
            }
            window.set_variable_kind(kind.index());
            *selected.borrow_mut() = editor.borrow().selected_argument(kind);
            window.set_variable_selected(selected.borrow().is_some());
            show(&window, &editor.borrow());
        }
    });
    window.on_cookie_clicked({
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
                let mut editor = editor.borrow_mut();
                let order = editor.cookies.order();
                editor.cookies.selection.click(&order, index, ctrl, shift);
                show(&window, &editor);
            }
        }
    });
    window.on_content_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move |i, c, s| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_child_open() || window.get_deleting() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let mut editor = editor.borrow_mut();
                let order = editor.order();
                editor.selection.click(&order, i, c, s);
                show(&window, &editor);
            }
        }
    });
    window.on_content_activated({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        let edit = edit.clone();
        move |i| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if window.get_child_open() || window.get_deleting() {
                return;
            }
            let selected = if let Ok(i) = usize::try_from(i) {
                let mut editor = editor.borrow_mut();
                let order = editor.order();
                editor.selection.click(&order, i, false, false);
                editor.selection.one()
            } else {
                None
            };
            if let Some(i) = selected {
                edit(Some(i));
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        let close = close.clone();
        let edit = edit.clone();
        let exchange = slots.exchange.clone();
        let selected_argument = selected_argument.clone();
        let argument = slots.argument.clone();
        let deleting_argument = deleting_argument.clone();
        let deleting_cookie = deleting_cookie.clone();
        let cookies = slots.cookies.clone();
        let store = store.clone();
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
            if window.get_deleting() && !matches!(action.as_str(), "confirm-delete" | "back") {
                return;
            }
            let mapped = match action.as_str() {
                "add-credential" => Some((0, "add-variable")),
                "edit-credential" => Some((0, "edit-variable")),
                "delete-credential" => Some((0, "delete-variable")),
                "add-static" => Some((1, "add-variable")),
                "edit-static" => Some((1, "edit-variable")),
                "delete-static" => Some((1, "delete-variable")),
                "add-temporary" => Some((2, "add-variable")),
                "edit-temporary" => Some((2, "edit-variable")),
                "delete-temporary" => Some((2, "delete-variable")),
                _ => None,
            };
            if let Some((kind, _)) = mapped {
                window.set_variable_kind(kind);
                *selected_argument.borrow_mut() = editor
                    .borrow()
                    .selected_argument(ArgumentKind::from_index(kind));
            }
            let action = mapped.map_or(action.as_str(), |(_, action)| action);
            match action {
                "add-cookie" | "edit-cookie" => {
                    let index = if action == "edit-cookie" {
                        editor.borrow().cookies.selection.one()
                    } else {
                        None
                    };
                    if action == "edit-cookie" && index.is_none() {
                        return;
                    }
                    edit_cookie(&window, &store, &editor, &active, &cookies.strings, index);
                }
                "delete-cookie" => {
                    if !editor.borrow().cookies.selection.is_empty() {
                        deleting_cookie.set(true);
                        window.set_deleting(true);
                    }
                }
                "add-variable" | "edit-variable" => {
                    let selected = if action == "edit-variable" {
                        selected_argument.borrow().clone()
                    } else {
                        None
                    };
                    if action == "edit-variable" && selected.is_none() {
                        return;
                    }
                    let kind = selected.as_ref().map_or(
                        ArgumentKind::from_index(window.get_variable_kind()),
                        |(kind, _)| *kind,
                    );
                    window.set_child_open(true);
                    window.set_error("".into());
                    edit_argument(
                        &window,
                        &editor,
                        &active,
                        &argument,
                        kind,
                        selected.as_ref().map(|(_, key)| key.as_str()),
                    );
                }
                "delete-variable" => {
                    if !editor
                        .borrow()
                        .selected_arguments(ArgumentKind::from_index(window.get_variable_kind()))
                        .is_empty()
                    {
                        deleting_argument.set(true);
                        window.set_deleting(true);
                    }
                }
                "add-content" => edit(None),
                "edit-content" => {
                    let selected = editor.borrow().selection.one();
                    if let Some(i) = selected {
                        edit(Some(i));
                    }
                }
                "delete-content" => {
                    if !editor.borrow().selection.is_empty() {
                        window.set_deleting(true);
                    }
                }
                "confirm-delete" => {
                    if deleting_cookie.replace(false) {
                        editor.borrow_mut().cookies.delete();
                    } else if deleting_argument.replace(false) {
                        editor
                            .borrow_mut()
                            .delete_arguments(ArgumentKind::from_index(window.get_variable_kind()));
                        selected_argument.borrow_mut().take();
                        window.set_variable_selected(false);
                        window.set_variable_index(-1);
                    } else {
                        editor.borrow_mut().delete();
                    }
                    window.set_deleting(false);
                    show(&window, &editor.borrow());
                }
                "back" => {
                    deleting_cookie.set(false);
                    deleting_argument.set(false);
                    window.set_deleting(false);
                }
                "apply" => {
                    let mut step = editor.borrow().value();
                    step.name = window.get_name().to_string();
                    step.scheme = if window.get_scheme() == 0 {
                        "http"
                    } else {
                        "https"
                    }
                    .into();
                    step.method = if window.get_method() == 0 {
                        "GET"
                    } else {
                        "POST"
                    }
                    .into();
                    let subdomain = window.get_subdomain().to_string();
                    step.subdomain =
                        (window.get_has_subdomain() && !subdomain.is_empty()).then_some(subdomain);
                    step.path = window.get_path().to_string();
                    step.cleanse();
                    match applied(step) {
                        Ok(()) => close(),
                        Err(error) => window.set_error(error.into()),
                    }
                }
                "import-content" | "export-content" => {
                    use hydrus_gui_model::downloader_interchange::{Definition, Native};
                    let importing = action == "import-content";
                    let definitions: Vec<_> = editor
                        .borrow()
                        .selection
                        .in_order(&editor.borrow().order())
                        .into_iter()
                        .map(|i| {
                            Definition::new(Native::Content(
                                editor.borrow().step.content_parsers[i].clone(),
                            ))
                        })
                        .collect();
                    let preview: crate::downloader_interchange_window::Preview =
                        Rc::new(|definitions| {
                            let mut draft = StepEditor::new(&LoginStep::default());
                            draft.import(definitions)?;
                            Ok(format!(
                                "{} response parser(s) to add.",
                                draft.step.content_parsers.len()
                            ))
                        });
                    let accepted: crate::downloader_interchange_window::Apply = Rc::new({
                        let weak = weak.clone();
                        let editor = editor.clone();
                        let active = active.clone();
                        move |definitions| {
                            if !active.get() {
                                return Err("The login step editor has closed.".into());
                            }
                            editor.borrow_mut().import(definitions)?;
                            if let Some(window) = weak.upgrade() {
                                show(&window, &editor.borrow());
                            }
                            Ok(())
                        }
                    });
                    match crate::downloader_interchange_window::open(
                        &exchange,
                        importing,
                        &definitions,
                        preview,
                        accepted,
                    ) {
                        Ok(child) => {
                            window.set_child_open(true);
                            let weak = weak.clone();
                            child.on_closed(move || {
                                if let Some(window) = weak.upgrade() {
                                    window.set_child_open(false);
                                }
                            });
                        }
                        Err(error) => window.set_error(error.into()),
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
    *slots.step.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
