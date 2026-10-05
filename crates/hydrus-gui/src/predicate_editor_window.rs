//! The "input predicate" window, bound to an [`Editor`]: its pages, its
//! ready-made buttons (each adding its predicates) and its panels (each
//! field changed as it is set, "ok" adding what the panel makes, or saying
//! why it can't).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString, VecModel};

use hydrus_core::search::recent::RecentPredicates;
use hydrus_search::{Predicate, SystemPredicate, TextContext, predicate_text};
use hydrus_store::Store;

pub use crate::predicate_editors::button_label;
use crate::predicate_editors::{
    Blank, Context, Editor, Field, Kind, Panel, Pressed,
    defaults::{CustomDefaults, CustomDefaultsExt},
};
use crate::{EditorField, EditorPanel, EditorTreeRow, PredicateEditorWindow};

/// A field as the window shows it.
fn field_row(panel: &Panel, i: usize) -> EditorField {
    let strings = |options: &[String]| -> ModelRc<SharedString> {
        ModelRc::new(VecModel::from(
            options
                .iter()
                .map(|o| SharedString::from(o.as_str()))
                .collect::<Vec<_>>(),
        ))
    };
    let mut row = EditorField {
        shown: panel.shown(i),
        enabled: panel.enabled(i),
        line: i32::from(panel.second_line.contains(&i)),
        ..EditorField::default()
    };
    let int = |n: i64| i32::try_from(n).unwrap_or(if n < 0 { i32::MIN } else { i32::MAX });
    match &panel.fields[i] {
        Field::Label(text) => {
            row.kind = 0;
            row.text = text.as_str().into();
        }
        Field::Choice { options, chosen } => {
            row.kind = if panel.kind == Kind::Size && i == 1 {
                8
            } else if panel.kind == Kind::Hash {
                9
            } else {
                1
            };
            row.options = strings(options);
            row.chosen = i32::try_from(*chosen).unwrap_or(0);
            // (room for the longest option, and the arrow)
            let longest = options.iter().map(|o| o.chars().count()).max().unwrap_or(0);
            row.width = (longest as f32 * 7.5 + 48.0).max(64.0);
        }
        Field::Ticks { options, ticked } => {
            row.kind = 2;
            row.options = strings(options);
            row.ticked = ModelRc::new(VecModel::from(ticked.clone()));
        }
        Field::Number {
            value,
            min,
            max,
            before,
            after,
        } => {
            row.kind = 3;
            row.value = int(*value);
            row.minimum = int(*min);
            row.maximum = int(*max);
            row.before = before.as_str().into();
            row.after = after.as_str().into();
        }
        Field::Text { text, placeholder } => {
            row.kind = 4;
            row.text = text.as_str().into();
            row.placeholder = placeholder.as_str().into();
        }
        Field::Lines { text, placeholder } => {
            row.kind = 5;
            row.text = text.as_str().into();
            row.placeholder = placeholder.as_str().into();
        }
        Field::Tree { groups } => {
            row.kind = 6;
            row.rows = tree_rows(groups);
        }
        Field::Button(label) => {
            row.kind = 7;
            row.text = label.as_str().into();
        }
    }
    row
}

fn hash_text(panel: &Panel) -> SharedString {
    if panel.kind == Kind::Hash
        && let Field::Lines { text, .. } = &panel.fields[2]
    {
        text.as_str().into()
    } else {
        SharedString::new()
    }
}

/// A panel's fields as the window shows them.
fn field_rows(panel: &Panel) -> Rc<VecModel<EditorField>> {
    Rc::new(VecModel::from(
        (0..panel.fields.len())
            .map(|i| field_row(panel, i))
            .collect::<Vec<_>>(),
    ))
}

/// The window's state: the editor, the page shown, and its panels' fields
/// as shown (updated in place, so a field being typed in keeps its focus).
struct State {
    editor: Editor,
    context: Context,
    page: usize,
    panels: Rc<VecModel<EditorPanel>>,
    fields: Vec<Rc<VecModel<EditorField>>>,
    /// The recent predicates the page shown shows.
    recent: Vec<SystemPredicate>,
}

impl State {
    fn panels(&self) -> &[Panel] {
        &self.editor.pages[self.page].panels
    }

