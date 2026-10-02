//! The options window, bound to its editor ([`Editor`]): the page chosen's
//! rows shown, edits kept until "apply" writes them (and what couldn't be
//! set is said in a popup, as the reference says it), "cancel" forgetting
//! them.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{
    ComponentHandle as _, Model as _, ModelRc, SharedString, StandardListViewItem, VecModel,
};

use hydrus_store::Store;

use crate::options::{
    Editor, Kind, Row, SEARCH_PLACEHOLDER, SEARCH_SHOWN, Settings, Unit, Value, duration_fields,
};
use crate::{DurationField, OptionRow, OptionsWindow};

/// A time's fields as the window shows them.
fn fields(seconds: f64, units: &[Unit]) -> ModelRc<DurationField> {
    let fields: Vec<DurationField> = duration_fields(seconds, units)
        .into_iter()
        .zip(units)
        .map(|(value, unit)| DurationField {
            value: int(value),
            maximum: int(unit.max()),
            label: unit.label().into(),
        })
        .collect();
    ModelRc::new(VecModel::from(fields))
}

fn int(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(if n < 0 { i32::MIN } else { i32::MAX })
}

/// A row as the window shows it (a sort's types are the store's).
fn option_row(row: &Row<'_>, store: &Store) -> OptionRow {
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
                (Kind::NoneableText { none_phrase }, Value::NoneableText { none, text }) => {
                    out.kind = 7;
                    out.text = text.as_str().into();
                    out.is_none = *none;
                    out.none_phrase = (*none_phrase).into();
                }
                (Kind::Duration { units, .. }, Value::Duration(seconds)) => {
                    out.kind = 8;
                    out.fields = fields(*seconds, units);
                }
                (
                    Kind::Velocity {
                        number: (min, max),
                        per,
                        units,
                        ..
                    },
                    Value::Velocity(n, seconds),
                ) => {
                    out.kind = 9;
                    out.number = int(*n);
                    out.minimum = int(*min);
                    out.maximum = int(*max);
                    out.per = (*per).into();
                    out.fields = fields(*seconds, units);
                }
                (Kind::Sort, Value::Sort(sort)) => {
                    out.kind = 10;
                    let choices = crate::sort::page_choices(store, &sort.by);
                    let names: Vec<SharedString> =
                        choices.iter().map(|c| c.name.as_str().into()).collect();
                    out.items = ModelRc::new(VecModel::from(names));
                    if let Some(i) = choices.iter().position(|c| c.by == sort.by) {
                        out.index = int(i as i64);
                        let orders: Vec<SharedString> =
                            choices[i].orders.iter().map(|&o| o.into()).collect();
                        out.orders = ModelRc::new(VecModel::from(orders));
                        out.order_index = i32::from(!sort.ascending);
                    }
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
        let store = store.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let editor = editor.borrow();
            let rows: Vec<OptionRow> = editor
                .rows()
                .iter()
                .enumerate()
                .map(|(i, row)| OptionRow {
                    found: editor.found(i),
                    ..option_row(row, &store)
                })
                .collect();
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
    // the search: suggestions as it is typed in; one chosen shows its page,
    // its row highlighted, and the search is cleared (as the reference's)
    window.set_search_placeholder(SEARCH_PLACEHOLDER.into());
    window.set_shown(int(SEARCH_SHOWN as i64));
    let matches: Rc<RefCell<Vec<crate::options::Suggestion>>> = Rc::default();
    window.on_search_edited({
        let editor = editor.clone();
        let matches = matches.clone();
        let weak = window.as_weak();
        move |text| {
            let Some(window) = weak.upgrade() else { return };
            let found: Vec<crate::options::Suggestion> =
                editor.borrow().search(&text).into_iter().cloned().collect();
            let texts: Vec<SharedString> = found.iter().map(|s| s.text.as_str().into()).collect();
            *matches.borrow_mut() = found;
            window.set_matches(ModelRc::new(VecModel::from(texts)));
            window.set_match_highlighted(-1);
        }
    });
    window.on_move_match({
        let matches = matches.clone();
        let weak = window.as_weak();
        move |by| {
            let Some(window) = weak.upgrade() else { return };
            let count = int(matches.borrow().len() as i64);
            if count == 0 {
                return;
            }
            let at = (window.get_match_highlighted() + by).clamp(0, count - 1);
            window.set_match_highlighted(at);
        }
    });
    window.on_search_chosen({
        let editor = editor.clone();
        let matches = matches.clone();
        let show_page = show_page.clone();
        let weak = window.as_weak();
        move |i| {
            let chosen = usize::try_from(i)
                .ok()
                .and_then(|i| matches.borrow().get(i).cloned());
            let Some(chosen) = chosen else {
                return;
            };
            editor.borrow_mut().go_to(&chosen);
            show_page();
            matches.borrow_mut().clear();
            if let Some(window) = weak.upgrade() {
                window.set_search_text(SharedString::new());
                window.set_matches(ModelRc::default());
                window.set_match_highlighted(-1);
            }
        }
    });
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
    window.on_field_edited({
        let editor = editor.clone();
        move |i, field, n| editor.borrow_mut().field(at(i), at(field), i64::from(n))
    });
    window.on_choice_chosen({
        let editor = editor.clone();
        move |i, index| editor.borrow_mut().choose(at(i), at(index))
    });
    // a sort's type (in its default order, as the reference's control
    // sets it), or its order; the row shows the type's orders
    let sort_edited = {
        let editor = editor.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |i: i32,
              edit: &dyn Fn(&mut hydrus_core::pages::PageSort, &[crate::sort::PageChoice])| {
            let Some(window) = weak.upgrade() else { return };
            let mut editor = editor.borrow_mut();
            let rows = editor.rows();
            let Some(Row::Opt {
                value: Value::Sort(sort),
                ..
            }) = rows.get(at(i))
            else {
                return;
            };
            let mut sort = sort.clone();
            drop(rows);
            let choices = crate::sort::page_choices(&store, &sort.by);
            edit(&mut sort, &choices);
            editor.sort(at(i), sort);
            if let Some(row) = editor.rows().get(at(i)) {
                window.get_rows().set_row_data(
                    at(i),
                    OptionRow {
                        found: editor.found(at(i)),
                        ..option_row(row, &store)
                    },
                );
            }
        }
    };
    window.on_sort_chosen({
        let sort_edited = sort_edited.clone();
        move |i, index| {
            sort_edited(i, &|sort, choices| {
                if let Some(choice) = choices.get(at(index)) {
                    sort.by = choice.by.clone();
                    sort.ascending = choice.default_ascending;
                }
            });
        }
    });
    window.on_order_chosen(move |i, index| {
        sort_edited(i, &|sort, _| sort.ascending = index == 0);
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
