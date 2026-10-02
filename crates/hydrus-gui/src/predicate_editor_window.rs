//! The "input predicate" window, bound to an [`Editor`]: its pages, its
//! ready-made buttons (each adding its predicates) and its panels (each
//! field changed as it is set, "ok" adding what the panel makes, or saying
//! why it can't).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString, VecModel};

use hydrus_search::{Predicate, TextContext, predicate_text};

use crate::predicate_editors::{Context, Editor, Field, Panel};
use crate::{EditorField, EditorPanel, PredicateEditorWindow};

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
    }
    row
}

/// A ready-made button's label: its own, or its predicates' texts.
pub fn button_label(button: &crate::predicate_editors::Button, text: &TextContext) -> String {
    button.label.clone().unwrap_or_else(|| {
        button
            .predicates
            .iter()
            .map(|p| predicate_text(&Predicate::System(p.clone()), text))
            .collect::<Vec<_>>()
            .join(", ")
    })
}

/// The window's state: the editor, the page shown, and each of the page's
/// panels' fields as shown (updated in place, so a field being typed in
/// keeps its focus).
struct State {
    editor: Editor,
    context: Context,
    page: usize,
    fields: Vec<Rc<VecModel<EditorField>>>,
}

impl State {
    fn panels(&self) -> &[Panel] {
        &self.editor.pages[self.page].panels
    }

    /// Show the fields of panel `p` that were shown or hidden, or greyed or
    /// not, by what was set.
    fn refresh(&self, p: usize) {
        let (Some(panel), Some(model)) = (self.panels().get(p), self.fields.get(p)) else {
            return;
        };
        for i in 0..panel.fields.len() {
            let row = field_row(panel, i);
            let old = model.row_data(i);
            // (what was set shows already)
            let changed = old
                .as_ref()
                .is_none_or(|old| old.shown != row.shown || old.enabled != row.enabled);
            if changed {
                model.set_row_data(i, row);
            }
        }
    }
}

/// Open the editor; a button or "ok" hands its predicates to `chosen` and
/// closes it. One already open is replaced.
pub(crate) fn open(
    slot: &Rc<RefCell<Option<PredicateEditorWindow>>>,
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
        fields: Vec::new(),
    }));
    let text = Rc::new(text);
    // show page `page`
    let show_page = {
        let weak = window.as_weak();
        let state = state.clone();
        let text = text.clone();
        move |page: usize| {
            let Some(window) = weak.upgrade() else { return };
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
            let fields: Vec<Rc<VecModel<EditorField>>> = shown
                .panels
                .iter()
                .map(|panel| {
                    Rc::new(VecModel::from(
                        (0..panel.fields.len())
                            .map(|i| field_row(panel, i))
                            .collect::<Vec<_>>(),
                    ))
                })
                .collect();
            let panels: Vec<EditorPanel> = fields
                .iter()
                .map(|f| EditorPanel {
                    fields: ModelRc::from(f.clone()),
                })
                .collect();
            state.fields = fields;
            window.set_page(i32::try_from(page).unwrap_or(0));
            window.set_buttons(ModelRc::new(VecModel::from(labels)));
            window.set_panels(ModelRc::new(VecModel::from(panels)));
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
        move |predicates: Vec<Predicate>| {
            if done.replace(true) {
                return;
            }
            close();
            chosen(predicates);
        }
    };
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
    // a field set: change it, and show what that changes
    let edit = {
        let state = state.clone();
        move |p: i32, change: &dyn Fn(&mut Panel)| {
            let Ok(p) = usize::try_from(p) else { return };
            let mut state = state.borrow_mut();
            let page = state.page;
            let Some(panel) = state.editor.pages[page].panels.get_mut(p) else {
                return;
            };
            change(panel);
            state.refresh(p);
        }
    };
    let index = |i: i32| usize::try_from(i).unwrap_or(usize::MAX);
    window.on_chose({
        let edit = edit.clone();
        move |p, f, option| edit(p, &|panel| panel.choose(index(f), index(option)))
    });
    window.on_ticked({
        let edit = edit.clone();
        move |p, f, option, on| edit(p, &|panel| panel.tick(index(f), index(option), on))
    });
    window.on_number_edited({
        let edit = edit.clone();
        move |p, f, value| edit(p, &|panel| panel.set_number(index(f), i64::from(value)))
    });
    window.on_text_edited({
        let edit = edit.clone();
        move |p, f, text| edit(p, &|panel| panel.set_text(index(f), &text))
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
