//! The siblings and parents windows, sharing the relationship editor and list.

use std::cell::RefCell;
use std::rc::Rc;

use hydrus_gui_model::tag_relationships::{Question, Relationships};
use slint::{ComponentHandle as _, ModelRc, VecModel};

use crate::{TableRow, TagRelationshipsWindow};

#[derive(Clone)]
enum Operation {
    Add,
    Delete,
    Import(String),
    Apply,
}

struct Binding {
    model: Relationships,
    operation: Option<Operation>,
    answers: Vec<Option<String>>,
    inputs: Vec<(String, String)>,
}

/// Open a staged editor. Closing drops changes; Apply calls `applied` only
/// after the writer commits the relations and their derived display counts.
pub(crate) fn open(
    model: Relationships,
    slot: &Rc<RefCell<Option<TagRelationshipsWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<TagRelationshipsWindow, slint::PlatformError> {
    let window = TagRelationshipsWindow::new()?;
    window.set_siblings(model.kind() == hydrus_store::display::RelationKind::Siblings);
    window.set_service_names(ModelRc::new(VecModel::from(
        model
            .service_names()
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>(),
    )));
    let inputs = vec![(String::new(), String::new()); model.service_names().len()];
    let binding = Rc::new(RefCell::new(Binding {
        inputs,
        model,
        operation: None,
        answers: Vec::new(),
    }));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let binding = binding.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let binding = binding.borrow();
            let model = &binding.model;
            let presentation: hydrus_core::tag_presentation::TagPresentation = model
                .store()
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            let colours: hydrus_core::tag_presentation::NamespaceColours = model
                .store()
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            let rows: Vec<TableRow> = model
                .rows()
                .into_iter()
                .map(|r| TableRow {
                    cells: ModelRc::new(VecModel::from(vec![
                        r.status.into(),
                        presentation.render(&r.pair.0).into(),
                        presentation.render(&r.pair.1).into(),
                        r.note.into(),
                    ])),
                    selected: r.selected,
                })
                .collect();
            window.set_rows(ModelRc::new(VecModel::from(rows)));
            let (left, right) = model.inputs();
            let tags = |tags: Vec<String>| {
                ModelRc::new(VecModel::from(
                    tags.into_iter()
                        .map(|t| crate::list_text(&presentation.render(&t), colours.tag(&t)))
                        .collect::<Vec<_>>(),
                ))
            };
            window.set_left_tags(tags(left));
            window.set_right_tags(tags(right));
            window.set_service_index(i32::try_from(model.service()).unwrap_or(0));
            window.set_can_add(model.can_add());
            window.set_has_selection(model.has_selection());
            let (all, pending, whole) = model.filters();
            window.set_show_all(all);
            window.set_show_pending(pending);
            window.set_whole_chain(whole);
            window.set_workspace(model.workspace().join(", ").into());
            match model.sync_status() {
                Ok(status) => window.set_sync_status(status.into()),
                Err(e) => {
                    window.set_error(format!("could not load application status: {e}").into());
                }
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
        }
    });
    let run: Rc<dyn Fn()> = Rc::new({
        let binding = binding.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        let close = close.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut binding = binding.borrow_mut();
            let Some(op) = binding.operation.clone() else {
                return;
            };
            let answers = binding.answers.clone();
            let result = match &op {
                Operation::Add => binding.model.add(&answers),
                Operation::Delete => binding.model.delete(&answers),
                Operation::Import(text) => binding.model.import(text, &answers).map(|odd| {
                    if odd {
                        window.set_error("Uneven number of tags in clipboard!".into());
                    }
                }),
                Operation::Apply => {
                    if let Some(q) = binding
                        .model
                        .apply_question()
                        .filter(|_| answers.is_empty())
                    {
                        Err(q)
                    } else if answers.first().is_some_and(Option::is_none) {
                        Ok(())
                    } else {
                        match binding.model.apply() {
                            Ok(()) => {
                                drop(binding);
                                applied();
                                close();
                                return;
                            }
                            Err(e) => {
                                window
                                    .set_error(format!("could not apply the changes: {e}").into());
                                Ok(())
                            }
                        }
                    }
                }
            };
            if let Err(Question {
                message,
                yes,
                no,
                reason,
            }) = result
            {
                window.set_question(message.into());
                window.set_yes_label(yes.into());
                window.set_no_label(no.into());
                window.set_ask_reason(reason);
                window.set_reason("".into());
            } else {
                binding.operation = None;
                binding.answers.clear();
                window.set_question("".into());
            }
            drop(binding);
            refresh();
        }
    });
    let start = {
        let binding = binding.clone();
        let weak = window.as_weak();
        let run = run.clone();
        move |op| {
            if let Some(w) = weak.upgrade() {
                w.set_error("".into());
            }
            let mut b = binding.borrow_mut();
            b.operation = Some(op);
            b.answers.clear();
            drop(b);
            run();
        }
    };
    window.on_add({
        let start = start.clone();
        move || start(Operation::Add)
    });
    window.on_delete({
        let start = start.clone();
        move || start(Operation::Delete)
    });
    window.on_apply({
        let start = start.clone();
        move || start(Operation::Apply)
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.on_answered({
        let binding = binding.clone();
        let weak = window.as_weak();
        let run = run.clone();
        move |yes| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if yes && w.get_ask_reason() && w.get_reason().is_empty() {
                return;
            }
            let response = yes.then(|| {
                if w.get_ask_reason() {
                    w.get_reason().to_string()
                } else {
                    String::new()
                }
            });
            binding.borrow_mut().answers.push(response);
            w.set_question("".into());
            run();
        }
    });
    window.on_service_chosen({
        let binding = binding.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move |i| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            let mut b = binding.borrow_mut();
            let previous = b.model.service();
            b.inputs[previous] = (
                w.get_left_input().to_string(),
                w.get_right_input().to_string(),
            );
            b.model
                .choose_service(usize::try_from(i).unwrap_or(usize::MAX));
            let (left, right) = b.inputs[b.model.service()].clone();
            drop(b);
            w.set_left_input(left.into());
            w.set_right_input(right.into());
            refresh();
        }
    });
    window.on_enter_tags({
        let binding = binding.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move |right, text| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            match binding.borrow_mut().model.enter_tags(right, &text) {
                Ok(()) => {
                    w.set_error("".into());
                    if right {
                        w.set_right_input("".into());
                    } else {
                        w.set_left_input("".into());
                    }
                }
                Err(e) => w.set_error(e.into()),
            }
            refresh();
        }
    });
    window.on_remove_input({
        let binding = binding.clone();
        let refresh = refresh.clone();
        move |right, i| {
            let mut b = binding.borrow_mut();
            let (left, rights) = b.model.inputs();
            if let Some(t) =
                (if right { &rights } else { &left }).get(usize::try_from(i).unwrap_or(usize::MAX))
            {
                b.model.remove_input(right, t);
            }
            drop(b);
            refresh();
        }
    });
    window.on_filters_changed({
        let binding = binding.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                binding.borrow_mut().model.set_filters(
                    w.get_show_all(),
                    w.get_show_pending(),
                    w.get_whole_chain(),
                );
            }
            refresh();
        }
    });
    window.on_wipe_workspace({
        let binding = binding.clone();
        let refresh = refresh.clone();
        move || {
            binding.borrow_mut().model.wipe_workspace();
            refresh();
        }
    });
    window.on_row_clicked({
        let binding = binding.clone();
        let refresh = refresh.clone();
        move |i, ctrl, shift| {
            binding
                .borrow_mut()
                .model
                .click(usize::try_from(i).unwrap_or(usize::MAX), ctrl, shift);
            refresh();
        }
    });
    window.on_row_activated({
        let binding = binding.clone();
        let start = start.clone();
        move |i| {
            binding.borrow_mut().model.click(
                usize::try_from(i).unwrap_or(usize::MAX),
                false,
                false,
            );
            start(Operation::Delete);
        }
    });
    window.on_sort({
        let binding = binding.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move |c, asc| {
            binding
                .borrow_mut()
                .model
                .sort(usize::try_from(c).unwrap_or(0), asc);
            if let Some(w) = weak.upgrade() {
                w.set_sort_column(c);
                w.set_ascending(asc);
            }
            refresh();
        }
    });
    window.on_import_pairs({
        let weak = window.as_weak();
        let start = start.clone();
        move |file| {
            let text = if file {
                let Some(path) = crate::pick(crate::Pick::Files, "Select the file to import.")
                    .into_iter()
                    .next()
                else {
                    return;
                };
                std::fs::read_to_string(path).map_err(|e| e.to_string())
            } else {
                crate::from_clipboard()
            };
            match text {
                Ok(t) => start(Operation::Import(t)),
                Err(e) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_error(e.into());
                    }
                }
            }
        }
    });
    window.on_export_pairs({
        let binding = binding.clone();
        let weak = window.as_weak();
        move |file| {
            let text = binding.borrow().model.export();
            if file {
                let Some(path) = rfd::FileDialog::new()
                    .set_title("Set the export path.")
                    .set_file_name(
                        if binding.borrow().model.kind()
                            == hydrus_store::display::RelationKind::Siblings
                        {
                            "siblings.txt"
                        } else {
                            "parents.txt"
                        },
                    )
                    .save_file()
                else {
                    return;
                };
                if let Err(e) = std::fs::write(path, text)
                    && let Some(w) = weak.upgrade()
                {
                    w.set_error(e.to_string().into());
                }
            } else {
                crate::copy_to_clipboard(&text);
            }
        }
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    refresh();
    window.show()?;
    Ok(window)
}
