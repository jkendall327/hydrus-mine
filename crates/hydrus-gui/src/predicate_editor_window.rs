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
use crate::predicate_editors::{Context, Editor, Field, Panel, Pressed};
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
            row.kind = 1;
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
                },
            );
            self.fields[p] = rows;
            return;
        }
        let model = &self.fields[p];
        for i in 0..panel.fields.len() {
            let row = field_row(panel, i);
            let old = model.row_data(i);
            // (what was typed or chosen shows already)
            let redrawn = matches!(panel.fields[i], Field::Ticks { .. } | Field::Tree { .. });
            let changed = (redrawn && Some(i) == set)
                || old
                    .as_ref()
                    .is_none_or(|old| old.shown != row.shown || old.enabled != row.enabled);
            if changed {
                model.set_row_data(i, row);
            }
        }
    }
}

/// The file whose path the clipboard holds: its pixel and perceptual
/// hashes, as the reference's "Paste image!" takes them.
fn pasted_hashes() -> Result<(hydrus_core::Sha256, Vec<hydrus_core::PerceptualHash>), String> {
    let text = arboard::Clipboard::new()
        .and_then(|mut c| c.get_text())
        .map_err(|_| "Did not see an image bitmap or a file path in the clipboard!".to_owned())?;
    let path = std::path::PathBuf::from(text.trim());
    if !path.is_file() {
        return Err("Sorry, that clipboard text did not look like a valid file path!".into());
    }
    hydrus_media::MediaTools::new().similar_search_hashes(&path)
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
    store: Arc<Store>,
    editor: Editor,
    context: Context,
    text: TextContext,
    chosen: Rc<dyn Fn(Vec<Predicate>)>,
) -> Result<(), String> {
    if let Some(old) = slot.borrow_mut().take() {
        let _ = old.hide();
    }
    let window = PredicateEditorWindow::new().map_err(|e| e.to_string())?;
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
    let state = Rc::new(RefCell::new(State {
        editor,
        context,
        page: 0,
        panels: Rc::new(VecModel::default()),
        fields: Vec::new(),
        recent: Vec::new(),
    }));
    let text = Rc::new(text);
    // show page `page`
    let show_page = {
        let weak = window.as_weak();
        let state = state.clone();
        let text = text.clone();
        let store = store.clone();
        move |page: usize| {
            let Some(window) = weak.upgrade() else { return };
            if !window.get_question().is_empty() {
                return;
            }
            let mut state = state.borrow_mut();
            if page >= state.editor.pages.len() {
                return;
            }
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
            window.set_panels(ModelRc::from(panels));
            window.set_error(SharedString::new());
        }
    };
    show_page(0);
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    let done = Rc::new(Cell::new(false));
    let finish = {
        let close = close.clone();
        let done = done.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |predicates: Vec<Predicate>| {
            if weak.upgrade().is_none_or(|window| {
                !window.window().is_visible() || !window.get_question().is_empty()
            }) {
                return;
            }
            if done.replace(true) {
                return;
            }
            close();
            let system: Vec<SystemPredicate> = predicates
                .iter()
                .filter_map(|p| match p {
                    Predicate::System(s) => Some(s.clone()),
                    _ => None,
                })
                .collect();
            change_recent(&store, |recent| recent.push(&system));
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
        let state = state.clone();
        let show_page = show_page.clone();
        let weak = window.as_weak();
        move |i| {
            if weak
                .upgrade()
                .is_none_or(|window| !window.get_question().is_empty())
            {
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
        let state = state.clone();
        let weak = window.as_weak();
        move |p: i32, set: Option<i32>, change: &dyn Fn(&mut Panel) -> Option<String>| {
            if weak.upgrade().is_none_or(|window| {
                !window.window().is_visible() || !window.get_question().is_empty()
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
            let said = change(panel);
            state.refresh(p, set.map(index), &before);
            if let Some(window) = weak.upgrade() {
                window.set_error(said.unwrap_or_default().into());
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
                Pressed::Paste => match pasted_hashes() {
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
        let edit = edit.clone();
        let weak = window.as_weak();
        move |accepted| {
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
    window.on_ok({
        let weak = window.as_weak();
        let state = state.clone();
        let finish = finish.clone();
        move |p| {
            let made = {
                let state = state.borrow();
                let Some(panel) = state.panels().get(index(p)) else {
                    return;
                };
                panel.predicates(&state.context)
            };
            match made {
                Ok(predicates) => finish(predicates),
                Err(why) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(why.into());
                    }
                }
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let slot = slot.clone();
        move || {
            slot.borrow_mut().take();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
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
