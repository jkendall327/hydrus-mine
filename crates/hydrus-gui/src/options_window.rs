//! The options window, bound to its editor ([`Editor`]): the page chosen's
//! rows shown, edits kept until "apply" writes them (and what couldn't be
//! set is said in a popup, as the reference says it), "cancel" forgetting
//! them.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, StandardListViewItem, VecModel};

use hydrus_store::Store;

use crate::options::{Editor, Kind, Row, Settings, Value};
use crate::{OptionRow, OptionsWindow};

fn int(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(if n < 0 { i32::MIN } else { i32::MAX })
}

/// A row as the window shows it.
fn option_row(row: &Row<'_>) -> OptionRow {
    let mut out = OptionRow::default();
    match row {
        Row::Title { title, depth } => {
            out.kind = 0;
            out.label = (*title).into();
            out.depth = int(*depth as i64);
        }
        Row::Opt {
            option,
            depth,
            value,
            number,
        } => {
            out.label = option.label.into();
            out.depth = int(*depth as i64);
            match (&option.kind, value) {
                (Kind::Check, Value::Check(b)) => {
                    out.kind = 1;
                    out.checked = *b;
                }
                (Kind::Int { min, max }, Value::Int(n)) => {
                    out.kind = 2;
                    out.number = int(*n);
                    out.minimum = int(*min);
                    out.maximum = int(*max);
                }
                (
                    Kind::Noneable {
                        none_phrase,
                        min,
                        max,
                        unit,
                        ..
                    },
                    Value::Noneable(n),
                ) => {
                    out.kind = 3;
                    out.number = int(n.unwrap_or(*number));
                    out.minimum = int(*min);
                    out.maximum = int(*max);
                    out.is_none = n.is_none();
                    out.none_phrase = (*none_phrase).into();
                    out.unit = unit.unwrap_or_default().into();
                }
                (Kind::Float { .. }, Value::Float(text)) => {
                    out.kind = 4;
                    out.text = text.as_str().into();
                }
                (Kind::Choice(items), Value::Choice(i)) => {
                    out.kind = 5;
                    let items: Vec<SharedString> = items.iter().map(|&s| s.into()).collect();
                    out.items = ModelRc::new(VecModel::from(items));
                    out.index = int(*i as i64);
                }
                (Kind::Text, Value::Text(text)) => {
                    out.kind = 6;
                    out.text = text.as_str().into();
                }
                _ => {}
            }
        }
    }
    out
}

/// Open the window on the store's settings; it forgets itself from `slot`
/// when closed, and calls `applied` once changes are written.
pub(crate) fn open(
    store: &Arc<Store>,
    slot: &Rc<RefCell<Option<OptionsWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<OptionsWindow, String> {
    let settings = store
        .read(Settings::load)
        .map_err(|e| format!("could not read the options: {e}"))?;
    let window = OptionsWindow::new().map_err(|e| e.to_string())?;
    let editor = Rc::new(RefCell::new(Editor::new(settings)));
    let names: Vec<StandardListViewItem> = editor
        .borrow()
        .page_names()
        .into_iter()
        .map(StandardListViewItem::from)
        .collect();
    window.set_pages(ModelRc::new(VecModel::from(names)));
    // (the rows are made anew only as the page changes: an edit leaves its
    // control as the user left it)
    let show_page = {
        let editor = editor.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let editor = editor.borrow();
            let rows: Vec<OptionRow> = editor.rows().iter().map(option_row).collect();
            window.set_page(int(editor.page() as i64));
            window.set_rows(ModelRc::new(VecModel::from(rows)));
        }
    };
    show_page();
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
    let at = |i: i32| usize::try_from(i).unwrap_or(usize::MAX);
    window.on_page_chosen({
        let editor = editor.clone();
        let show_page = show_page.clone();
        move |i| {
            editor.borrow_mut().show_page(at(i));
            show_page();
        }
    });
    window.on_check_toggled({
        let editor = editor.clone();
        move |i, checked| editor.borrow_mut().check(at(i), checked)
    });
    window.on_number_edited({
        let editor = editor.clone();
        move |i, n| editor.borrow_mut().number(at(i), i64::from(n))
    });
    window.on_none_toggled({
        let editor = editor.clone();
        move |i, none| editor.borrow_mut().none(at(i), none)
    });
    window.on_text_edited({
        let editor = editor.clone();
        move |i, text| editor.borrow_mut().text(at(i), &text)
    });
    window.on_choice_chosen({
        let editor = editor.clone();
        move |i, index| editor.borrow_mut().choose(at(i), at(index))
    });
    window.on_apply({
        let editor = editor.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            let (after, before, problems) = {
                let editor = editor.borrow();
                let (after, before, problems) = editor.applied();
                (after, before.clone(), problems)
            };
            let saved = store.write_and_refresh(move |ctx| {
                after.save(ctx.conn(), &before)?;
                let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
                for problem in &problems {
                    let job = hydrus_store::popups::Job::text(problem.clone(), now as f64);
                    hydrus_store::popups::add(ctx.conn(), &job, now)?;
                }
                Ok(())
            });
            if let Err(e) = saved {
                eprintln!("could not save the options: {e}");
                return;
            }
            applied();
            close();
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
