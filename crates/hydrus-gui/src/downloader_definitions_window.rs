//! Slint downloader definition windows, owning isolated typed drafts until Apply.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::url::{AnyGug, StringMatch, StringProcessor, UrlClass, UrlParameter, UrlType};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::downloader_definitions::{
    self as definitions, DefinitionEditor as Editor, Draft, EditValue as Value, Kind, RuleEdit,
};
use crate::string_editors::{CHARACTER_SETS, MATCH_TYPES, MatchEditor};
use crate::{
    DefinitionField, DownloaderDefinitionEditWindow, DownloaderDefinitionsWindow, TableColumn,
    TableRow,
};

/// Definition list and child editors while open.
#[derive(Clone, Default)]
pub struct Slots {
    pub class_exchange: crate::downloader_interchange_window::Slots,
    pub gug_exchange: crate::downloader_interchange_window::Slots,
    pub classes: Rc<RefCell<Option<DownloaderDefinitionsWindow>>>,
    pub gugs: Rc<RefCell<Option<DownloaderDefinitionsWindow>>>,
    pub class_edit: Rc<RefCell<Option<DownloaderDefinitionEditWindow>>>,
    pub gug_edit: Rc<RefCell<Option<DownloaderDefinitionEditWindow>>>,
    pub rule: Rc<RefCell<Option<DownloaderDefinitionEditWindow>>>,
    pub strings: crate::string_processor_window::Slots,
}

impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("classes", &self.classes.borrow().is_some())
            .field("gugs", &self.gugs.borrow().is_some())
            .finish_non_exhaustive()
    }
}

fn strings(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        values.into_iter().map(Into::into).collect::<Vec<_>>(),
    ))
}