    /// Show panel `p` after field `set` (or, with none, the panel by a
    /// button) changed it from `before`: what it showed or hid, or greyed
    /// or not; ticks and trees as they now are; and the whole panel again
    /// if other fields changed with it (as a rating chosen chooses "is").
    fn refresh(&mut self, p: usize, set: Option<usize>, before: &[Field]) {
        let Some(panel) = self.panels().get(p) else {
            return;
        };
        let others = panel
            .fields
            .iter()
            .zip(before)
            .enumerate()
            .any(|(i, (now, was))| Some(i) != set && now != was);
        if others {
            let rows = field_rows(panel);
            self.panels.set_row_data(
                p,
                EditorPanel {
                    fields: ModelRc::from(rows.clone()),
                    two_lines: !panel.second_line.is_empty(),
                    hash_layout: panel.kind == Kind::Hash,
                    hash_text: hash_text(panel),
                },
            );
            self.fields[p] = rows;
            return;
        }
        if panel.kind == Kind::Hash
            && let Some(mut row) = self.panels.row_data(p)
        {
            row.hash_text = hash_text(panel);
            self.panels.set_row_data(p, row);
        }
        let model = &self.fields[p];
        for i in 0..panel.fields.len() {
            let row = field_row(panel, i);
            let old = model.row_data(i);
            // The active input already shows what was typed. A button also
            // synchronizes cached text before freezing or cleaning that draft.
            let redrawn = matches!(panel.fields[i], Field::Ticks { .. } | Field::Tree { .. });
            let size_control = panel.kind == Kind::Size && matches!(i, 1..=3);
            let hash_control = panel.kind == Kind::Hash;
            let changed = (redrawn && Some(i) == set)
                || old.as_ref().is_none_or(|old| {
                    old.shown != row.shown
                        || old.enabled != row.enabled
                        || ((size_control || hash_control)
                            && (old.chosen != row.chosen
                                || old.value != row.value
                                || old.text != row.text))
                        || (set.is_none() && old.text != row.text)
                });
            if changed {
                model.set_row_data(i, row);
            }
        }
    }
}

/// The file whose path the clipboard holds: its pixel and perceptual
/// hashes, as the reference's "Paste image!" takes them.
fn pasted_hashes(
    store: &Arc<Store>,
) -> Result<(hydrus_core::Sha256, Vec<hydrus_core::PerceptualHash>), String> {
    let text = arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .map_err(|_| "Did not see an image bitmap or a file path in the clipboard!".to_owned())?;
    let path = std::path::PathBuf::from(text.trim());
    if !path.is_file() {
        return Err("Sorry, that clipboard text did not look like a valid file path!".into());
    }
    hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
        .tools()
        .similar_search_hashes(&path)
}

/// The recent predicates kept in `store`.
fn recent_of(store: &Store) -> RecentPredicates {
    store.read(hydrus_store::settings::get).unwrap_or_default()
}

/// Change the recent predicates kept in `store`.
fn change_recent(store: &Store, change: impl FnOnce(&mut RecentPredicates)) {
    let mut recent = recent_of(store);
    change(&mut recent);
    if let Err(e) = store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &recent)) {
        eprintln!("could not keep the recent predicates: {e}");
    }
}

