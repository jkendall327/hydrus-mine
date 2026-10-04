//! Reusable typed formula and rule windows for downloaders and sidecars.
pub use crate::formula_editors::FormulaTestData;
use crate::formula_editors::{FormulaChild, FormulaEditor, Rule, RuleEditor, formula_summary};
use crate::{FormulaRuleWindow, FormulaWindow, TableRow};
use hydrus_parse::formula::{Formula, FormulaKind, HtmlContent, JsonContent};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

/// Child windows owned by a caller, retained while they are open.
#[derive(Clone, Default)]
pub struct Slots {
    pub exchange: crate::downloader_interchange_window::Slots,
    pub formula: Rc<RefCell<Option<FormulaWindow>>>,
    pub rule: Rc<RefCell<Option<FormulaRuleWindow>>>,
    /// Separate child slots at each depth retain arbitrary recursive editors.
    pub child: Rc<RefCell<Option<Box<Slots>>>>,
    pub strings: crate::string_processor_window::Slots,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("formula", &self.formula.borrow().is_some())
            .field("rule", &self.rule.borrow().is_some())
            .finish_non_exhaustive()
    }
}
impl Slots {
    /// Cancel the formula and every child editor when its owning dialog closes.
    pub fn cancel(&self) {
        let formula = self
            .formula
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(w) = formula {
            w.invoke_cancel();
        } else {
            self.cancel_children();
        }
    }
    fn has_children(&self) -> bool {
        self.child.borrow().is_some()
            || self.rule.borrow().is_some()
            || self.strings.has_open()
            || self.exchange.has_open()
    }
    fn cancel_children(&self) {
        let child = self.child.borrow_mut().take();
        if let Some(child) = child {
            child.cancel();
        }
        self.exchange.cancel();
        self.strings.cancel_all();
        let rule = self
            .rule
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = rule {
            window.invoke_cancel();
        }
    }
}
fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items.into_iter().map(Into::into).collect::<Vec<_>>(),
    ))
}
fn rows(items: impl IntoIterator<Item = String>, selected: &[usize]) -> ModelRc<TableRow> {
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .enumerate()
            .map(|(i, t)| TableRow {
                cells: strings([t]),
                selected: selected.contains(&i),
            })
            .collect::<Vec<_>>(),
    ))
}
fn int(n: impl TryInto<i32>) -> i32 {
    n.try_into().unwrap_or(0)
}
fn show(w: &FormulaWindow, e: &FormulaEditor) {
    w.set_supported(e.supported());
    w.set_name(e.formula.name.as_str().into());
    let (kind, content, choices) = match &e.formula.kind {
        FormulaKind::Html { content, .. } => {
            if let HtmlContent::Attribute(a) = content {
                w.set_attribute(a.as_str().into());
            }
            (
                0,
                match content {
                    HtmlContent::Attribute(_) => 0,
                    HtmlContent::Text => 1,
                    HtmlContent::Html => 2,
                },
                vec!["attribute", "string", "html"],
            )
        }
        FormulaKind::Json { content, .. } => (
            1,
            match content {
                JsonContent::Strings => 0,
                JsonContent::Json => 1,
                JsonContent::DictKeys => 2,
            },
            vec!["string", "json", "dictionary keys"],
        ),
        FormulaKind::ContextVariable { variable } => {
            w.set_variable(variable.as_str().into());
            (4, 0, vec![])
        }
        FormulaKind::Static { text, count } => {
            w.set_static_text(text.as_str().into());
            w.set_output_count(int(*count));
            (5, 0, vec![])
        }
        FormulaKind::Nested { main, sub } => {
            w.set_main_label(formula_summary(main).into());
            w.set_sub_label(formula_summary(sub).into());
            (2, 0, vec![])
        }
        FormulaKind::Zipper { phrase, .. } => {
            w.set_phrase(phrase.as_str().into());
            (3, 0, vec![])
        }
    };
    w.set_kind(kind);
    w.set_content(content);
    w.set_contents(strings(choices.into_iter().map(str::to_owned)));
    w.set_rules(rows(e.queue_descriptions(), &e.selected()));
    w.set_examples(strings(e.test.examples.iter().enumerate().map(|(i, t)| {
        format!("example {} ({} characters)", i + 1, t.chars().count())
    })));
    w.set_example(int(e.example));
    w.set_selected(!e.selected().is_empty());
    w.set_processing(e.formula.processor.button_label().into());
    w.set_newline_note(if e.test.collapse_newlines { "Newlines are removed from parsed strings right after parsing, before string processing." } else { "Newlines are not collapsed here (probably a note parser)" }.into());
    match e.results() {
        Ok(results) => {
            w.set_status(format!("{} parsed strings", results.len()).into());
            w.set_results(rows(results, &[]));
        }
        Err(error) => {
            w.set_status(error.into());
            w.set_results(rows(Vec::new(), &[]));
        }
    }
}
fn read(w: &FormulaWindow, e: &mut FormulaEditor) {
    if e.supported() {
        e.formula.name = w.get_name().to_string();
        match &mut e.formula.kind {
            FormulaKind::Html { content, .. } => {
                *content = match w.get_content() {
                    1 => HtmlContent::Text,
                    2 => HtmlContent::Html,
                    _ => HtmlContent::Attribute(w.get_attribute().to_string()),
                }
            }
            FormulaKind::Json { content, .. } => {
                *content = match w.get_content() {
                    1 => JsonContent::Json,
                    2 => JsonContent::DictKeys,
                    _ => JsonContent::Strings,
                }
            }
            FormulaKind::ContextVariable { variable } => *variable = w.get_variable().to_string(),
            FormulaKind::Static { text, count } => {
                *text = w.get_static_text().to_string();
                *count = usize::try_from(w.get_output_count().clamp(1, 65535)).unwrap_or(1);
            }
            FormulaKind::Zipper { phrase, .. } => *phrase = w.get_phrase().to_string(),
            FormulaKind::Nested { .. } => {}
        }
    }
    e.test.text = w.get_document().to_string();
    if let Some(text) = e.test.examples.get_mut(e.example) {
        text.clone_from(&e.test.text);
    }
    e.test.context = w
        .get_context()
        .lines()
        .filter_map(|line| {
            line.split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
        })
        .collect();
}
/// Edit a formula in isolation. Apply returns the typed draft; Cancel leaves
/// the original unchanged. Recursive children use separate slots at each depth.
pub fn open(
    store: &Arc<Store>,
    formula: &Formula,
    test_data: FormulaTestData,
    slots: &Slots,
    applied: Rc<dyn Fn(Formula)>,
) -> Result<FormulaWindow, slint::PlatformError> {
    let w = FormulaWindow::new()?;
    let active = Rc::new(Cell::new(true));
    let blocked: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let slots = slots.clone();
        move || !active.get() || slots.has_children()
    });
    w.set_document(
        test_data
            .examples
            .first()
            .unwrap_or(&test_data.text)
            .as_str()
            .into(),
    );
    w.set_context(
        test_data
            .context
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
    );
    let state = Rc::new(RefCell::new(FormulaEditor::new(formula, test_data)));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        let slots = slots.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                show(&w, &state.borrow());
                w.set_child_open(slots.has_children());
            }
        }
    });
    w.on_exchange({
        let weak = w.as_weak();
        let slots = slots.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |importing| {
            use hydrus_gui_model::downloader_interchange::{Definition, Native};
            if blocked() {
                return;
            }
            let definitions = vec![Definition::new(Native::Formula(
                state.borrow().formula.clone(),
            ))];
            let preview = Rc::new(|definitions: Vec<Definition>| {
                if definitions.len() != 1 || !matches!(definitions[0].native, Native::Formula(_)) {
                    return Err("Import one parsing formula into this editor.".into());
                }
                Ok(format!(
                    "Replace this draft with formula: {}",
                    definitions[0].name()
                ))
            });
            let applied = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                move |mut definitions: Vec<Definition>| {
                    let definition = definitions.pop().ok_or("No formula to import.")?;
                    let Native::Formula(formula) = definition.native else {
                        return Err("Import one parsing formula.".into());
                    };
                    state.borrow_mut().formula = formula;
                    refresh();
                    Ok(())
                }
            });
            match crate::downloader_interchange_window::open(
                &slots.exchange,
                importing,
                definitions,
                preview,
                applied,
            ) {
                Ok(child) => {
                    let refresh = refresh.clone();
                    child.on_closed(move || refresh());
                }
                Err(e) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_veto(e.into());
                    }
                }
            }
            refresh();
        }
    });
    w.on_changed({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                read(&w, &mut state.borrow_mut());
                w.set_veto(SharedString::new());
            }
            refresh();
        }
    });
    w.on_example_chosen({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                refresh();
                return;
            }
            if let Some(w) = weak.upgrade() {
                let mut e = state.borrow_mut();
                read(&w, &mut e);
                e.choose_example(usize::try_from(w.get_example()).unwrap_or(0));
                w.set_document(e.test.text.as_str().into());
            }
            refresh();
        }
    });
    w.on_type_chosen({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                refresh();
                return;
            }
            if let Some(w) = weak.upgrade() {
                let mut e = state.borrow_mut();
                if w.get_allow_type_change() {
                    e.change_kind(usize::try_from(w.get_kind()).unwrap_or(0));
                }
            }
            refresh();
        }
    });
    w.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |r, c, s| {
            if blocked() {
                return;
            }
            if let Ok(r) = usize::try_from(r) {
                state.borrow_mut().click(r, c, s);
            }
            refresh();
        }
    });
    let edit_child: Rc<dyn Fn(FormulaChild)> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        let blocked = blocked.clone();
        let active = active.clone();
        move |address| {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                read(&w, &mut state.borrow_mut());
            }
            let (formula, test) = {
                let e = state.borrow();
                let Some(formula) = e.child(address) else {
                    return;
                };
                (formula, e.child_test_data(address))
            };
            let child_slots = Slots::default();
            let applied = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                move |formula| {
                    if !active.get() {
                        return;
                    }
                    state.borrow_mut().put_child(address, formula);
                    refresh();
                }
            });
            match open(&store, &formula, test, &child_slots, applied) {
                Ok(child) => {
                    let parent_slots = slots.clone();
                    let refresh = refresh.clone();
                    child.on_closed(move |_| {
                        parent_slots.child.borrow_mut().take();
                        refresh();
                    });
                    *child_slots.formula.borrow_mut() = Some(child);
                    *slots.child.borrow_mut() = Some(Box::new(child_slots));
                }
                Err(error) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_veto(error.to_string().into());
                    }
                }
            }
            refresh();
        }
    });
    w.on_edit_child({
        let edit_child = edit_child.clone();
        move |second| {
            edit_child(if second {
                FormulaChild::Sub
            } else {
                FormulaChild::Main
            })
        }
    });
    w.on_member_exchange({
        let weak = w.as_weak();
        let state = state.clone();
        let slots = slots.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        let active = active.clone();
        move |importing| {
            use hydrus_gui_model::downloader_interchange::{Definition, Native};
            if blocked() {
                return;
            }
            let definitions = {
                let e = state.borrow();
                let FormulaKind::Zipper { formulae, .. } = &e.formula.kind else {
                    return;
                };
                e.selected()
                    .iter()
                    .filter_map(|i| formulae.get(*i))
                    .cloned()
                    .map(|f| Definition::new(Native::Formula(f)))
                    .collect()
            };
            let preview = Rc::new(|definitions: Vec<Definition>| {
                if definitions.is_empty()
                    || definitions
                        .iter()
                        .any(|d| !matches!(d.native, Native::Formula(_)))
                {
                    return Err("Import one or more component formulae.".into());
                }
                Ok(format!(
                    "Append {} component formulae to this draft.",
                    definitions.len()
                ))
            });
            let applied = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                move |definitions: Vec<Definition>| {
                    if !active.get() {
                        return Ok(());
                    }
                    for definition in definitions {
                        if let Native::Formula(formula) = definition.native {
                            state
                                .borrow_mut()
                                .put_child(FormulaChild::Member(None), formula);
                        }
                    }
                    refresh();
                    Ok(())
                }
            });
            match crate::downloader_interchange_window::open(
                &slots.exchange,
                importing,
                definitions,
                preview,
                applied,
            ) {
                Ok(child) => {
                    let refresh = refresh.clone();
                    child.on_closed(move || refresh());
                }
                Err(error) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_veto(error.into());
                    }
                }
            }
            refresh();
        }
    });
    let edit: Rc<dyn Fn(Option<usize>)> = Rc::new({
        let state = state.clone();
        let refresh = refresh.clone();
        let store = store.clone();
        let slots = slots.clone();
        let blocked = blocked.clone();
        let active = active.clone();
        let edit_child = edit_child.clone();
        move |at| {
            if blocked() {
                return;
            }
            if matches!(state.borrow().formula.kind, FormulaKind::Zipper { .. }) {
                edit_child(FormulaChild::Member(at));
                return;
            }
            if state.borrow().kind_index() > 1 {
                return;
            }
            let rule = {
                let e = state.borrow();
                at.and_then(|i| e.rules().get(i).cloned())
                    .unwrap_or_else(|| e.new_rule())
            };
            let put: Rc<dyn Fn(Rule)> = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                let active = active.clone();
                move |r| {
                    if !active.get() {
                        return;
                    }
                    state.borrow_mut().put(at, r);
                    refresh();
                }
            });
            match open_rule(&store, &rule, &slots, put) {
                Ok(w) => {
                    let refresh = refresh.clone();
                    w.on_closed(move || refresh());
                    *slots.rule.borrow_mut() = Some(w);
                }
                Err(e) => eprintln!("could not open formula rule: {e}"),
            }
            refresh();
        }
    });
    w.on_add({
        let edit = edit.clone();
        move || edit(None)
    });
    w.on_edit({
        let state = state.clone();
        let edit = edit.clone();
        move || {
            let selected = state.borrow().selected().first().copied();
            if let Some(i) = selected {
                edit(Some(i));
            }
        }
    });
    w.on_row_activated(move |r| {
        if let Ok(i) = usize::try_from(r) {
            edit(Some(i));
        }
    });
    w.on_delete({
        let weak = w.as_weak();
        let state = state.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let count = state.borrow().selected().len();
                if count > 0 {
                    w.set_question(format!("Remove {count} selected?").into());
                    w.set_deleting(true);
                }
            }
        }
    });
    w.on_chosen({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |i| {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                if w.get_deleting() && i == 0 {
                    state.borrow_mut().delete();
                }
                w.set_deleting(false);
            }
            refresh();
        }
    });
    w.on_cancelled({
        let weak = w.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                w.set_deleting(false);
            }
        }
    });
    w.on_shift({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |down| {
            if blocked() {
                return;
            }
            state.borrow_mut().shift(down);
            refresh();
        }
    });
    w.on_test({
        let weak = w.as_weak();
        let state = state.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                read(&w, &mut state.borrow_mut());
                match state.borrow().results() {
                    Ok(results) => {
                        w.set_status(format!("{} parsed strings", results.len()).into());
                        w.set_results(rows(results, &[]));
                    }
                    Err(e) => {
                        w.set_status(e.into());
                        w.set_results(rows(Vec::new(), &[]));
                    }
                }
            }
        }
    });
    w.on_edit_processing({
        let weak = w.as_weak();
        let state = state.clone();
        let store = store.clone();
        let slots = slots.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            if slots.strings.processor.borrow().is_some() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                read(&w, &mut state.borrow_mut());
            }
            let (processor, texts) = {
                let e = state.borrow();
                (e.formula.processor.clone(), e.processor_texts())
            };
            let applied = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                move |p| {
                    state.borrow_mut().formula.processor = p;
                    refresh();
                }
            });
            match crate::string_processor_window::open(
                &store,
                &processor,
                texts,
                &slots.strings,
                applied,
            ) {
                Ok(w) => {
                    let refresh = refresh.clone();
                    w.on_closed(move || refresh());
                    *slots.strings.processor.borrow_mut() = Some(w);
                }
                Err(e) => eprintln!("could not open string processor: {e}"),
            }
            refresh();
        }
    });
    let close: Rc<dyn Fn(bool)> = Rc::new({
        let weak = w.as_weak();
        let slots = slots.clone();
        let active = active.clone();
        move |accepted| {
            if !active.replace(false) {
                return;
            }
            slots.cancel_children();
            let window = weak.upgrade();
            if let Some(w) = &window {
                let _ = w.hide();
            }
            slots.formula.borrow_mut().take();
            if let Some(w) = window {
                w.invoke_closed(accepted);
            }
        }
    });
    w.on_apply({
        let weak = w.as_weak();
        let state = state.clone();
        let close = close.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                read(&w, &mut state.borrow_mut());
                let value = state.borrow().value();
                match value {
                    Ok(f) => {
                        close(true);
                        applied(f);
                    }
                    Err(e) => w.set_veto(e.into()),
                }
            }
        }
    });
    w.on_cancel({
        let close = close.clone();
        move || close(false)
    });
    w.window().on_close_requested(move || {
        close(false);
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    w.show()?;
    Ok(w)
}
fn show_rule(w: &FormulaRuleWindow, e: &RuleEditor) {
    w.set_description(e.value().description().into());
    w.set_match_label(e.string_match().describe(false, false).into());
    w.set_attributes(rows(e.attrs.iter().map(|(k, v)| format!("{k}={v}")), &[]));
}
fn read_rule(w: &FormulaRuleWindow, e: &mut RuleEditor) {
    e.kind = usize::try_from(w.get_rule_type()).unwrap_or(0);
    e.tag = w.get_tag().to_string();
    e.index = (!e.html || w.get_index_on()).then(|| i64::from(w.get_index()));
    e.depth = usize::try_from(w.get_depth()).unwrap_or(1);
    e.match_on = w.get_match_on();
}
fn open_rule(
    store: &Arc<Store>,
    rule: &Rule,
    slots: &Slots,
    applied: Rc<dyn Fn(Rule)>,
) -> Result<FormulaRuleWindow, slint::PlatformError> {
    let w = FormulaRuleWindow::new()?;
    let active = Rc::new(Cell::new(true));
    let blocked: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let strings = slots.strings.clone();
        move || !active.get() || strings.has_open()
    });
    let e = RuleEditor::new(rule);
    w.set_window_title(
        if e.html {
            "edit tag rule"
        } else {
            "edit parse rule"
        }
        .into(),
    );
    w.set_html(e.html);
    w.set_types(strings(
        if e.html {
            vec![
                "search descendants",
                "walk back up ancestors",
                "search previous siblings",
                "search next siblings",
            ]
        } else {
            vec![
                "dictionary entry by key",
                "all dictionary/list items",
                "indexed item",
                "filter strings/numbers/bools with string match",
                "walk back up ancestors",
                "de-minify json",
            ]
        }
        .into_iter()
        .map(str::to_owned),
    ));
    w.set_rule_type(int(e.kind));
    w.set_tag(e.tag.as_str().into());
    w.set_index_on(e.index.is_some());
    w.set_index(int(e.index.unwrap_or(0).clamp(-65536, 65535)));
    w.set_depth(int(e.depth.clamp(1, if e.html { 255 } else { 64 })));
    w.set_match_on(e.match_on);
    let state = Rc::new(RefCell::new(e));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                show_rule(&w, &state.borrow());
            }
        }
    });
    w.on_changed({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                read_rule(&w, &mut state.borrow_mut());
            }
            refresh();
        }
    });
    w.on_attribute_put({
        let weak = w.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                state
                    .borrow_mut()
                    .put_attribute(w.get_attr_key().to_string(), w.get_attr_value().to_string());
            }
            refresh();
        }
    });
    w.on_attribute_remove({
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |r| {
            if blocked() {
                return;
            }
            if let Ok(i) = usize::try_from(r)
                && i < state.borrow().attrs.len()
            {
                state.borrow_mut().attrs.remove(i);
            }
            refresh();
        }
    });
    w.on_edit_match({
        let store = store.clone();
        let strings = slots.strings.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let weak = w.as_weak();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            let m = state.borrow().string_match().clone();
            crate::string_processor_window::open_match(
                &store,
                &m,
                &strings,
                Rc::new({
                    let state = state.clone();
                    let refresh = refresh.clone();
                    let active = active.clone();
                    move |m| {
                        if !active.get() {
                            return;
                        }
                        state.borrow_mut().set_match(m);
                        refresh();
                    }
                }),
            );
            let step = strings
                .step
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(step) = step {
                let parent = weak.clone();
                step.on_closed(move || {
                    if let Some(w) = parent.upgrade() {
                        w.set_child_open(false);
                    }
                });
                if let Some(w) = weak.upgrade() {
                    w.set_child_open(true);
                }
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let slot = slots.rule.clone();
        let strings = slots.strings.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            strings.cancel_all();
            let window = weak.upgrade();
            if let Some(w) = &window {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
            if let Some(w) = window {
                w.invoke_closed();
            }
        }
    });
    w.on_apply({
        let weak = w.as_weak();
        let state = state.clone();
        let close = close.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                read_rule(&w, &mut state.borrow_mut());
            }
            let r = state.borrow().value();
            close();
            applied(r);
        }
    });
    w.on_cancel({
        let close = close.clone();
        move || close()
    });
    w.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    w.show()?;
    Ok(w)
}