fn int(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

fn row(cells: Vec<String>, selected: bool) -> TableRow {
    TableRow {
        cells: strings(cells),
        selected,
    }
}

fn field(id: i32, label: &str, text: impl Into<String>) -> DefinitionField {
    DefinitionField {
        id,
        label: label.into(),
        text: text.into().into(),
        enabled: true,
        ..DefinitionField::default()
    }
}

fn choice(id: i32, label: &str, options: &[&str], chosen: usize) -> DefinitionField {
    DefinitionField {
        kind: 1,
        options: strings(options.iter().map(|s| (*s).to_owned())),
        chosen: int(chosen),
        ..field(id, label, "")
    }
}

fn tick(id: i32, label: &str, checked: bool) -> DefinitionField {
    DefinitionField {
        kind: 2,
        checked,
        ..field(id, label, "")
    }
}

fn show_list(window: &DownloaderDefinitionsWindow, draft: &Draft) {
    let headings = match draft.kind {
        Kind::Classes => definitions::CLASS_COLUMNS,
        Kind::Generators => definitions::GUG_COLUMNS,
        Kind::Nested => definitions::NESTED_COLUMNS,
    };
    window.set_columns(ModelRc::new(VecModel::from(
        headings
            .iter()
            .enumerate()
            .map(|(i, title)| TableColumn {
                title: (*title).into(),
                width: if i == 0 { 220.0 } else { 260.0 },
                stretch: i == 1,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_rows(ModelRc::new(VecModel::from(
        draft
            .order()
            .into_iter()
            .map(|i| row(draft.row(i), draft.selection.is_selected(i)))
            .collect::<Vec<_>>(),
    )));
    window.set_can_edit(draft.selection.one().is_some());
    window.set_can_delete(!draft.selection.is_empty());
    window.set_tab(i32::from(draft.kind == Kind::Nested));
    window.set_sort_column(int(draft.sort_column));
    window.set_ascending(draft.ascending);
}

#[derive(Clone)]
enum Pending {
    Cancel,
    Delete(Kind, Vec<usize>),
}

/// Open URL classes (`classes=true`) or single/nested GUGs from native settings.
pub fn open(
    store: &Arc<Store>,
    slots: &Slots,
    classes: bool,
) -> Result<DownloaderDefinitionsWindow, String> {
    slots.strings.set_store(store);
    let slot = if classes { &slots.classes } else { &slots.gugs };
    let exchange = if classes {
        &slots.class_exchange
    } else {
        &slots.gug_exchange
    };
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = DownloaderDefinitionsWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title(
        if classes {
            "edit url classes"
        } else {
            "edit gallery url generators"
        }
        .into(),
    );
    window.set_generators(!classes);
    window.set_test_result(definitions::CHECKER_HINT.into());
    let draft = Rc::new(RefCell::new(
        Draft::load(
            store,
            if classes {
                Kind::Classes
            } else {
                Kind::Generators
            },
        )
        .map_err(|e| e.to_string())?,
    ));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let draft = draft.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                show_list(&w, &draft.borrow());
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = slot.clone();
        let slots = slots.clone();
        let exchange = exchange.clone();
        move || {
            exchange.cancel();
            if classes {
                slots.strings.cancel_all();
                if let Some(rule) = slots.rule.borrow_mut().take() {
                    let _ = rule.hide();
                }
            }
            let edit_slot = if classes {
                &slots.class_edit
            } else {
                &slots.gug_edit
            };
            if let Some(editor) = edit_slot.borrow_mut().take() {
                let _ = editor.hide();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
        }
    });
    let pending_question: Rc<RefCell<Option<Pending>>> = Rc::default();
    window.on_row_clicked({
        let exchange = exchange.clone();
        let draft = draft.clone();
        let refresh = refresh.clone();
        move |i, c, s| {
            if exchange.has_open() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let mut d = draft.borrow_mut();
                let order = d.order();
                d.selection.click(&order, i, c, s);
            }
            refresh();
        }
    });
    window.on_sort({
        let exchange = exchange.clone();
        let draft = draft.clone();
        let refresh = refresh.clone();
        move |i, a| {
            if exchange.has_open() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let mut d = draft.borrow_mut();
                d.sort_column = i;
                d.ascending = a;
            }
            refresh();
        }
    });
    window.on_tab_chosen({
        let draft = draft.clone();
        let refresh = refresh.clone();
        let slots = slots.clone();
        let exchange = exchange.clone();
        move |tab| {
            if slots.gug_edit.borrow().is_some() || exchange.has_open() {
                return;
            }
            let mut d = draft.borrow_mut();
            d.kind = if tab == 1 {
                Kind::Nested
            } else {
                Kind::Generators
            };
            d.selection.select_only(None);
            drop(d);
            refresh();
        }
    });
    window.on_test_edited({
        let draft = draft.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        move |url| {
            let text = draft.borrow_mut().check_url(&url);
            if let Some(w) = weak.upgrade() {
                w.set_test_result(text.into());
            }
            refresh();
        }
    });
    window.on_answered({
        let draft = draft.clone();
        let refresh = refresh.clone();
        let close = close.clone();
        let pending = pending_question.clone();
        let weak = window.as_weak();
        move |yes| {
            if let Some(w) = weak.upgrade() {
                w.set_question(SharedString::new());
            }
            let question = pending.borrow_mut().take();
            if yes {
                match question {
                    Some(Pending::Cancel) => close(),
                    Some(Pending::Delete(kind, items)) => {
                        let mut draft = draft.borrow_mut();
                        let shown_kind = draft.kind;
                        draft.kind = kind;
                        draft.selection.select_many(&items);
                        draft.delete_selected();
                        draft.kind = shown_kind;
                        drop(draft);
                        refresh();
                    }
                    None => (),
                }
            }
        }
    });
    window.on_action({
        let draft = draft.clone();
        let refresh = refresh.clone();
        let close = close.clone();
        let weak = window.as_weak();
        let pending = pending_question.clone();
        let store = store.clone();
        let slots = slots.clone();
        let exchange = exchange.clone();
        move |action| {
            let Some(w) = weak.upgrade() else { return };
            let edit_slot = if classes {
                &slots.class_edit
            } else {
                &slots.gug_edit
            };
            if edit_slot.borrow().is_some() || pending.borrow().is_some() || exchange.has_open() {
                return;
            }
            match action.as_str() {
                "apply" => match draft.borrow().save(&store) {
                    Ok(()) => close(),
                    Err(e) => w.set_error(e.to_string().into()),
                },
                "cancel" => {
                    if draft.borrow().changed() {
                        *pending.borrow_mut() = Some(Pending::Cancel);
                        w.set_question(definitions::CANCEL.into());
                    } else {
                        close();
                    }
                }
                "delete" => {
                    if !draft.borrow().selection.is_empty() {
                        let draft = draft.borrow();
                        *pending.borrow_mut() = Some(Pending::Delete(
                            draft.kind,
                            draft.selection.in_order(&draft.order()),
                        ));
                        w.set_question(draft.delete_question().into());
                    }
                }
                "import" | "export" => {
                    let preview = Rc::new({
                        let draft = draft.clone();
                        move |definitions| {
                            let mut next = draft.borrow().clone();
                            next.import(definitions).map(|r| r.text())
                        }
                    });
                    let applied = Rc::new({
                        let draft = draft.clone();
                        let refresh = refresh.clone();
                        move |definitions| {
                            draft.borrow_mut().import(definitions)?;
                            refresh();
                            Ok(())
                        }
                    });
                    let definitions = draft.borrow().selected_definitions();
                    match crate::downloader_interchange_window::open(
                        &exchange,
                        action == "import",
                        definitions,
                        preview,
                        applied,
                    ) {
                        Ok(child) => {
                            let refresh = refresh.clone();
                            child.on_closed(move || refresh());
                        }
                        Err(e) => w.set_error(e.into()),
                    }
                }
                "add" | "edit" | "duplicate" => {
                    if let Err(e) =
                        open_definition(&store, &draft, &slots, action.as_str(), refresh.clone())
                    {
                        w.set_error(e.into());
                    }
                }
                _ => (),
            }
        }
    });
    window.window().on_close_requested({
        let weak = window.as_weak();
        let exchange = exchange.clone();
        move || {
            exchange.cancel();
            if let Some(w) = weak.upgrade() {
                w.invoke_action("cancel".into());
            }
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    refresh();
    window.show().map_err(|e| e.to_string())?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}

fn editor_fields(editor: &Editor) -> Vec<DefinitionField> {
    match &editor.value {
        Value::Class(c) => class_fields(editor, c),
        Value::Gug(AnyGug::Single(g)) => vec![
            field(0, "name:", &g.name),
            field(1, "url template:", &g.url_template),
            field(2, "replacement phrase:", &g.replacement_phrase),
            field(3, "search terms separator:", &g.separator),
            field(
                4,
                "initial search text (to prompt user):",
                &g.initial_search_text,
            ),
            field(5, "example search text:", &g.example_search_text),
        ],
        Value::Gug(AnyGug::Nested(g)) => vec![
            field(0, "name:", &g.name),
            field(
                4,
                "initial search text (to prompt user):",
                &g.initial_search_text,
            ),
        ],
        Value::Rule(r, parameter) => rule_fields(r, *parameter),
        Value::Header(name, value) => vec![
            field(0, "header name:", name),
            field(1, "header value:", value),
        ],
    }
}

fn class_fields(editor: &Editor, c: &UrlClass) -> Vec<DefinitionField> {
    let mut fields = if editor.tab == 0 {
        vec![
            field(0, "name:", &c.name),
            choice(
                1,
                "url type:",
                &["post url", "gallery url", "watchable url", "file url"],
                class_type_index(c.url_type),
            ),
            choice(
                2,
                "preferred scheme:",
                &["http", "https"],
                usize::from(c.preferred_scheme == "https"),
            ),
            tick(
                7,
                "do not allow extra path components",
                c.no_more_path_components_than_this,
            ),
            tick(
                8,
                "do not allow extra parameters",
                c.no_more_parameters_than_this,
            ),
            tick(
                9,
                "has single-value parameters",
                c.has_single_value_parameters,
            ),
        ]
    } else {
        vec![
            tick(
                10,
                "alphabetise GET parameters",
                c.alphabetise_get_parameters,
            ),
            tick(
                11,
                "keep extra parameters for server",
                c.keep_extra_parameters_for_server,
            ),
            DefinitionField {
                enabled: c.url_type == UrlType::Post,
                ..tick(
                    12,
                    "can produce multiple files",
                    c.can_produce_multiple_files,
                )
            },
            tick(
                13,
                "should be associated with files",
                c.should_be_associated_with_files,
            ),
            tick(14, "keep fragment", c.keep_fragment),
            choice(
                15,
                "send referral URL:",
                &[
                    "only if provided",
                    "never",
                    "converter if none provided",
                    "only converter",
                ],
                c.referral.mode as usize,
            ),
            choice(
                20,
                "next gallery page index:",
                &["none", "path component (zero based)", "parameter"],
                editor.gallery_position,
            ),
            field(21, "index identifier:", &editor.gallery_identifier),
            field(22, "index delta (1–65536):", &editor.gallery_delta),
        ]
    };
    fields.push(field(40, "example url:", &c.example_url));
    fields
}

fn editor_blocked(editor: &Editor, slots: &Slots) -> bool {
    match editor.value {
        Value::Class(_) => slots.rule.borrow().is_some() || slots.strings.has_open(),
        Value::Rule(_, _) => slots.strings.has_open(),
        Value::Gug(_) | Value::Header(_, _) => false,
    }
}

fn class_type_index(kind: UrlType) -> usize {
    match kind {
        UrlType::Gallery => 1,
        UrlType::Watchable => 2,
        UrlType::File => 3,
        _ => 0,
    }
}

fn rule_fields(r: &RuleEdit, parameter: bool) -> Vec<DefinitionField> {
    let mut fields = Vec::new();
    if parameter {
        fields.push(field(0, "parameter name, %-encoded:", &r.name));
        fields.push(tick(3, "ephemeral (only sent to server)", r.ephemeral));
    }
    fields.push(choice(
        100,
        "match type:",
        &MATCH_TYPES,
        r.matcher.match_type,
    ));
    if matches!(r.matcher.match_type, 1 | 3) {
        fields.push(field(
            101,
            if r.matcher.match_type == 1 {
                "fixed text:"
            } else {
                "regex:"
            },
            if r.matcher.match_type == 1 {
                &r.matcher.fixed
            } else {
                &r.matcher.regex
            },
        ));
    }
    if r.matcher.match_type == 2 {
        let labels = CHARACTER_SETS
            .iter()
            .map(|(_, name)| *name)
            .collect::<Vec<_>>();
        fields.push(choice(
            102,
            "character set:",
            &labels,
            CHARACTER_SETS
                .iter()
                .position(|(f, _)| *f == r.matcher.flexible)
                .unwrap_or(0),
        ));
    }
    if r.matcher.match_type != 1 {
        fields.push(field(
            103,
            "minimum characters (blank: no limit):",
            r.matcher
                .min_chars
                .map_or_else(String::new, |n| n.to_string()),
        ));
        fields.push(field(
            104,
            "maximum characters (blank: no limit):",
            r.matcher
                .max_chars
                .map_or_else(String::new, |n| n.to_string()),
        ));
        fields.push(field(105, "example text:", &r.matcher.example));
    }
    fields.push(tick(2, "has default value", r.default_enabled));
    fields.push(field(1, "default value, %-encoded:", &r.default));
    fields
}

fn show_editor(window: &DownloaderDefinitionEditWindow, editor: &Editor, whole: bool) {
    window.set_error(SharedString::new());
    if whole {
        window.set_fields(ModelRc::new(VecModel::from(editor_fields(editor))));
    }
    window.set_tab(int(editor.tab));
    window.set_rule_tab(int(editor.rule_tab));
    window.set_selected_rule(editor.selected_rule.map_or(-1, int));
    window.set_rules(strings(editor.rules()));
    window.set_preview(editor.preview().into());
    let is_class = matches!(&editor.value, Value::Class(_));
    window.set_url_class(is_class);
    if let Value::Class(c) = &editor.value {
        window.set_domain_mode(int(editor.domain_mode));
        window.set_domain_mode_enabled(
            c.domain_mask.raw_domains.len() == 1 && c.domain_mask.domain_regexes.is_empty(),
        );
        // Keep unfinished trailing newlines/spaces while the user types a full list.
        if whole {
            window.set_domain_raw(c.domain_mask.raw_domains.join("\n").into());
            window.set_domain_regex(c.domain_mask.domain_regexes.join("\n").into());
        }
        window.set_domain_match(c.domain_mask.match_subdomains);
        window.set_domain_keep(c.domain_mask.keep_matched_subdomains);
        window.set_domain_test(editor.domain_test.clone().into());
        let (status, normalised) = editor.domain_preview();
        window.set_domain_status(status.into());
        window.set_domain_normalised(normalised.into());
        let preview = definitions::class_preview(c, editor.classes.collapse_leading_slashes);
        let invalid = preview.status.starts_with("Example does not match");
        window.set_preview_status(preview.status.into());
        window.set_preview_normalised(preview.normalised.into());
        window.set_preview_request(preview.request.into());
        window.set_preview_api(preview.api.into());
        // Qt clears these three outputs on a mismatch, retaining its last referral/next.
        if !invalid {
            window.set_preview_referral(preview.referral.into());
            window.set_preview_next(preview.next.into());
        }
    }
    if let Value::Gug(AnyGug::Nested(n)) = &editor.value {
        window.set_members(ModelRc::new(VecModel::from(
            editor
                .downloaders
                .gugs
                .gugs
                .iter()
                .filter(|g| matches!(g, AnyGug::Single(_)))
                .map(|g| {
                    row(
                        vec![g.name().to_owned()],
                        n.gugs
                            .iter()
                            .any(|(k, name)| k == g.key() || name == g.name()),
                    )
                })
                .collect::<Vec<_>>(),
        )));
    }
}

type Done = Rc<dyn Fn(Value) -> Result<(), String>>;

fn open_definition(
    store: &Arc<Store>,
    draft: &Rc<RefCell<Draft>>,
    slots: &Slots,
    action: &str,
    refresh: Rc<dyn Fn()>,
) -> Result<(), String> {
    let d = draft.borrow();
    let slot = if d.kind == Kind::Classes {
        &slots.class_edit
    } else {
        &slots.gug_edit
    };
    if slot.borrow().is_some() {
        return Ok(());
    }
    let selected = if action == "add" {
        None
    } else {
        d.selection.one()
    };
    if action != "add" && selected.is_none() {
        return Ok(());
    }
    let value = match d.kind {
        Kind::Classes => {
            Value::Class(Box::new(selected.map_or_else(UrlClass::default, |i| {
                d.classes.url_classes[i].clone()
            })))
        }
        Kind::Generators => Value::Gug(selected.map_or_else(
            || AnyGug::Single(definitions::new_gug()),
            |i| d.downloaders.gugs.gugs[i].clone(),
        )),
        Kind::Nested => Value::Gug(selected.map_or_else(
            || AnyGug::Nested(definitions::new_nested()),
            |i| d.downloaders.gugs.gugs[i].clone(),
        )),
    };
    let editor = Editor::new(value, &d);
    drop(d);
    let replacing = if action == "duplicate" {
        None
    } else {
        selected
    };
    let done: Done = Rc::new({
        let draft = draft.clone();
        move |value| {
            match value {
                Value::Class(c) => draft.borrow_mut().put_class(*c, replacing)?,
                Value::Gug(g) => draft.borrow_mut().put_gug(g, replacing)?,
                _ => (),
            }
            refresh();
            Ok(())
        }
    });
    let window = open_editor(store, editor, slots, slot.clone(), done)?;
    *slot.borrow_mut() = Some(window);
    Ok(())
}

fn open_editor(
    store: &Arc<Store>,
    editor: Editor,
    slots: &Slots,
    slot: Rc<RefCell<Option<DownloaderDefinitionEditWindow>>>,
    done: Done,
) -> Result<DownloaderDefinitionEditWindow, String> {
    let window = DownloaderDefinitionEditWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title(
        match &editor.value {
            Value::Class(_) => "edit url class",
            Value::Gug(AnyGug::Single(_)) => "edit gallery url generator",
            Value::Gug(AnyGug::Nested(_)) => "edit nested gallery url generator",
            Value::Rule(_, true) => "edit parameter",
            Value::Rule(_, false) => "edit path component",
            Value::Header(..) => "edit header override",
        }
        .into(),
    );
    if matches!(editor.value, Value::Class(_)) {
        window.set_tabs(strings(["matching".into(), "options".into()]));
        window.set_rule_tabs(strings([
            "path components".into(),
            "parameters".into(),
            "header overrides".into(),
            "single-value parameter test".into(),
        ]));
    }
    window.set_rule_processor(matches!(editor.value, Value::Rule(_, true)));
    window.set_nested(matches!(editor.value, Value::Gug(AnyGug::Nested(_))));
    let state = Rc::new(RefCell::new(editor));
    let refresh: Rc<dyn Fn(bool)> = Rc::new({
        let state = state.clone();
        let weak = window.as_weak();
        let slots = slots.clone();
        move |whole| {
            if let Some(w) = weak.upgrade() {
                let editor = state.borrow();
                w.set_child_open(editor_blocked(&editor, &slots));
                show_editor(&w, &editor, whole);
            }
        }
    });
    let blocked: Rc<dyn Fn() -> bool> = Rc::new({
        let state = state.clone();
        let slots = slots.clone();
        move || editor_blocked(&state.borrow(), &slots)
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        let slots = slots.clone();
        move || {
            match state.borrow().value {
                Value::Class(_) => {
                    slots.strings.cancel_all();
                    if let Some(rule) = slots.rule.borrow_mut().take() {
                        let _ = rule.hide();
                    }
                }
                Value::Rule(_, _) => slots.strings.cancel_all(),
                Value::Gug(_) | Value::Header(_, _) => (),
            }
            let window = weak.upgrade();
            if let Some(window) = &window {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            if let Some(window) = window {
                window.invoke_closed();
            }
        }
    });
    window.on_text_edited({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |i, t| {
            if blocked() {
                return;
            }
            state.borrow_mut().text(i, t.into());
            refresh(false);
        }
    });
    window.on_choice_edited({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |i, c| {
            if blocked() {
                return;
            }
            if let Ok(c) = usize::try_from(c) {
                state.borrow_mut().choose(i, c);
                refresh(true);
            }
        }
    });
    window.on_toggled({
        let state = state.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        let blocked = blocked.clone();
        move |i, v| {
            if blocked() {
                return;
            }
            state.borrow_mut().toggle(i, v);
            if i == 13
                && let Value::Class(class) = &state.borrow().value
                && let Some(window) = weak.upgrade()
            {
                window.set_note(
                    definitions::association_notice(class.url_type, v)
                        .unwrap_or_default()
                        .into(),
                );
            }
            refresh(false);
        }
    });
    window.on_tab_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |i| {
            if blocked() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                state.borrow_mut().tab = i;
                refresh(true);
            }
        }
    });
    window.on_rule_tab_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |i| {
            if blocked() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let mut e = state.borrow_mut();
                e.rule_tab = i;
                e.selected_rule = None;
                drop(e);
                refresh(false);
            }
        }
    });
    window.on_rule_selected({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |i| {
            if blocked() {
                return;
            }
            let mut editor = state.borrow_mut();
            editor.selected_rule = usize::try_from(i)
                .ok()
                .filter(|&index| index < editor.rules().len());
            drop(editor);
            refresh(false);
        }
    });
    window.on_member_toggled({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |i| {
            if blocked() {
                return;
            }
            let mut e = state.borrow_mut();
            let member = usize::try_from(i)
                .ok()
                .and_then(|i| {
                    e.downloaders
                        .gugs
                        .gugs
                        .iter()
                        .filter(|g| matches!(g, AnyGug::Single(_)))
                        .nth(i)
                })
                .map(|g| (g.key().to_owned(), g.name().to_owned()));
            if let (Some(member), Value::Gug(AnyGug::Nested(g))) = (member, &mut e.value) {
                if let Some(i) = g
                    .gugs
                    .iter()
                    .position(|(k, n)| k == &member.0 || n == &member.1)
                {
                    g.gugs.remove(i);
                } else {
                    g.gugs.push(member);
                }
            }
            drop(e);
            refresh(false);
        }
    });
    window.on_rule_action({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        let weak = window.as_weak();
        let blocked = blocked.clone();
        move |action| {
            if blocked() {
                return;
            }
            if let Err(e) = rule_action(&store, &state, &slots, action.as_str(), refresh.clone())
                && let Some(w) = weak.upgrade()
            {
                w.set_error(e.into());
            }
        }
    });
    window.on_converter({
        let store = store.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let slots = slots.clone();
        let blocked = blocked.clone();
        move |which| {
            if blocked() {
                return;
            }
            if which == 2 {
                open_default_processor(&store, &state, &slots, refresh.clone());
            } else {
                open_converter(&state, &slots, which, refresh.clone());
            }
        }
    });
    let apply: Rc<dyn Fn(bool)> = Rc::new({
        let state = state.clone();
        let weak = window.as_weak();
        let close = close.clone();
        let blocked = blocked.clone();
        move |confirmed| {
            if blocked() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            let e = state.borrow();
            if let Err(error) = e.validate() {
                w.set_error(error.into());
                return;
            }
            if !confirmed
                && let Value::Class(c) = &e.value
                && c.uses_api_url()
                && c.api_url(&c.example_url, e.classes.collapse_leading_slashes)
                    .is_ok_and(|url| c.matches(&url, e.classes.collapse_leading_slashes))
            {
                w.set_question("This URL class matches its own API/Redirect URL! This can break a downloader unless there is a more specific URL Class the matches the API URL before this. I recommend you fix this here, but you do not have to. Exit now?".into());
                return;
            }
            match done(e.value.clone()) {
                Ok(()) => close(),
                Err(error) => w.set_error(error.into()),
            }
        }
    });
    window.on_action({
        let apply = apply.clone();
        let close = close.clone();
        let state = state.clone();
        let slots = slots.clone();
        move |action| {
            if editor_blocked(&state.borrow(), &slots) {
                return;
            }
            match action.as_str() {
                "apply" => apply(false),
                "cancel" => close(),
                _ => (),
            }
        }
    });
    window.on_answered({
        let apply = apply.clone();
        let weak = window.as_weak();
        move |yes| {
            if let Some(w) = weak.upgrade() {
                w.set_question(SharedString::new());
            }
            if yes {
                apply(true);
            }
        }
    });
    window.window().on_close_requested({
        let close = close.clone();
        let state = state.clone();
        let slots = slots.clone();
        move || {
            if editor_blocked(&state.borrow(), &slots) {
                return slint::CloseRequestResponse::KeepWindowShown;
            }
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    refresh(true);
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

fn open_converter(
    state: &Rc<RefCell<Editor>>,
    slots: &Slots,
    which: i32,
    refresh: Rc<dyn Fn(bool)>,
) {
    if slots.strings.converter.borrow().is_some() {
        return;
    }
    let e = state.borrow();
    let Value::Class(c) = &e.value else { return };
    let converter = if which == 0 {
        &c.api_lookup_converter
    } else {
        &c.referral.converter
    };
    let done = Rc::new({
        let state = state.clone();
        let refresh = refresh.clone();
        move |converter| {
            if let Value::Class(c) = &mut state.borrow_mut().value {
                if which == 0 {
                    c.api_lookup_converter = converter;
                } else {
                    c.referral.converter = converter;
                }
            }
            refresh(false);
        }
    });
    match crate::string_processor_window::open_converter(
        converter,
        Some(c.example_url.clone()),
        &slots.strings,
        done,
    ) {
        Ok(w) => {
            let refresh_now = refresh.clone();
            w.on_closed(move || refresh(true));
            *slots.strings.converter.borrow_mut() = Some(w);
            refresh_now(false);
        }
        Err(e) => eprintln!("could not open converter: {e}"),
    }
}

fn rule_action(
    store: &Arc<Store>,
    state: &Rc<RefCell<Editor>>,
    slots: &Slots,
    action: &str,
    refresh: Rc<dyn Fn(bool)>,
) -> Result<(), String> {
    if slots.rule.borrow().is_some() {
        return Ok(());
    }
    let mut editor = state.borrow_mut();
    let tab = editor.rule_tab;
    let selected = editor.selected_rule;
    let Value::Class(class) = &mut editor.value else {
        return Ok(());
    };
    if matches!(action, "delete" | "up" | "down") {
        if let Some(index) = selected {
            match action {
                "delete" => match tab {
                    0 => {
                        if index < class.path_components.len() {
                            class.path_components.remove(index);
                        }
                    }
                    1 => {
                        if index < class.parameters.len() {
                            class.parameters.remove(index);
                        }
                    }
                    2 => {
                        if index < class.header_overrides.len() {
                            class.header_overrides.remove(index);
                        }
                    }
                    _ => class.single_value_parameters_match = StringMatch::any(),
                },
                "up" | "down" if tab == 0 => {
                    let to = if action == "up" {
                        index.saturating_sub(1)
                    } else {
                        index + 1
                    };
                    if to < class.path_components.len() {
                        class.path_components.swap(index, to);
                        editor.selected_rule = Some(to);
                    }
                }
                _ => (),
            }
        }
        if action == "delete" {
            editor.selected_rule = None;
        }
        drop(editor);
        refresh(false);
        return Ok(());
    }
    if slots.rule.borrow().is_some() {
        return Ok(());
    }
    let replacing = if action == "add" { None } else { selected };
    if action != "add" && replacing.is_none() {
        return Ok(());
    }
    let value = match tab {
        0 => {
            let (matcher, default) = replacing
                .and_then(|index| class.path_components.get(index))
                .cloned()
                .unwrap_or_else(|| (StringMatch::any(), None));
            Value::Rule(
                RuleEdit {
                    matcher: MatchEditor::new(&matcher),
                    name: String::new(),
                    default_enabled: default.is_some(),
                    default: default.unwrap_or_default(),
                    ephemeral: false,
                    default_processor: StringProcessor::default(),
                },
                false,
            )
        }
        1 => {
            let parameter = replacing
                .and_then(|index| class.parameters.get(index))
                .cloned()
                .unwrap_or_else(|| UrlParameter {
                    name: "new parameter".into(),
                    value: StringMatch::any(),
                    ephemeral: false,
                    default: None,
                    default_processor: StringProcessor::default(),
                });
            Value::Rule(
                RuleEdit {
                    matcher: MatchEditor::new(&parameter.value),
                    name: parameter.name,
                    default_enabled: parameter.default.is_some(),
                    default: parameter.default.unwrap_or_default(),
                    ephemeral: parameter.ephemeral,
                    default_processor: parameter.default_processor,
                },
                true,
            )
        }
        2 => {
            let (header_name, header_value) = replacing
                .and_then(|index| class.header_overrides.get(index))
                .cloned()
                .unwrap_or_default();
            Value::Header(header_name, header_value)
        }
        _ => Value::Rule(
            RuleEdit {
                matcher: MatchEditor::new(&class.single_value_parameters_match),
                name: String::new(),
                default_enabled: false,
                default: String::new(),
                ephemeral: false,
                default_processor: StringProcessor::default(),
            },
            false,
        ),
    };
    let draft = Draft::new(
        editor.classes.clone(),
        editor.downloaders.clone(),
        Kind::Classes,
    );
    drop(editor);
    let child_editor = Editor::new(value, &draft);
    let done: Done = Rc::new({
        let state = state.clone();
        let refresh = refresh.clone();
        move |value| {
            let mut editor = state.borrow_mut();
            let Value::Class(class) = &mut editor.value else {
                return Ok(());
            };
            match value {
                Value::Rule(rule, is_parameter) => {
                    let (matcher, default) = if is_parameter {
                        rule.parameter_value()?
                    } else {
                        rule.value()?
                    };
                    if is_parameter {
                        let other_names = class
                            .parameters
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| Some(*index) != replacing)
                            .map(|(_, parameter)| parameter.name.clone())
                            .collect::<Vec<_>>();
                        definitions::validate_parameter_name(&rule.name, &other_names)?;
                        if let Some(index) = replacing {
                            let parameter = &mut class.parameters[index];
                            parameter.name = rule.name;
                            parameter.value = matcher;
                            parameter.default = default;
                            parameter.ephemeral = rule.ephemeral;
                            parameter.default_processor = rule.default_processor;
                        } else {
                            class.parameters.push(UrlParameter {
                                name: rule.name,
                                value: matcher,
                                default,
                                ephemeral: rule.ephemeral,
                                default_processor: rule.default_processor,
                            });
                        }
                    } else if tab == 3 {
                        class.single_value_parameters_match = matcher;
                    } else if let Some(index) = replacing {
                        class.path_components[index] = (matcher, default);
                    } else {
                        class.path_components.push((matcher, default));
                    }
                }
                Value::Header(header_name, header_value) => {
                    if let Some(index) = replacing {
                        class.header_overrides[index] = (header_name, header_value);
                    } else {
                        class.header_overrides.push((header_name, header_value));
                    }
                }
                _ => (),
            }
            drop(editor);
            refresh(false);
            Ok(())
        }
    });
    let window = open_editor(store, child_editor, slots, slots.rule.clone(), done)?;
    let refresh_now = refresh.clone();
    window.on_closed(move || refresh(true));
    *slots.rule.borrow_mut() = Some(window);
    refresh_now(false);
    Ok(())
}

fn open_default_processor(
    store: &Arc<Store>,
    state: &Rc<RefCell<Editor>>,
    slots: &Slots,
    refresh: Rc<dyn Fn(bool)>,
) {
    if slots.strings.processor.borrow().is_some() {
        return;
    }
    let e = state.borrow();
    let Value::Rule(r, _) = &e.value else { return };
    let done = Rc::new({
        let state = state.clone();
        let refresh = refresh.clone();
        move |processor| {
            if let Value::Rule(r, _) = &mut state.borrow_mut().value {
                r.default_processor = processor;
            }
            refresh(false);
        }
    });
    match crate::string_processor_window::open(
        store,
        &r.default_processor,
        vec![r.default.clone()],
        &slots.strings,
        done,
    ) {
        Ok(window) => {
            let refresh_now = refresh.clone();
            window.on_closed(move || refresh(true));
            *slots.strings.processor.borrow_mut() = Some(window);
            refresh_now(false);
        }
        Err(e) => eprintln!("could not open default processor: {e}"),
    }
}