/// Open the editor; a button, a recent predicate or "ok" hands its
/// predicates to `chosen`, keeps them as recent predicates in `store` (as
/// the reference's `FleshOutPredicates` does), and closes it. One already
/// open is replaced.
pub(crate) fn open(
    slot: &Rc<RefCell<Option<PredicateEditorWindow>>>,
    store: &Arc<Store>,
    mut editor: Editor,
    context: Context,
    text: TextContext,
    chosen: Rc<dyn Fn(Vec<Predicate>)>,
    owner: Option<Rc<dyn Fn() -> bool>>,
) -> Result<(), String> {
    let old = slot.borrow_mut().take();
    if let Some(old) = old {
        old.invoke_cancel();
    }
    let defaults = store
        .read(hydrus_store::settings::get::<CustomDefaults>)
        .map_err(|e| e.to_string())?;
    editor.apply_defaults(&defaults, &context);
    let window = PredicateEditorWindow::new().map_err(|e| e.to_string())?;
    window.set_editing_existing(editor.supplied.is_some());
    window.set_batch_mode(editor.batch.is_some());
    if let Some(batch) = &editor.batch {
        window.set_simple_texts(ModelRc::new(VecModel::from(
            batch
                .simple
                .iter()
                .map(|s| SharedString::from(s.as_str()))
                .collect::<Vec<_>>(),
        )));
        window.set_invertible_labels(ModelRc::new(VecModel::from(
            batch
                .invertible
                .iter()
                .map(|p| SharedString::from(predicate_text(p, &text)))
                .collect::<Vec<_>>(),
        )));
    }
    let closed = Rc::new(Cell::new(false));
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let closed = closed.clone();
        let owner = owner.clone();
        move || !closed.get() && owner.as_ref().is_none_or(|owner| owner())
    });
    let notices = Rc::new(crate::predicate_notice::Notices::new(
        &window,
        valid.clone(),
    ));
    window.set_note(editor.note.clone().unwrap_or_default().into());
    let names: Vec<SharedString> = if editor.pages.len() > 1 {
        editor
            .pages
            .iter()
            .map(|p| p.name.as_str().into())
            .collect()
    } else {
        Vec::new()
    };
    window.set_pages(ModelRc::new(VecModel::from(names)));
    window.set_two_columns(editor.blank == crate::predicate_editors::Blank::FileProperties);
    let filesize = editor.blank == Blank::Filesize;
    let hash = editor.blank == Blank::Hash;
    let state = Rc::new(RefCell::new(State {
        editor,
        context,
        page: 0,
        panels: Rc::new(VecModel::default()),
        fields: Vec::new(),
        recent: Vec::new(),
    }));
    window.on_force_radio_ok({
        let state = state.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        let store = store.clone();
        move |panel| {
            if !valid() {
                return false;
            }
            let Some(window) = weak.upgrade() else {
                return false;
            };
            if !window.window().is_visible()
                || !window.get_question().is_empty()
                || window.get_notice_open()
            {
                return false;
            }
            let Ok(index) = usize::try_from(panel) else {
                return false;
            };
            if !state
                .borrow()
                .panels()
                .get(index)
                .is_some_and(|panel| matches!(panel.kind, Kind::Size | Kind::Hash))
            {
                return false;
            }
            // Remember the focused radio panel before an unforced key bubbles.
            window.set_radio_default_panel(panel);
            match store.read(hydrus_store::radio_return::load) {
                Ok(policy) => policy.force_dialog_ok,
                Err(error) => {
                    eprintln!("Could not read the radio Return preference: {error}");
                    false
                }
            }
        }
    });
    let text = Rc::new(text);
    // show page `page`
    let show_page = {
        let valid = valid.clone();
        let weak = window.as_weak();
        let state = state.clone();
        let text = text.clone();
        let store = Arc::clone(store);
        move |page: usize| {
            if !valid() {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            if !window.get_question().is_empty() || window.get_notice_open() {
                return;
            }
            let mut state = state.borrow_mut();
            if page >= state.editor.pages.len() {
                return;
            }
            window.set_radio_default_panel(-1);
            state.page = page;
            let shown = &state.editor.pages[page];
            let labels: Vec<SharedString> = shown
                .buttons
                .iter()
                .map(|b| button_label(b, &text).into())
                .collect();
            let fields: Vec<Rc<VecModel<EditorField>>> =
                shown.panels.iter().map(field_rows).collect();
            let panels: Rc<VecModel<EditorPanel>> = Rc::new(VecModel::from(
                fields
                    .iter()
                    .zip(&shown.panels)
                    .map(|(f, panel)| EditorPanel {
                        fields: ModelRc::from(f.clone()),
                        two_lines: !panel.second_line.is_empty(),
                        hash_layout: panel.kind == Kind::Hash,
                        hash_text: hash_text(panel),
                    })
                    .collect::<Vec<_>>(),
            ));
            state.fields = fields;
            state.panels = panels.clone();
            state.recent = state.editor.pages[page].recent(&recent_of(&store));
            let recent: Vec<SharedString> = state
                .recent
                .iter()
                .map(|p| predicate_text(&Predicate::System(p.clone()), &text).into())
                .collect();
            window.set_recent(ModelRc::new(VecModel::from(recent)));
            window.set_page(i32::try_from(page).unwrap_or(0));
            window.set_buttons(ModelRc::new(VecModel::from(labels)));
            window.set_edit_order(ModelRc::new(VecModel::from(
                state.editor.batch.as_ref().map_or_else(
                    || {
                        (0..panels.row_count())
                            .filter_map(|i| i32::try_from(i).ok())
                            .collect()
                    },
                    |b| b.order.clone(),
                ),
            )));
            window.set_panels(ModelRc::from(panels));
            if let Some(panel) = state.panels().iter().find(|panel| panel.kind == Kind::Hash)
                && let Field::Lines { text, .. } = &panel.fields[2]
            {
                window.set_hash_text(text.as_str().into());
            }
            window.set_error(SharedString::new());
        }
    };
    show_page(0);
    let close = {
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        let notices = notices.clone();
        let closed = closed.clone();
        move || {
            if closed.replace(true) {
                return;
            }
            let window = weak.upgrade();
            if let Some(window) = &window {
                let _ = window.hide();
            }
            if let (Some(slot), Some(window)) = (slot.upgrade(), window.as_ref()) {
                let owns = slot
                    .borrow()
                    .as_ref()
                    .is_some_and(|current| std::ptr::eq(current.window(), window.window()));
                if owns {
                    slot.borrow_mut().take();
                }
            }
            notices.cancel();
            if let Some(window) = window {
                window.invoke_closed();
            }
        }
    };
    let done = Rc::new(Cell::new(false));
    let finish = {
        let valid = valid.clone();
        let close = close.clone();
        let done = done.clone();
        let store = Arc::clone(store);
        let weak = window.as_weak();
        move |predicates: Vec<Predicate>| {
            if !valid() {
                return;
            }
            if weak.upgrade().is_none_or(|window| {
                !window.window().is_visible()
                    || (!window.get_question().is_empty() || window.get_notice_open())
            }) {
                return;
            }
            if done.replace(true) {
                return;
            }
            close();
            if owner.as_ref().is_some_and(|owner| !owner()) {
                return;
            }
            change_recent(&store, |recent| recent.push_all(&predicates));
            chosen(predicates);
        }
    };
    window.on_recent_clicked({
        let state = state.clone();
        let finish = finish.clone();
        move |i| {
            let chosen = usize::try_from(i)
                .ok()
                .and_then(|i| state.borrow().recent.get(i).cloned());
            if let Some(predicate) = chosen {
                finish(vec![Predicate::System(predicate)]);
            }
        }
    });
    window.on_recent_forgotten({
        let valid = valid.clone();
        let store = Arc::clone(store);
        let state = state.clone();
        let show_page = show_page.clone();
        let weak = window.as_weak();
        move |i| {
            if !valid() {
                return;
            }
            if weak.upgrade().is_none_or(|window| {
                !window.window().is_visible()
                    || (!window.get_question().is_empty() || window.get_notice_open())
            }) {
                return;
            }
            let forgotten = usize::try_from(i)
                .ok()
                .and_then(|i| state.borrow().recent.get(i).cloned());
            if let Some(predicate) = forgotten {
                change_recent(&store, |recent| recent.remove(&predicate));
                let page = state.borrow().page;
                show_page(page);
            }
        }
    });
    window.on_page_chosen(move |page| show_page(usize::try_from(page).unwrap_or(0)));
    window.on_button_clicked({
        let state = state.clone();
        let finish = finish.clone();
        move |i| {
            let predicates = {
                let state = state.borrow();
                let page = &state.editor.pages[state.page];
                let Some(button) = usize::try_from(i).ok().and_then(|i| page.buttons.get(i)) else {
                    return;
                };
                button
                    .predicates
                    .iter()
                    .cloned()
                    .map(Predicate::System)
                    .collect()
            };
            finish(predicates);
        }
    });
    let index = |i: i32| usize::try_from(i).unwrap_or(usize::MAX);
    // a field set (or, with none, a button pressed): change the panel, and
    // show what that changes; what a button says to the user
    let edit = {
        let notices = notices.clone();
        let valid = valid.clone();
        let state = state.clone();
        let weak = window.as_weak();
        move |p: i32, set: Option<i32>, change: &dyn Fn(&mut Panel) -> Option<String>| {
            if !valid() {
                return;
            }
            if weak.upgrade().is_none_or(|window| {
                !window.window().is_visible()
                    || (!window.get_question().is_empty() || window.get_notice_open())
            }) {
                return;
            }
            let Ok(p) = usize::try_from(p) else { return };
            let mut state = state.borrow_mut();
            let page = state.page;
            let Some(panel) = state.editor.pages[page].panels.get_mut(p) else {
                return;
            };
            let before = panel.fields.clone();
            let hash = panel.kind == Kind::Hash;
            let said = change(panel);
            let hash_text = hash.then(|| match &panel.fields[2] {
                Field::Lines { text, .. } => text.clone(),
                _ => String::new(),
            });
            state.refresh(p, set.map(index), &before);
            drop(state);
            if let Some(window) = weak.upgrade() {
                if let Some(text) = hash_text {
                    window.set_hash_text(text.into());
                }
                match said {
                    Some(said) if hash => {
                        if let Err(error) = notices.show(&said) {
                            window.set_error(format!("{said}\n{error}").into());
                        }
                    }
                    said => window.set_error(said.unwrap_or_default().into()),
                }
            }
        }
    };
    window.on_chose({
        let edit = edit.clone();
        move |p, f, option| {
            edit(p, Some(f), &|panel| {
                panel.choose(index(f), index(option));
                None
            });
        }
    });
    window.on_ticked({
        let edit = edit.clone();
        move |p, f, option, on| {
            edit(p, Some(f), &|panel| {
                panel.tick(index(f), index(option), on);
                None
            });
        }
    });
    window.on_number_edited({
        let edit = edit.clone();
        move |p, f, value| {
            edit(p, Some(f), &|panel| {
                panel.set_number(index(f), i64::from(value));
                None
            });
        }
    });
    window.on_text_edited({
        let edit = edit.clone();
        move |p, f, text| {
            edit(p, Some(f), &|panel| {
                panel.set_text(index(f), &text);
                None
            });
        }
    });
    window.on_tree_ticked({
        let edit = edit.clone();
        move |p, f, group, option, on| {
            edit(p, Some(f), &|panel| {
                panel.tick_tree(index(f), index(group), usize::try_from(option).ok(), on);
                None
            });
        }
    });
    window.on_expanded({
        let edit = edit.clone();
        move |p, f, group, expanded| {
            edit(p, Some(f), &|panel| {
                panel.expand(index(f), index(group), expanded);
                None
            });
        }
    });
    let pending_question = Rc::new(RefCell::new(None));
    window.on_pressed({
        let store = store.clone();
        let edit = edit.clone();
        let pending_question = pending_question.clone();
        let weak = window.as_weak();
        move |p, f| {
            edit(p, None, &|panel| match panel.press(index(f)) {
                Pressed::Done => None,
                Pressed::Warning(said) => Some(said),
                Pressed::Confirm(question) => {
                    *pending_question.borrow_mut() = Some((p, f));
                    if let Some(window) = weak.upgrade() {
                        window.set_question(question.into());
                    }
                    None
                }
                Pressed::Paste => match pasted_hashes(&store) {
                    Ok((pixel, perceptual)) => {
                        panel.paste_hashes(&pixel, &perceptual);
                        None
                    }
                    Err(why) => Some(why),
                },
            });
        }
    });
    window.on_answer({
        let valid = valid.clone();
        let edit = edit.clone();
        let weak = window.as_weak();
        move |accepted| {
            if !valid() {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            if !window.window().is_visible() {
                return;
            }
            let Some((p, f)) = pending_question.borrow_mut().take() else {
                return;
            };
            window.set_question(SharedString::new());
            if accepted {
                edit(p, None, &|panel| match panel.press_confirmed(index(f)) {
                    Pressed::Warning(said) => Some(said),
                    _ => None,
                });
            }
        }
    });
    window.on_defaults_menu({
        let valid = valid.clone();
        let weak = window.as_weak();
        let state = state.clone();
        let store = Arc::clone(store);
        move |p| {
            if !valid() {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            if !window.window().is_visible()
                || (!window.get_question().is_empty() || window.get_notice_open())
            {
                return;
            }
            let state = state.borrow();
            if state.editor.batch.is_some() {
                return;
            }
            let Some(panel) = state.panels().get(index(p)) else {
                return;
            };
            let defaults = match store.read(hydrus_store::settings::get::<CustomDefaults>) {
                Ok(defaults) => defaults,
                Err(e) => {
                    window.set_error(e.to_string().into());
                    return;
                }
            };
            let mut actions = vec![SharedString::from("set this as new default")];
            if defaults.uses(panel.kind) {
                actions.push("reset to original default".into());
            }
            window.set_defaults_actions(ModelRc::new(VecModel::from(actions)));
        }
    });
    window.on_defaults_action({
        let valid = valid.clone();
        let weak = window.as_weak();
        let state = state.clone();
        let store = Arc::clone(store);
        move |p, action| {
            if !valid() {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            if !window.window().is_visible()
                || (!window.get_question().is_empty() || window.get_notice_open())
            {
                return;
            }
            let state = state.borrow();
            if state.editor.batch.is_some() {
                return;
            }
            let Some(panel) = state.panels().get(index(p)) else {
                return;
            };
            let made = match action.as_str() {
                "set this as new default" => match panel.predicates_for_default(&state.context) {
                    Ok(predicates) => Some(predicates),
                    Err(e) => {
                        window.set_error(e.into());
                        return;
                    }
                },
                "reset to original default" => None,
                _ => return,
            };
            let kind = panel.kind;
            let result = store.write(move |writer| {
                let mut defaults = hydrus_store::settings::get::<CustomDefaults>(writer.conn())?;
                if let Some(predicates) = made {
                    defaults.save(predicates);
                } else {
                    defaults.reset(kind);
                }
                hydrus_store::settings::set(writer.conn(), &defaults)
            });
            match result {
                Ok(()) => window.set_error(SharedString::new()),
                Err(e) => window.set_error(e.to_string().into()),
            }
        }
    });
    window.on_simple_edited({
        let valid = valid.clone();
        let weak = window.as_weak();
        let state = state.clone();
        move |i, value| {
            if !valid()
                || weak.upgrade().is_none_or(|w| {
                    !w.window().is_visible() || !w.get_question().is_empty() || w.get_notice_open()
                })
            {
                return;
            }
            if let Some(text) = state
                .borrow_mut()
                .editor
                .batch
                .as_mut()
                .and_then(|b| b.simple.get_mut(index(i)))
            {
                *text = value.to_string();
                if let Some(window) = weak.upgrade() {
                    window.get_simple_texts().set_row_data(index(i), value);
                }
            }
        }
    });
    window.on_invertible_clicked({
        let valid = valid.clone();
        let weak = window.as_weak();
        let state = state.clone();
        let text = text.clone();
        move |i| {
            if !valid() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !window.window().is_visible()
                || !window.get_question().is_empty()
                || window.get_notice_open()
            {
                return;
            }
            let mut state = state.borrow_mut();
            let Some(batch) = state.editor.batch.as_mut() else {
                return;
            };
            let Some(p) = batch.invertible.get_mut(index(i)) else {
                return;
            };
            if let Some(inverse) = p.inverse(&|s| hydrus_search::entry::is_incdec(s, &text)) {
                *p = inverse;
            }
            window.set_invertible_labels(ModelRc::new(VecModel::from(
                batch
                    .invertible
                    .iter()
                    .map(|p| SharedString::from(predicate_text(p, &text)))
                    .collect::<Vec<_>>(),
            )));
        }
    });
    window.on_ok({
        let notices = notices.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        let state = state.clone();
        let finish = finish.clone();
        move |p| {
            if !valid() {
                return;
            }
            if weak.upgrade().is_none_or(|window| {
                !window.window().is_visible()
                    || (!window.get_question().is_empty() || window.get_notice_open())
            }) {
                return;
            }
            let (hash, made) = {
                let state = state.borrow();
                if state.editor.batch.is_some() {
                    (false, state.editor.mixed_predicates(&state.context))
                } else {
                    let Some(panel) = state.panels().get(index(p)) else {
                        return;
                    };
                    (panel.kind == Kind::Hash, panel.predicates(&state.context))
                }
            };
            match made {
                Ok(predicates) => finish(predicates),
                Err(why) => {
                    if let Some(window) = weak.upgrade() {
                        if hash {
                            let why = format!("Sorry, predicate was not valid: {why}");
                            if let Err(error) = notices.show(&why) {
                                window.set_error(format!("{why}\n{error}").into());
                            }
                        } else {
                            window.set_error(why.into());
                        }
                    }
                }
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    window.set_comparison_focus(filesize);
    window.set_hash_focus(hash);
    *slot.borrow_mut() = Some(window);
    Ok(())
}

/// The filetype tree's rows (`ui/filetype_tree.slint`): each group, ticked
/// if all of its filetypes are, then its filetypes.
pub(crate) fn tree_rows(groups: &[crate::predicate_editors::TreeGroup]) -> ModelRc<EditorTreeRow> {
    let mut rows = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        let g = i32::try_from(g).unwrap_or(0);
        rows.push(EditorTreeRow {
            text: group.name.as_str().into(),
            ticked: group.ticked.iter().all(|t| *t),
            partial: group.ticked.iter().any(|t| *t) && !group.ticked.iter().all(|t| *t),
            group: g,
            option: -1,
            shown: true,
            expanded: group.expanded,
        });
        for (o, (option, ticked)) in group.options.iter().zip(&group.ticked).enumerate() {
            rows.push(EditorTreeRow {
                text: option.as_str().into(),
                ticked: *ticked,
                partial: false,
                group: g,
                option: i32::try_from(o).unwrap_or(0),
                shown: group.expanded,
                expanded: false,
            });
        }
    }
    ModelRc::new(VecModel::from(rows))
}
