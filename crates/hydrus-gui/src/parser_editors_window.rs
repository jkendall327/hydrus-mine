//! Native page/content parser windows with staged persistence and owned children.
use crate::{
    DefinitionField, ParserEditWindow, ParserListWindow, ParserPickerWindow, TableColumn, TableRow,
};
use hydrus_gui_model::formula_editors::FormulaTestData;
use hydrus_gui_model::list_selection::ListSelection;
use hydrus_gui_model::parser_editors::{self as model, ContentEditor, Draft, TestContext};
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

/// Retain parser dialogs and all their child formula/string windows.
#[derive(Clone, Default)]
pub struct Slots {
    pub exchange: crate::downloader_interchange_window::Slots,
    pub list: Rc<RefCell<Option<ParserListWindow>>>,
    pub links: Rc<RefCell<Option<ParserListWindow>>>,
    pub picker: Rc<RefCell<Option<ParserPickerWindow>>>,
    pub page: Rc<RefCell<Option<ParserEditWindow>>>,
    pub content: Rc<RefCell<Option<ParserEditWindow>>>,
    pub formula: crate::formula_window::Slots,
    /// Every recursive child page owns separate page/content/formula slots.
    pub child: Rc<RefCell<Option<Box<Slots>>>>,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots").finish_non_exhaustive()
    }
}
impl Slots {
    /// Cancel every descendant when a caller closes its parser owner.
    pub fn cancel(&self) {
        let list = self
            .links
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .or_else(|| {
                self.list
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
            });
        if let Some(list) = list {
            list.invoke_force_close();
        }
        let window = self
            .page
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .or_else(|| {
                self.content
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
            });
        if let Some(window) = window {
            window.invoke_force_close();
        } else {
            self.formula.cancel();
            self.exchange.cancel();
        }
    }
}
fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items.into_iter().map(Into::into).collect::<Vec<_>>(),
    ))
}
fn table(items: impl IntoIterator<Item = (Vec<String>, bool)>) -> ModelRc<TableRow> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(|(cells, selected)| TableRow {
                cells: strings(cells),
                selected,
            })
            .collect::<Vec<_>>(),
    ))
}
fn field(
    id: i32,
    label: &str,
    kind: i32,
    text: String,
    options: &[&str],
    chosen: i32,
    checked: bool,
) -> DefinitionField {
    DefinitionField {
        id,
        label: label.into(),
        kind,
        text: text.into(),
        options: strings(options.iter().map(|s| (*s).to_owned())),
        chosen,
        checked,
        enabled: true,
    }
}
fn text(id: i32, label: &str, value: &str) -> DefinitionField {
    field(id, label, 0, value.into(), &[], 0, false)
}
fn choice(id: i32, label: &str, values: &[&str], at: usize) -> DefinitionField {
    field(
        id,
        label,
        1,
        String::new(),
        values,
        i32::try_from(at).unwrap_or(0),
        false,
    )
}
fn check(id: i32, label: &str, value: bool) -> DefinitionField {
    field(id, label, 2, String::new(), &[], 0, value)
}
#[derive(Clone)]
enum Value {
    Page(Box<PageParser>),
    Content(Box<ContentEditor>),
}
struct Editor {
    errors: std::collections::BTreeMap<i32, String>,
    fetch_control: hydrus_gui_model::network_job_control::Control,
    value: Value,
    original: Value,
    selected: Option<usize>,
    subsidiary_selected: Option<usize>,
    subsidiary_selection: ListSelection<usize>,
    pending_subsidiary_delete: Option<Vec<(usize, hydrus_parse::content::SubsidiaryPageParser)>>,
    test: FormulaTestData,
    raw_mimes: hydrus_gui_model::parser_test_data::ExampleMimes,
    example: usize,
    permitted_types: Vec<usize>,
    subsidiary: Option<Rc<RefCell<model::SubsidiaryEditor>>>,
}
fn fields(value: &Value, permitted_types: &[usize]) -> Vec<DefinitionField> {
    match value {
        Value::Page(p) => vec![
            text(0, "name or description", &p.name),
            field(
                1,
                "example URLs (one per line)",
                3,
                p.example_urls.join("\n"),
                &[],
                0,
                false,
            ),
        ],
        Value::Content(e) => {
            let types = permitted_types
                .iter()
                .map(|i| model::CONTENT_TYPES[*i])
                .collect::<Vec<_>>();
            let mut fields = vec![
                text(0, "name or description", &e.parser.name),
                choice(
                    1,
                    "content type",
                    &types,
                    permitted_types
                        .iter()
                        .position(|i| *i == e.kind_index())
                        .unwrap_or(0),
                ),
            ];
            match &e.parser.kind {
                ContentKind::Url { url_type, priority } => {
                    fields.push(choice(
                        2,
                        "URL type",
                        &[
                            "download/pursue (file/post)",
                            "associate (source)",
                            "next gallery page",
                            "sub-gallery page",
                        ],
                        [7, 8, 6, 9].iter().position(|t| t == url_type).unwrap_or(0),
                    ));
                    fields.push(text(3, "priority (0–100)", &priority.to_string()));
                }
                ContentKind::Tag { namespace } => {
                    fields.push(check(4, "any namespace", namespace.is_none()));
                    let mut namespace_field = text(
                        5,
                        "namespace (empty forces unnamespaced)",
                        e.namespace_text(),
                    );
                    namespace_field.enabled = namespace.is_some();
                    fields.push(namespace_field);
                }
                ContentKind::Note { name } => fields.push(text(5, "note name", name)),
                ContentKind::Hash {
                    hash_type,
                    encoding,
                } => {
                    fields.push(choice(
                        6,
                        "hash type",
                        &["md5", "sha1", "sha256", "sha512"],
                        ["md5", "sha1", "sha256", "sha512"]
                            .iter()
                            .position(|t| *t == hash_type)
                            .unwrap_or(0),
                    ));
                    fields.push(choice(
                        7,
                        "encoding",
                        &["hex", "base64"],
                        usize::from(encoding == "base64"),
                    ));
                }
                ContentKind::Timestamp { .. } => {
                    fields.push(choice(8, "timestamp type", &["source time"], 0));
                }
                ContentKind::Title { priority } => {
                    fields.push(text(3, "priority (0–100)", &priority.to_string()));
                }
                ContentKind::HttpHeader { name } => fields.push(text(5, "header name", name)),
                ContentKind::Variable { name } => fields.push(text(5, "variable name", name)),
                ContentKind::Veto {
                    if_matches_found,
                    string_match,
                } => {
                    fields.push(check(9, "veto if match found", *if_matches_found));
                    fields.push(text(
                        10,
                        "string match",
                        &string_match.describe(false, false),
                    ));
                    fields.last_mut().unwrap().enabled = false;
                }
            }
            fields
        }
    }
}
fn show_editor(w: &ParserEditWindow, e: &Editor) {
    w.set_fetch_has_error(e.fetch_control.error().is_some());
    w.set_subsidiary(e.subsidiary.is_some());
    if let Some(details) = &e.subsidiary {
        let details = details.borrow();
        w.set_own_separator(
            hydrus_gui_model::formula_editors::formula_summary(&details.formula).into(),
        );
        w.set_own_sorted(details.sort_by_source_time);
    }
    w.set_examples(strings(e.test.examples.iter().enumerate().map(
        |(i, text)| format!("example {} ({} characters)", i + 1, text.chars().count()),
    )));
    w.set_example(i32::try_from(e.example).unwrap_or(0));
    let raw = hydrus_gui_model::parser_test_data::preview(
        &e.test.text,
        e.raw_mimes.get(e.example, &e.test.text),
    );
    w.set_raw_description(raw.description.into());
    w.set_raw_preview(raw.text.into());
    w.set_parse_enabled(raw.parse_enabled);
    let mut editor_fields = fields(&e.value, &e.permitted_types);
    if e.subsidiary.is_some() {
        editor_fields.retain(|field| field.id != 1);
    }
    w.set_fields(ModelRc::new(VecModel::from(editor_fields)));
    if let Value::Page(p) = &e.value {
        w.set_nodes(table(p.content_parsers.iter().enumerate().map(|(i, c)| {
            (
                vec![
                    c.name.clone(),
                    model::CONTENT_TYPES[model::kind_index(&c.kind)].into(),
                ],
                e.selected == Some(i),
            )
        })));
        w.set_subsidiaries(table(p.subsidiary.iter().enumerate().map(|(i, child)| {
            (
                vec![
                    child.parser.name.clone(),
                    hydrus_gui_model::formula_editors::formula_summary(&child.formula),
                ],
                e.subsidiary_selection.is_selected(i),
            )
        })));
        w.set_subsidiary_selected(e.subsidiary_selected.is_some());
        w.set_subsidiary_exportable(!e.subsidiary_selection.is_empty());
        w.set_subsidiary_sorted(
            e.subsidiary_selected
                .and_then(|i| p.subsidiary.get(i))
                .is_some_and(|child| child.sort_by_source_time),
        );
        w.set_subsidiary_note("Each subsidiary separates this page into documents and parses them with its own recursive page parser.".into());
    }
    w.set_selected(e.selected.is_some());
}
fn test_data(
    w: &ParserEditWindow,
    e: &Editor,
    collapse_newlines: bool,
) -> Result<FormulaTestData, String> {
    let context = TestContext::parse(
        w.get_test_url().to_string(),
        w.get_post_index().as_str(),
        w.get_variables().as_str(),
    )?;
    let mut test = e.test.clone();
    test.context = context.values();
    test.collapse_newlines = collapse_newlines;
    test.remember_example(e.example, w.get_document().to_string());
    Ok(test.selected_first(e.example))
}
fn child_test_data(
    w: &ParserEditWindow,
    e: &Editor,
    collapse: bool,
) -> Result<FormulaTestData, String> {
    let mut test = test_data(w, e, collapse)?;
    if let Value::Page(page) = &e.value {
        if let Some(details) = &e.subsidiary {
            test = details.borrow().child_test_data(page, &test)?;
        } else {
            test.examples = test
                .examples
                .iter()
                .map(|text| page.converter.convert(text).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            test.text = test.examples.first().cloned().unwrap_or_default();
        }
    }
    test.collapse_newlines = collapse;
    Ok(test)
}
fn edit_text(value: &mut Value, id: i32, value_text: String) -> Result<(), String> {
    match value {
        Value::Page(p) => match id {
            0 => p.name = value_text,
            1 => {
                p.example_urls = value_text
                    .lines()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            _ => (),
        },
        Value::Content(e) => match id {
            0 => e.parser.name = value_text,
            3 => {
                let priority: i64 = value_text
                    .parse()
                    .map_err(|_| "Priority must be an integer from 0 to 100.".to_owned())?;
                if !(0..=100).contains(&priority) {
                    return Err("Priority must be from 0 to 100.".into());
                }
                if let ContentKind::Url { priority: old, .. }
                | ContentKind::Title { priority: old } = &mut e.parser.kind
                {
                    *old = priority;
                }
            }
            5 => match &mut e.parser.kind {
                ContentKind::Tag { namespace: Some(n) } => *n = value_text,
                ContentKind::Note { name }
                | ContentKind::HttpHeader { name }
                | ContentKind::Variable { name } => *name = value_text,
                _ => (),
            },

            _ => (),
        },
    }
    Ok(())
}
fn edit_choice(value: &mut Value, id: i32, index: usize) {
    if let Value::Content(e) = value {
        match id {
            1 => e.change_kind(index),
            2 => {
                if let ContentKind::Url { url_type, .. } = &mut e.parser.kind
                    && let Some(t) = [7, 8, 6, 9].get(index)
                {
                    *url_type = *t;
                }
            }
            6 => {
                if let ContentKind::Hash { hash_type, .. } = &mut e.parser.kind
                    && let Some(t) = ["md5", "sha1", "sha256", "sha512"].get(index)
                {
                    *hash_type = (*t).into();
                }
            }
            7 => {
                if let ContentKind::Hash { encoding, .. } = &mut e.parser.kind {
                    *encoding = if index == 1 { "base64" } else { "hex" }.into();
                }
            }
            8 => {
                if index == 0
                    && let ContentKind::Timestamp { timestamp_type } = &mut e.parser.kind
                {
                    *timestamp_type = Some(hydrus_parse::content::TIMESTAMP_MODIFIED_DOMAIN);
                }
            }
            _ => (),
        }
    }
}
fn child_open(slots: &Slots, page: bool) -> bool {
    (page && slots.content.borrow().is_some())
        || slots.formula.formula.borrow().is_some()
        || slots.formula.strings.has_open()
        || slots.exchange.has_open()
        || slots.child.borrow().is_some()
}
type Done = Rc<dyn Fn(Value) -> Result<(), String>>;
/// Open a staged reusable content editor, restricting selectable kinds to the
/// caller's `CONTENT_TYPES` indices. Formula context accepts arbitrary named
/// variables. Cancel leaves the caller's parser untouched; Apply returns its draft.
pub fn open_content(
    store: &Arc<Store>,
    parser: &ContentParser,
    test: FormulaTestData,
    slots: &Slots,
    permitted_types: &[usize],
    applied: Rc<dyn Fn(ContentParser) -> Result<(), String>>,
) -> Result<ParserEditWindow, slint::PlatformError> {
    let permitted = permitted_types.to_vec();
    let done: Done = Rc::new(move |value| {
        let Value::Content(content) = value else {
            return Ok(());
        };
        let parser = content.value();
        if !permitted.contains(&model::kind_index(&parser.kind)) {
            return Err("This content type is not permitted here.".into());
        }
        applied(parser)
    });
    let window = open_editor(
        store,
        Value::Content(Box::new(ContentEditor::new(parser, test.clone()))),
        test,
        slots,
        done,
        Some(permitted_types),
        None,
    )?;
    *slots.content.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
fn open_editor(
    store: &Arc<Store>,
    value: Value,
    mut test: FormulaTestData,
    slots: &Slots,
    applied: Done,
    permitted_types: Option<&[usize]>,
    subsidiary: Option<Rc<RefCell<model::SubsidiaryEditor>>>,
) -> Result<ParserEditWindow, slint::PlatformError> {
    slots.formula.strings.set_store(store);
    test.prepare_examples();
    let w = ParserEditWindow::new()?;
    let page = matches!(value, Value::Page(_));
    w.set_page(page);
    w.set_document(test.text.as_str().into());
    w.set_test_url(test.context.get("url").cloned().unwrap_or_default().into());
    w.set_fetch_url(test.context.get("url").cloned().unwrap_or_default().into());
    w.set_post_index(
        test.context
            .get("post_index")
            .cloned()
            .unwrap_or_else(|| "0".into())
            .into(),
    );
    w.set_variables(
        test.context
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "url" | "post_index"))
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
    );
    let state = Rc::new(RefCell::new(Editor {
        errors: std::collections::BTreeMap::new(),
        fetch_control: hydrus_gui_model::network_job_control::Control::default(),
        raw_mimes: hydrus_gui_model::parser_test_data::ExampleMimes::default(),
        original: value.clone(),
        value,
        selected: None,
        subsidiary_selected: None,
        subsidiary_selection: ListSelection::default(),
        pending_subsidiary_delete: None,
        test,
        example: 0,
        permitted_types: permitted_types.map_or_else(
            || (0..model::CONTENT_TYPES.len()).collect(),
            |types| {
                types
                    .iter()
                    .copied()
                    .filter(|i| *i < model::CONTENT_TYPES.len())
                    .collect()
            },
        ),
        subsidiary,
    }));
    let active = Rc::new(Cell::new(true));
    let fetch = crate::parser_test_fetch::Slot::default();
    let fetch_errors = crate::network_job_control::Errors::default();
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        let slots = slots.clone();
        let fetch = fetch.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                show_editor(&w, &state.borrow());
                w.set_fetching(fetch.busy());
                w.set_child_open(child_open(&slots, page) || fetch.busy());
            }
        }
    });
    let blocked: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let slots = slots.clone();
        let fetch = fetch.clone();
        let weak = w.as_weak();
        move || {
            !active.get()
                || child_open(&slots, page)
                || fetch.busy()
                || weak.upgrade().is_none_or(|w| !w.get_question().is_empty())
        }
    });
    w.on_raw_action({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |action| {
            if blocked() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            match action.as_str() {
                "copy" => crate::copy_to_clipboard(w.get_document().as_str()),
                "paste" => match crate::from_clipboard() {
                    Ok(text) => {
                        let mut e = state.borrow_mut();
                        let index = e.example;
                        e.test.remember_example(index, text.clone());
                        e.raw_mimes.remember(index, &text, None);
                        w.set_document(text.into());
                        w.set_fetch_status("Pasted!".into());
                        drop(e);
                        refresh();
                    }
                    Err(error) => {
                        w.set_error(format!("Problem loading!\n\n{error}").into());
                    }
                },
                _ => {}
            }
        }
    });
    w.on_fetch_error_action({
        let state = state.clone();
        let active = active.clone();
        let fetch_errors = fetch_errors.clone();
        move |action| {
            if !active.get() || !page || state.borrow().subsidiary.is_some() {
                return;
            }
            let error = state.borrow().fetch_control.error().map(str::to_owned);
            if let Some(error) = error {
                match action {
                    8 => {
                        let _ = fetch_errors.show(&error);
                    }
                    9 => crate::copy_to_clipboard(&error),
                    _ => (),
                }
            }
        }
    });
    w.on_cancel_fetch({
        let fetch = fetch.clone();
        move || fetch.cancel()
    });
    let fetch_request: Rc<dyn Fn(bool)> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        let active = active.clone();
        let fetch = fetch.clone();
        let store = store.clone();
        move |example_url| {
            if blocked() || (example_url && !page) {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let raw = if example_url {
                w.get_test_url()
            } else {
                w.get_fetch_url()
            }
            .trim()
            .to_owned();
            if raw.is_empty() {
                if example_url
                    && let Value::Page(parser) = &state.borrow().value
                    && let Some(url) = parser.example_urls.first()
                {
                    w.set_test_url(url.as_str().into());
                }
                return;
            }
            let url = if example_url {
                hydrus_core::url::ensure_url_is_encoded(&raw, true, false)
            } else {
                raw
            };
            if let Err(error) = hydrus_core::url::functions::check_full_url(&url) {
                w.set_error(error.to_string().into());
                return;
            }
            {
                let mut editor = state.borrow_mut();
                match test_data(&w, &editor, true) {
                    Ok(mut test) => {
                        test.examples.rotate_right(editor.example);
                        test.source_urls.rotate_right(editor.example);
                        editor.test = test;
                    }
                    Err(error) => {
                        w.set_error(error.into());
                        return;
                    }
                }
            }
            let mut request = hydrus_net::Request::get(url.clone());
            request.one_shot = example_url;
            let referral = w.get_referral_url().trim().to_owned();
            request.referral_url = (example_url && !referral.is_empty()).then_some(referral);
            // Only the page example-data owner has NetworkJobControl in Qt.
            if example_url {
                state.borrow_mut().fetch_control.clear_error();
            }
            w.set_fetch_status("initialising…".into());
            w.set_error(SharedString::new());
            let progress = Rc::new({
                let weak = weak.clone();
                let active = active.clone();
                move |status: String| {
                    if active.get()
                        && let Some(w) = weak.upgrade()
                    {
                        w.set_fetch_status(status.into());
                    }
                }
            });
            let completed = Rc::new({
                let weak = weak.clone();
                let active = active.clone();
                let state = state.clone();
                let refresh = refresh.clone();
                move |outcome: crate::parser_test_fetch::Outcome| {
                    if !active.get() {
                        return;
                    }
                    let Some(w) = weak.upgrade() else {
                        return;
                    };
                    let mut editor = state.borrow_mut();
                    if example_url
                        && let hydrus_gui_model::formula_editors::FetchedDocument::Failed {
                            error,
                            text,
                        } = &outcome.document
                    {
                        // Preserve native failure details after the live job is removed.
                        let detail = if text.is_empty() {
                            error.clone()
                        } else {
                            format!("{error}\n\n{text}")
                        };
                        editor.fetch_control.set_error(detail);
                    }
                    editor.example =
                        editor
                            .test
                            .fetched(url.clone(), outcome.document, example_url);
                    let index = editor.example;
                    let text = editor.test.text.clone();
                    editor.raw_mimes.remember(index, &text, outcome.mime);
                    w.set_document(editor.test.text.as_str().into());
                    w.set_test_url(url.as_str().into());
                    w.set_post_index("0".into());
                    w.set_fetch_status("fetch finished".into());
                    if let Some(error) = outcome.accounting_error {
                        w.set_error(format!("Could not save bandwidth usage: {error}").into());
                    }
                    drop(editor);
                    refresh();
                }
            });
            fetch.start(store.clone(), request, progress, completed);
            refresh();
        }
    });
    w.on_fetch({
        let fetch_request = fetch_request.clone();
        move || fetch_request(true)
    });
    w.on_fetch_from_url(move || fetch_request(false));
    w.on_text_edited({
        let state = state.clone();
        let weak = w.as_weak();
        let blocked = blocked.clone();
        move |id, t| {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let mut editor = state.borrow_mut();
                let result = edit_text(&mut editor.value, id, t.to_string());
                match result {
                    Ok(()) => {
                        editor.errors.remove(&id);
                    }
                    Err(error) => {
                        editor.errors.insert(id, error);
                    }
                }
                w.set_error(
                    editor
                        .errors
                        .values()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n")
                        .into(),
                );
            }
        }
    });
    w.on_choice_edited({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |id, index| {
            if blocked() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                let mut e = state.borrow_mut();
                let index = if id == 1 && matches!(e.value, Value::Content(_)) {
                    let Some(index) = e.permitted_types.get(index).copied() else {
                        return;
                    };
                    index
                } else {
                    index
                };
                edit_choice(&mut e.value, id, index);
                if id == 1 {
                    e.errors.clear();
                }
            }
            refresh();
        }
    });
    w.on_toggled({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |id, v| {
            if blocked() {
                return;
            }
            if let Value::Content(e) = &mut state.borrow_mut().value {
                if id == 4 {
                    e.set_any_namespace(v);
                }
                if id == 9
                    && let ContentKind::Veto {
                        if_matches_found, ..
                    } = &mut e.parser.kind
                {
                    *if_matches_found = v;
                }
            }
            refresh();
        }
    });
    w.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |row, _, _| {
            if blocked() {
                return;
            }
            if let Ok(index) = usize::try_from(row) {
                let mut s = state.borrow_mut();
                if let Value::Page(p) = &s.value
                    && index < p.content_parsers.len()
                {
                    s.selected = Some(index);
                }
            }
            refresh();
        }
    });
    w.on_example_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        let weak = w.as_weak();
        move || {
            if blocked() {
                refresh();
                return;
            }
            if let Some(w) = weak.upgrade() {
                let mut e = state.borrow_mut();
                e.test
                    .context
                    .insert("url".into(), w.get_test_url().to_string());
                let old = e.example;
                e.test.remember_example(old, w.get_document().to_string());
                let next = usize::try_from(w.get_example()).unwrap_or(0);
                if e.test.choose_example(next) {
                    e.example = next;
                }
                w.set_document(e.test.text.as_str().into());
                w.set_test_url(
                    e.test
                        .context
                        .get("url")
                        .cloned()
                        .unwrap_or_default()
                        .into(),
                );
            }
            refresh();
        }
    });
    w.on_subsidiary_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |row, ctrl, shift| {
            if blocked() {
                return;
            }
            if let Ok(index) = usize::try_from(row) {
                let mut e = state.borrow_mut();
                if let Value::Page(page) = &e.value
                    && index < page.subsidiary.len()
                {
                    let order = (0..page.subsidiary.len()).collect::<Vec<_>>();
                    e.subsidiary_selection.click(&order, index, ctrl, shift);
                    e.subsidiary_selected = e.subsidiary_selection.one();
                }
            }
            refresh();
        }
    });
    w.on_subsidiary_sort_changed({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |sorted| {
            if blocked() {
                return;
            }
            let mut e = state.borrow_mut();
            if let Some(index) = e.subsidiary_selected
                && let Value::Page(page) = &mut e.value
                && let Some(child) = page.subsidiary.get_mut(index)
            {
                child.sort_by_source_time = sorted;
            }
            drop(e);
            refresh();
        }
    });
    w.on_own_sort_changed({
        let state = state.clone();
        let blocked = blocked.clone();
        move |sorted| {
            if !blocked()
                && let Some(details) = &state.borrow().subsidiary
            {
                details.borrow_mut().sort_by_source_time = sorted;
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let active = active.clone();
        let slots = slots.clone();
        let fetch = fetch.clone();
        let fetch_errors = fetch_errors.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            fetch.stop();
            fetch_errors.cancel();
            let child = slots.child.borrow_mut().take();
            if let Some(child) = child {
                child.cancel();
            }
            if page {
                let child = slots
                    .content
                    .borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong);
                if let Some(child) = child {
                    child.invoke_force_close();
                }
            }
            slots.exchange.cancel();
            slots.formula.cancel();
            if page {
                slots.page.borrow_mut().take();
            } else {
                slots.content.borrow_mut().take();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
                w.invoke_closed();
            }
        }
    });
    w.on_force_close({
        let close = close.clone();
        move || close()
    });
    w.on_answered({
        let weak = w.as_weak();
        let close = close.clone();
        let active = active.clone();
        let slots = slots.clone();
        let fetch = fetch.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        move |yes| {
            if !active.get() || child_open(&slots, page) || fetch.busy() {
                return;
            }
            let pending = state.borrow_mut().pending_subsidiary_delete.take();
            if let Some(rows) = pending {
                if yes {
                    let mut e = state.borrow_mut();
                    if let Value::Page(page) = &mut e.value {
                        if rows
                            .iter()
                            .all(|(i, original)| page.subsidiary.get(*i) == Some(original))
                        {
                            for (i, _) in rows.into_iter().rev() {
                                page.subsidiary.remove(i);
                            }
                            e.subsidiary_selection = ListSelection::default();
                            e.subsidiary_selected = None;
                        } else if let Some(w) = weak.upgrade() {
                            w.set_error(
                                "The subsidiary queue changed while deletion was pending.".into(),
                            );
                        }
                    }
                }
                if let Some(w) = weak.upgrade() {
                    w.set_question(SharedString::new());
                }
                refresh();
                return;
            }
            if yes {
                close();
            } else if let Some(w) = weak.upgrade() {
                w.set_question(SharedString::new());
            }
        }
    });
    w.on_action({ let weak = w.as_weak(); let state = state.clone(); let slots = slots.clone(); let store = store.clone(); let refresh = refresh.clone(); let blocked = blocked.clone(); let active = active.clone(); let close = close.clone(); let fetch = fetch.clone(); move |action| {
        if blocked() && !(action == "cancel" && fetch.busy() && active.get()) { return; } let Some(w) = weak.upgrade() else { return; };
        if !w.get_question().is_empty() { return; }
        let result = (|| -> Result<(),String> {
            match action.as_str() {
                "apply" => { if !state.borrow().errors.is_empty() { return Err(state.borrow().errors.values().cloned().collect::<Vec<_>>().join("\n")); } let value = match &state.borrow().value { Value::Content(e) => Value::Content(Box::new(ContentEditor::new(&e.value(),e.test.clone()))), v @ Value::Page(_) => v.clone() }; applied(value)?; close(); }
                "cancel" => { fetch.stop();let changed = { let e = state.borrow(); match (&e.value,&e.original) { (Value::Page(p),Value::Page(o)) => p != o, (Value::Content(e),Value::Content(o)) => e.value()!=o.value(), _ => false } }; if changed { w.set_question(if page { "It looks like you have made changes to the parser--are you sure you want to cancel?" } else { model::CONTENT_CANCEL }.into()); } else { close(); } }
                "import" | "export" => {
                    use hydrus_gui_model::downloader_interchange::{Definition,Native};
                    let native=match &state.borrow().value{Value::Page(p)=>Native::Page((**p).clone()),Value::Content(c)=>Native::Content(c.value())};
                    let preview=Rc::new(move |definitions:Vec<Definition>| { if definitions.len()!=1 || !matches!((page,&definitions[0].native),(true,Native::Page(_))|(false,Native::Content(_))) { return Err("Import one matching page or content parser into this editor.".into()); } Ok(format!("Replace this draft with parser: {}",definitions[0].name())) });
                    let applied=Rc::new({let state=state.clone();let refresh=refresh.clone();move |mut definitions:Vec<Definition>| {let definition=definitions.pop().ok_or("No parser to import.")?;let mut e=state.borrow_mut();match (page,definition.native){(true,Native::Page(p))=>e.value=Value::Page(Box::new(p)),(false,Native::Content(c))=>{let test=if let Value::Content(old)=&e.value{old.test.clone()}else{FormulaTestData::default()};e.value=Value::Content(Box::new(ContentEditor::new(&c,test)));},_=>return Err("Import one matching parser.".into())}e.errors.clear();e.selected=None;e.subsidiary_selected=None;e.subsidiary_selection=ListSelection::default();drop(e);refresh();Ok(())}});
                    let child=crate::downloader_interchange_window::open(&slots.exchange,action=="import",vec![Definition::new(native)],preview,applied)?; let refresh=refresh.clone(); child.on_closed(move||refresh());
                }
                "test" => { if state.borrow().raw_mimes.get(state.borrow().example,w.get_document().as_str()).is_some() {return Ok(());} let mut test = test_data(&w,&state.borrow(),true)?; let mut e = state.borrow_mut();let subsidiary=e.subsidiary.clone();let parsed = match &mut e.value { Value::Page(p) => {if let Some(details)=subsidiary {test.context.insert("post_index".into(),"0".into());details.borrow().preview(p,&mut test.context,&test.text)}else{p.parse(&mut test.context,&test.text)}}, Value::Content(c) => { c.test = test; c.preview().map(|p| vec![p]) } }; w.set_preview(match parsed { Ok(posts) => model::preview_text(&posts), Err(error) => error.to_string() }.into()); }
                "delete-content" => { let mut e = state.borrow_mut(); if let Some(index) = e.selected.take() && let Value::Page(p) = &mut e.value && index < p.content_parsers.len() { p.content_parsers.remove(index); } }
                "add-content"|"edit-content" => {
                    let test = child_test_data(&w,&state.borrow(),true)?;
                    let (at, original) = { let e = state.borrow(); let Value::Page(p) = &e.value else { return Ok(()); }; let at = if action == "edit-content" { e.selected } else { None }; if action == "edit-content" && at.is_none() { return Ok(()); } (at,at.and_then(|i| p.content_parsers.get(i).cloned())) };
                    let parser = original.clone().unwrap_or_else(model::new_content);
                    let test = FormulaTestData {collapse_newlines: !matches!(parser.kind,ContentKind::Note { .. }), ..test };
                    let done: Done = Rc::new({ let state = state.clone(); let active = active.clone(); move |v| { if !active.get() { return Ok(()); } let Value::Content(c) = v else { return Ok(()); }; let mut e = state.borrow_mut(); let Value::Page(p) = &mut e.value else { return Ok(()); }; if let Some(index) = at { if p.content_parsers.get(index) != original.as_ref() { return Err("The content parser changed while its editor was open.".into()); } p.content_parsers[index] = c.value(); } else { p.content_parsers.push(c.value()); } Ok(()) } });
                    let child = open_editor(&store,Value::Content(Box::new(ContentEditor::new(&parser,test.clone()))),test,&slots,done,None,None).map_err(|e| e.to_string())?;
                    let refresh = refresh.clone(); child.on_closed(move || refresh()); *slots.content.borrow_mut() = Some(child);
                }
                "add-example" | "remove-example" => {
                    let mut e = state.borrow_mut();
                    e.test.context.insert("url".into(),w.get_test_url().to_string());
                    let old = e.example;e.test.remember_example(old,w.get_document().to_string());
                    e.example = if action == "add-example" { let url = e.test.context.get("url").cloned();e.test.add_example(String::new(),url) } else { if e.test.examples.len()>1 {e.raw_mimes.remove(old);} e.test.remove_example(old) };
                    w.set_document(e.test.text.as_str().into());
                    w.set_test_url(e.test.context.get("url").cloned().unwrap_or_default().into());
                }
                "import-subsidiary" | "export-subsidiary" => {
                    let parsers = {
                        let e = state.borrow(); let Value::Page(page) = &e.value else { return Ok(()); };
                        let order = (0..page.subsidiary.len()).collect::<Vec<_>>();
                        e.subsidiary_selection.in_order(&order).iter().map(|i| page.subsidiary[*i].clone()).collect::<Vec<_>>()
                    };
                    if action == "export-subsidiary" && parsers.is_empty() { return Ok(()); }
                    let preview = Rc::new(|parsers: Vec<hydrus_parse::content::SubsidiaryPageParser>| Ok(format!("Add {} subsidiary parsers:\n{}\nChanges are saved only when you apply the owning parser.", parsers.len(), parsers.iter().map(|p| p.parser.name.as_str()).collect::<Vec<_>>().join("\n"))));
                    let applied = Rc::new({ let state = state.clone(); let active = active.clone(); let refresh = refresh.clone(); move |parsers| {
                        if !active.get() { return Ok(()); }
                        let mut e = state.borrow_mut(); let Value::Page(page) = &mut e.value else { return Ok(()); };
                        let added = model::append_subsidiaries(page, parsers);
                        e.subsidiary_selection.select_many(&added); e.subsidiary_selected = e.subsidiary_selection.one();
                        drop(e); refresh(); Ok(())
                    }});
                    let child = crate::downloader_interchange_window::open_subsidiaries_with_store(&store,&slots.exchange, action == "import-subsidiary", parsers, preview, applied)?;
                    let refresh = refresh.clone(); child.on_closed(move || refresh());
                }
                "duplicate-subsidiary" => {
                    let mut e = state.borrow_mut();
                    let Value::Page(page) = &e.value else { return Ok(()); };
                    let order = (0..page.subsidiary.len()).collect::<Vec<_>>();
                    let copies = e.subsidiary_selection.in_order(&order).into_iter().map(|i| page.subsidiary[i].clone()).collect::<Vec<_>>();
                    let Value::Page(page) = &mut e.value else { return Ok(()); };
                    let added = model::append_subsidiaries(page, copies);
                    e.subsidiary_selection.select_many(&added); e.subsidiary_selected = e.subsidiary_selection.one();
                }
                "delete-subsidiary" => {
                    let mut e = state.borrow_mut(); let Value::Page(page) = &e.value else { return Ok(()); };
                    let order = (0..page.subsidiary.len()).collect::<Vec<_>>();
                    let rows = e.subsidiary_selection.in_order(&order).into_iter().map(|i| (i, page.subsidiary[i].clone())).collect::<Vec<_>>();
                    if !rows.is_empty() { e.pending_subsidiary_delete = Some(rows); w.set_question("Remove all selected?".into()); }
                }
                "add-subsidiary" | "edit-subsidiary" => {
                    let test = child_test_data(&w,&state.borrow(),false)?;
                    let (at,original) = {
                        let e = state.borrow();let Value::Page(p) = &e.value else { return Ok(()); };
                        let at = if action == "edit-subsidiary" { e.subsidiary_selected } else { None };
                        if action == "edit-subsidiary" && at.is_none() { return Ok(()); }
                        (at,at.and_then(|i|p.subsidiary.get(i).cloned()))
                    };
                    let parser = original.clone().unwrap_or_else(model::new_subsidiary);
                    let details = Rc::new(RefCell::new(model::SubsidiaryEditor::new(&parser)));
                    let done: Done = Rc::new({let state=state.clone();let active=active.clone();let details=details.clone();move |value| {
                        if !active.get() { return Ok(()); }
                        let Value::Page(child) = value else { return Ok(()); };
                        let child = details.borrow().value(*child);
                        let mut e=state.borrow_mut();let Value::Page(page)=&mut e.value else { return Ok(()); };
                        let key=child.parser.key.clone();
                        if let Some(index)=at {
                            if page.subsidiary.get(index)!=original.as_ref() { return Err("The subsidiary parser changed while its editor was open.".into()); }
                            page.subsidiary[index]=child;
                        } else { page.subsidiary.push(child); }
                        page.subsidiary.sort_by(|a,b|a.parser.name.cmp(&b.parser.name));
                        e.subsidiary_selected=page.subsidiary.iter().position(|child|child.parser.key==key);
                        let selected=e.subsidiary_selected;e.subsidiary_selection.select_only(selected);
                        Ok(())
                    }});
                    let owned = Slots::default();
                    let child = open_editor(&store,Value::Page(Box::new(parser.parser)),test,&owned,done,None,Some(details)).map_err(|e|e.to_string())?;
                    *owned.page.borrow_mut()=Some(child.clone_strong());
                    *slots.child.borrow_mut()=Some(Box::new(owned));
                    let slots=slots.clone();let refresh=refresh.clone();
                    child.on_closed(move || { slots.child.borrow_mut().take();refresh(); });
                }
                "own-separator" => {
                    let test=test_data(&w,&state.borrow(),false)?;
                    let Some(details)=state.borrow().subsidiary.clone() else { return Ok(()); };
                    let formula=details.borrow().formula.clone();
                    let done=Rc::new({let active=active.clone();let refresh=refresh.clone();move |formula| {
                        if active.get() { details.borrow_mut().formula=formula;refresh(); }
                    }});
                    let child=crate::formula_window::open(&store,&formula,test,&slots.formula,done).map_err(|e|e.to_string())?;
                    let refresh=refresh.clone();child.on_closed(move |_|refresh());
                    *slots.formula.formula.borrow_mut()=Some(child);
                }
                "separator" => {
                    let test = child_test_data(&w,&state.borrow(),false)?;
                    let (index,original) = {
                        let e = state.borrow(); let Value::Page(page) = &e.value else { return Ok(()); };
                        let Some(index) = e.subsidiary_selected else { return Ok(()); };
                        let Some(child) = page.subsidiary.get(index) else { return Ok(()); };
                        (index,child.clone())
                    };
                    let formula = original.formula.clone();
                    let done = Rc::new({ let state = state.clone();let active = active.clone();let refresh = refresh.clone();move |formula| {
                        if !active.get() { return; }
                        let mut e = state.borrow_mut();
                        if let Value::Page(page) = &mut e.value && let Some(child) = page.subsidiary.get_mut(index) && *child == original {
                            child.formula = formula;
                        }
                        drop(e);refresh();
                    }});
                    let child = crate::formula_window::open(&store,&formula,test,&slots.formula,done).map_err(|e|e.to_string())?;
                    let refresh = refresh.clone();child.on_closed(move |_|refresh());
                    *slots.formula.formula.borrow_mut() = Some(child);
                }
                "formula" => { let test = test_data(&w,&state.borrow(),true)?; let Value::Content(e) = &state.borrow().value else { return Ok(()); }; let formula = e.parser.formula.clone(); let test = FormulaTestData { collapse_newlines: !matches!(e.parser.kind,ContentKind::Note { .. }), ..test }; let done = Rc::new({ let state = state.clone(); let active = active.clone(); move |formula| { if active.get() && let Value::Content(e) = &mut state.borrow_mut().value { e.parser.formula = formula; } } }); let child = crate::formula_window::open(&store,&formula,test,&slots.formula,done).map_err(|e| e.to_string())?; let refresh = refresh.clone(); child.on_closed(move |_| refresh()); *slots.formula.formula.borrow_mut() = Some(child); }
                "converter" => { let Value::Page(p) = &state.borrow().value else { return Ok(()); }; let converter = p.converter.clone(); let done = Rc::new({ let state = state.clone(); let active = active.clone(); move |converter| { if active.get() && let Value::Page(p) = &mut state.borrow_mut().value { p.converter = converter; } } }); let child = crate::string_processor_window::open_converter(&converter,Some(w.get_document().to_string()),&slots.formula.strings,done).map_err(|e| e.to_string())?; let refresh = refresh.clone(); child.on_closed(move || refresh()); *slots.formula.strings.converter.borrow_mut() = Some(child); }
                "match" => { let matcher = match &state.borrow().value { Value::Content(e) => match &e.parser.kind { ContentKind::Veto { string_match,.. } => string_match.clone(), _ => return Ok(()) }, Value::Page(_) => return Ok(()) }; let done = Rc::new({ let state = state.clone(); let active = active.clone(); let refresh = refresh.clone(); move |matcher| { if active.get() && let Value::Content(e) = &mut state.borrow_mut().value && let ContentKind::Veto { string_match,.. } = &mut e.parser.kind { *string_match = matcher; } refresh(); } }); crate::string_processor_window::open_match(&store,&matcher,&slots.formula.strings,done); if let Some(child)=slots.formula.strings.step.borrow().as_ref() { let refresh=refresh.clone(); child.on_closed(move ||refresh()); } }
                _ => (),
            } Ok(())
        })();
        if let Err(error) = result { w.set_error(error.into()); }
        refresh();
    } });
    w.window().on_close_requested({
        let weak = w.as_weak();
        let exchange = slots.exchange.clone();
        move || {
            exchange.cancel();
            if let Some(w) = weak.upgrade() {
                w.invoke_action("cancel".into());
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    refresh();
    w.show()?;
    Ok(w)
}
struct ListState {
    draft: Draft,
    selection: ListSelection<usize>,
    order: Vec<usize>,
    links: bool,
    sort_column: usize,
    ascending: bool,
    pending_delete: Option<Vec<String>>,
    pending_clear: Option<Vec<String>>,
}
fn list_row(s: &ListState, i: usize) -> Vec<String> {
    if s.links {
        let c = &s.draft.classes.url_classes[i];
        let key = hex::encode(&c.key);
        let parser = s
            .draft
            .classes
            .parser_links
            .iter()
            .find(|(k, _)| k == &key)
            .and_then(|(_, key)| key.as_ref())
            .and_then(|key| s.draft.parsers.iter().find(|p| &p.key == key));
        vec![
            c.name.clone(),
            c.url_type.name().unwrap_or("source url").into(),
            parser.map_or(String::new(), |p| p.name.clone()),
        ]
    } else {
        let p = &s.draft.parsers[i];
        vec![
            p.name.clone(),
            p.example_urls.join(", "),
            p.content_parsers
                .iter()
                .map(|c| model::CONTENT_TYPES[model::kind_index(&c.kind)])
                .collect::<Vec<_>>()
                .join(", "),
        ]
    }
}
fn show_list(w: &ParserListWindow, s: &mut ListState, slots: &Slots) {
    s.order = if s.links {
        s.draft.linkable()
    } else {
        (0..s.draft.parsers.len()).collect()
    };
    let mut rows = s
        .order
        .iter()
        .map(|&i| (i, list_row(s, i)))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        let order = a.1[s.sort_column].cmp(&b.1[s.sort_column]);
        if s.ascending { order } else { order.reverse() }
    });
    s.order = rows.iter().map(|(i, _)| *i).collect();
    w.set_rows(table(
        rows.into_iter()
            .map(|(i, row)| (row, s.selection.is_selected(i))),
    ));
    let selected = s.selection.in_order(&s.order);
    w.set_selected(!selected.is_empty());
    w.set_single_selected(selected.len() == 1);
    w.set_clearable(
        s.links
            && selected.iter().any(|&i| {
                let key = hex::encode(&s.draft.classes.url_classes[i].key);
                s.draft
                    .classes
                    .parser_links
                    .iter()
                    .any(|(k, p)| *k == key && p.is_some())
            }),
    );
    w.set_child_open(
        slots.page.borrow().is_some()
            || slots.picker.borrow().is_some()
            || slots.exchange.has_open(),
    );
    w.set_parser_choices(strings(s.draft.parsers.iter().map(|p| p.name.clone())));
    w.set_gaps_exist(s.links && s.draft.gaps_exist());
}
/// Open either named page parsers or direct URL-class links on native settings.
pub fn open(store: &Arc<Store>, slots: &Slots, links: bool) -> Result<ParserListWindow, String> {
    let slot = if links { &slots.links } else { &slots.list };
    if let Some(w) = slot.borrow().as_ref() {
        w.show().map_err(|e| e.to_string())?;
        return Ok(w.clone_strong());
    }
    let draft = Draft::load(store).map_err(|e| e.to_string())?;
    let w = ParserListWindow::new().map_err(|e| e.to_string())?;
    w.set_links(links);
    w.set_columns(ModelRc::new(VecModel::from(
        (if links {
            model::LINK_COLUMNS
        } else {
            model::PARSER_COLUMNS
        })
        .iter()
        .map(|title| TableColumn {
            title: (*title).into(),
            width: 280.,
            stretch: true,
        })
        .collect::<Vec<_>>(),
    )));
    let active = Rc::new(Cell::new(true));
    let state = Rc::new(RefCell::new(ListState {
        draft,
        selection: ListSelection::default(),
        order: Vec::new(),
        links,
        sort_column: 0,
        ascending: true,
        pending_delete: None,
        pending_clear: None,
    }));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        let slots = slots.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                show_list(&w, &mut state.borrow_mut(), &slots);
            }
        }
    });
    let blocked: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let slots = slots.clone();
        let state = state.clone();
        move || {
            !active.get()
                || slots.page.borrow().is_some()
                || slots.exchange.has_open()
                || slots.picker.borrow().is_some()
                || state.borrow().pending_clear.is_some()
                || state.borrow().pending_delete.is_some()
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let active = active.clone();
        let weak = w.as_weak();
        let slot = slot.clone();
        let exchange = slots.exchange.clone();
        let picker = slots.picker.clone();
        move || {
            active.set(false);
            let child = picker
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(child) = child {
                child.invoke_answered(false);
            }
            exchange.cancel();
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
        }
    });
    w.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |r, c, h| {
            if blocked() {
                return;
            }
            let mut s = state.borrow_mut();
            if let Ok(r) = usize::try_from(r)
                && r < s.order.len()
            {
                let order = s.order.clone();
                s.selection.click(&order, r, c, h);
            }
            drop(s);
            refresh();
        }
    });
    w.on_sort({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |c, a| {
            if blocked() {
                return;
            }
            if let Ok(c) = usize::try_from(c)
                && c < 3
            {
                let mut s = state.borrow_mut();
                s.sort_column = c;
                s.ascending = a;
            }
            refresh();
        }
    });
    w.on_answered({
        let state = state.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let slots = slots.clone();
        let weak = w.as_weak();
        move |yes| {
            if !active.get() || slots.page.borrow().is_some() {
                return;
            }
            let mut s = state.borrow_mut();
            if let Some(keys) = s.pending_clear.take() {
                if yes {
                    for key in keys {
                        let _ = s.draft.link(&key, None);
                    }
                }
            } else if let Some(keys) = s.pending_delete.take() {
                if yes {
                    s.draft.remove(&keys);
                    s.selection = ListSelection::default();
                }
            } else {
                return;
            }
            drop(s);
            if let Some(w) = weak.upgrade() {
                w.set_question(SharedString::new());
            }
            refresh();
        }
    });
    let mut api_pairs = {
        let state = state.borrow();
        let draft = &state.draft;
        draft
            .api_pairs()
            .into_iter()
            .map(|(source, target)| {
                vec![
                    draft.classes.url_classes[source].name.clone(),
                    draft.classes.url_classes[target].name.clone(),
                ]
            })
            .collect::<Vec<_>>()
    };
    api_pairs.sort();
    w.set_api_columns(ModelRc::new(VecModel::from(
        ["url class", "api/redirect url class"]
            .into_iter()
            .map(|title| TableColumn {
                title: title.into(),
                width: 400.,
                stretch: true,
            })
            .collect::<Vec<_>>(),
    )));
    w.set_api_rows(table(api_pairs.iter().cloned().map(|row| (row, false))));
    w.on_api_sort({
        let weak = w.as_weak();
        let active = active.clone();
        let rows = Rc::new(RefCell::new(api_pairs));
        move |column, ascending| {
            if !active.get() {
                return;
            }
            let Ok(column) = usize::try_from(column) else {
                return;
            };
            if column > 1 {
                return;
            }
            let mut rows = rows.borrow_mut();
            rows.sort_by(|a, b| {
                let order = a[column].cmp(&b[column]);
                if ascending { order } else { order.reverse() }
            });
            if let Some(w) = weak.upgrade() {
                w.set_api_rows(table(rows.iter().cloned().map(|row| (row, false))));
            }
        }
    });
    w.on_action({
        let weak = w.as_weak();
        let state = state.clone();
        let store = store.clone();
        let slots = slots.clone();
        let active = active.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        let close = close.clone();
        move |action| {
            if blocked() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let result = (|| -> Result<(), String> {
                match action.as_str() {
                    "apply" => {
                        state
                            .borrow()
                            .draft
                            .save(&store)
                            .map_err(|e| e.to_string())?;
                        close();
                    }
                    "cancel" => close(),
                    "delete" => {
                        let mut s = state.borrow_mut();
                        let keys = s
                            .selection
                            .in_order(&s.order)
                            .iter()
                            .map(|&i| s.draft.parsers[i].key.clone())
                            .collect::<Vec<_>>();
                        if !keys.is_empty() {
                            s.pending_delete = Some(keys);
                            w.set_question("Remove all selected?".into());
                        }
                    }
                    "auto-link" => {
                        let mut s = state.borrow_mut();
                        if s.links && w.get_links_tab() == 0 && s.draft.gaps_exist() {
                            s.draft.try_fill_gaps();
                        }
                    }
                    "pick-parser" => {
                        let s = state.borrow();
                        if !s.links || w.get_links_tab() != 0 { return Ok(()); }
                        if s.draft.parsers.is_empty() {
                            return Err("Unfortunately, you do not have any parsers, so none can be linked to your url classes. Please create some!".into());
                        }
                        let selected = s.selection.in_order(&s.order);
                        if selected.len() != 1 { return Ok(()); }
                        let class = &s.draft.classes.url_classes[selected[0]];
                        let key = hex::encode(&class.key);
                        let choices = s.draft.parser_choices(&key)?;
                        let current = s.draft.classes.parser_links.iter().find(|(k, _)| k == &key).and_then(|(_, p)| p.as_ref());
                        let selected = current.and_then(|key| choices.iter().position(|(_, p)| p.as_ref() == Some(key))).unwrap_or(0);
                        let title = format!("select parser for {}", class.name);
                        drop(s);
                        let child = ParserPickerWindow::new().map_err(|e| e.to_string())?;
                        child.set_window_title(title.into());
                        child.set_selected_index(i32::try_from(selected).unwrap_or(0));
                        let choices = Rc::new(choices);
                        let update: Rc<dyn Fn()> = Rc::new({
                            let weak = child.as_weak();
                            let choices = choices.clone();
                            move || { if let Some(child) = weak.upgrade() {
                                child.set_rows(table(choices.iter().enumerate().map(|(i, (label, _))| (vec![label.clone()], i32::try_from(i).ok() == Some(child.get_selected_index())))));
                            } }
                        });
                        let picker_active = Rc::new(Cell::new(true));
                        child.on_selected({
                            let weak = child.as_weak(); let update = update.clone();
                            let picker_active = picker_active.clone(); let choices = choices.clone();
                            move |i| { if picker_active.get() && usize::try_from(i).is_ok_and(|i| i < choices.len()) && let Some(child) = weak.upgrade() {
                                child.set_selected_index(i); update();
                            } }
                        });
                        child.on_answered({
                            let state = state.clone(); let active = active.clone();
                            let weak = child.as_weak(); let parent = w.as_weak();
                            let slot = slots.picker.clone(); let refresh = refresh.clone();
                            move |yes| {
                                if !picker_active.replace(false) { return; }
                                if let Some(child) = weak.upgrade() {
                                    if yes && active.get() && let Some((_, Some(parser))) = usize::try_from(child.get_selected_index()).ok().and_then(|i| choices.get(i))
                                        && let Err(error) = state.borrow_mut().draft.link(&key, Some(parser)) && let Some(parent) = parent.upgrade() { parent.set_error(error.into()); }
                                    let _ = child.hide();
                                }
                                slot.borrow_mut().take(); refresh();
                            }
                        });
                        child.window().on_close_requested({ let weak = child.as_weak(); move || {
                            if let Some(child) = weak.upgrade() { child.invoke_answered(false); }
                            slint::CloseRequestResponse::KeepWindowShown
                        } });
                        update(); child.show().map_err(|e| e.to_string())?;
                        *slots.picker.borrow_mut() = Some(child);
                    }
                    "link" | "clear" => {
                        let mut s = state.borrow_mut();
                        if !s.links || w.get_links_tab() != 0 {
                            return Ok(());
                        }
                        let classes = s
                            .selection
                            .in_order(&s.order)
                            .iter()
                            .map(|&i| hex::encode(&s.draft.classes.url_classes[i].key))
                            .collect::<Vec<_>>();
                        if action == "clear" {
                            if w.get_clearable() {
                                s.pending_clear = Some(classes);
                                w.set_question("Clear all the selected linked parsers?".into());
                            }
                            return Ok(());
                        }
                        let parser = {
                            Some(
                                usize::try_from(w.get_chosen_parser())
                                    .ok()
                                    .and_then(|i| s.draft.parsers.get(i))
                                    .ok_or_else(|| {
                                        "No parser selected. Create a parser first.".to_owned()
                                    })?
                                    .key
                                    .clone(),
                            )
                        };
                        for key in classes {
                            s.draft.link(&key, parser.as_deref())?;
                        }
                    }
                    "import" | "export" => {
                        let preview = Rc::new({
                            let state = state.clone();
                            move |definitions| {
                                let mut next = state.borrow().draft.clone();
                                next.import(definitions).map(|r| r.text())
                            }
                        });
                        let applied = Rc::new({
                            let state = state.clone();
                            let refresh = refresh.clone();
                            move |definitions| {
                                state.borrow_mut().draft.import(definitions)?;
                                refresh();
                                Ok(())
                            }
                        });
                        let s = state.borrow();
                        let definitions = s
                            .selection
                            .in_order(&s.order)
                            .iter()
                            .map(|&i| {
                                s.draft.auxiliary.definition(
                                    hydrus_gui_model::downloader_interchange::Native::Page(
                                        s.draft.parsers[i].clone(),
                                    ),
                                )
                            })
                            .collect();
                        let child = crate::downloader_interchange_window::open(
                            &slots.exchange,
                            action == "import",
                            definitions,
                            preview,
                            applied,
                        )?;
                        let refresh = refresh.clone();
                        child.on_closed(move || refresh());
                    }
                    "add" | "edit" | "duplicate" => {
                        let s = state.borrow();
                        let selected = s.selection.in_order(&s.order).first().copied();
                        let page = if action == "add" {
                            model::new_page()
                        } else {
                            let Some(i) = selected else {
                                return Ok(());
                            };
                            s.draft.parsers[i].clone()
                        };
                        let replacing = (action == "edit").then(|| page.key.clone());
                        let test = FormulaTestData {
                            context: TestContext {
                                url: page.example_urls.first().cloned().unwrap_or_default(),
                                ..TestContext::default()
                            }
                            .values(),
                            ..FormulaTestData::default()
                        };
                        drop(s);
                        let done: Done = Rc::new({
                            let state = state.clone();
                            let active = active.clone();
                            move |v| {
                                if !active.get() {
                                    return Ok(());
                                }
                                if let Value::Page(p) = v {
                                    state.borrow_mut().draft.put(replacing.as_deref(), *p)?;
                                }
                                Ok(())
                            }
                        });
                        let child = open_editor(
                            &store,
                            Value::Page(Box::new(page)),
                            test,
                            &slots,
                            done,
                            None,
                            None,
                        )
                        .map_err(|e| e.to_string())?;
                        let refresh = refresh.clone();
                        child.on_closed(move || refresh());
                        *slots.page.borrow_mut() = Some(child);
                    }
                    _ => (),
                }
                Ok(())
            })();
            if let Err(error) = result {
                w.set_error(error.into());
            }
            refresh();
        }
    });
    w.on_force_close({
        let close = close.clone();
        move || close()
    });
    w.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::KeepWindowShown
    });
    refresh();
    w.show().map_err(|e| e.to_string())?;
    *slot.borrow_mut() = Some(w.clone_strong());
    Ok(w)
}
