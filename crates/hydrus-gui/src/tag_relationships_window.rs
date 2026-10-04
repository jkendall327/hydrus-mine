//! The siblings and parents windows, sharing the relationship editor and list.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use hydrus_gui_model::tag_relationships::{Question, Relationships};
use hydrus_gui_model::write_autocomplete::{Paste, WriteAutocomplete};
use slint::{ComponentHandle as _, ModelRc, VecModel};

use crate::{TableRow, TagRelationshipsWindow};

#[derive(Clone)]
enum Operation {
    Add,
    Delete,
    Import(String),
    Apply,
    Paste {
        right: bool,
        tags: Vec<String>,
        message: String,
    },
}

struct Binding {
    model: Relationships,
    operation: Option<Operation>,
    answers: Vec<Option<String>>,
    inputs: Vec<(WriteAutocomplete, WriteAutocomplete)>,
}

impl Binding {
    fn sync_context(&mut self) {
        let (left, right) = self.model.inputs();
        let pair = &mut self.inputs[self.model.service()];
        pair.0.set_context_tags(left);
        pair.1.set_context_tags(right);
    }
    fn input_mut(&mut self, right: bool) -> &mut WriteAutocomplete {
        let pair = &mut self.inputs[self.model.service()];
        if right { &mut pair.1 } else { &mut pair.0 }
    }
}

/// Open a staged editor. Closing drops changes; Apply calls `applied` only
/// after the writer commits the relations and their derived display counts.
pub(crate) fn open(
    model: Relationships,
    slot: &Rc<RefCell<Option<TagRelationshipsWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<TagRelationshipsWindow, slint::PlatformError> {
    let window = TagRelationshipsWindow::new()?;
    window.set_use_listbook(model.use_listbook());
    window.set_siblings(model.kind() == hydrus_store::display::RelationKind::Siblings);
    window.set_service_names(ModelRc::new(VecModel::from(
        model
            .service_names()
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>(),
    )));
    let location = model
        .store()
        .read(hydrus_store::settings::get::<hydrus_store::settings::SearchDefaults>)
        .unwrap_or_default()
        .local_location;
    let inputs = (0..model.service_names().len())
        .map(|i| {
            let key = model.service_key(i).expect("loaded editable service");
            (
                WriteAutocomplete::new(model.store().clone(), key.clone(), location.clone()),
                WriteAutocomplete::new(model.store().clone(), key, location.clone()),
            )
        })
        .collect();
    let active = Rc::new(Cell::new(true));
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
            let mut binding = binding.borrow_mut();
            binding.sync_context();
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
            let (left_input, right_input) = &binding.inputs[model.service()];
            let (file_label, tag_label) = left_input.domain_labels();
            window.set_left_file_label(file_label.into());
            window.set_left_tag_label(tag_label.into());
            let (file_label, tag_label) = right_input.domain_labels();
            window.set_right_file_label(file_label.into());
            window.set_right_tag_label(tag_label.into());
            let suggestions = |input: &WriteAutocomplete| {
                ModelRc::new(VecModel::from(
                    input
                        .rows()
                        .iter()
                        .map(|r| crate::list_text(&r.label, colours.tag(&r.colour_tag)))
                        .collect::<Vec<_>>(),
                ))
            };
            window.set_left_tab(i32::try_from(left_input.tab().index()).unwrap_or(0));
            window.set_right_tab(i32::try_from(right_input.tab().index()).unwrap_or(0));
            window.set_left_suggestions(suggestions(left_input));
            window.set_right_suggestions(suggestions(right_input));
            window.set_left_selected(ModelRc::new(VecModel::from(left_input.selection_mask())));
            window.set_right_selected(ModelRc::new(VecModel::from(right_input.selection_mask())));
            window.set_left_highlighted(
                left_input
                    .highlighted()
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1),
            );
            window.set_right_highlighted(
                right_input
                    .highlighted()
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1),
            );
            window.set_left_input(left_input.text().into());
            window.set_right_input(right_input.text().into());
            window.set_autocomplete_height(
                i32::try_from(left_input.options().autocomplete_list_height.clamp(1, 128))
                    .unwrap_or(11),
            );
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
    let menu_target = Rc::new(Cell::new((0usize, false)));
    let tag_menu = crate::write_tag_menu::TagMenu::new(
        binding.borrow().model.store().clone(),
        Rc::new({
            let active = active.clone();
            let binding = binding.clone();
            move || active.get() && binding.borrow().operation.is_none()
        }),
        Rc::new({
            let binding = binding.clone();
            let target = menu_target.clone();
            move |action| {
                if let hydrus_gui_model::write_tag_menu::Action::Decorate { tab, kind, value } =
                    action
                {
                    let (service, right) = target.get();
                    let mut binding = binding.borrow_mut();
                    let pair = &mut binding.inputs[service];
                    let input = if right { &mut pair.1 } else { &mut pair.0 };
                    input.decorate(tab, kind, value);
                } else if let hydrus_gui_model::write_tag_menu::Action::Domain(choice) = action {
                    let (service, right) = target.get();
                    let mut binding = binding.borrow_mut();
                    let pair = &mut binding.inputs[service];
                    let input = if right { &mut pair.1 } else { &mut pair.0 };
                    input.choose_domain(choice);
                }
            }
        }),
        Rc::new({
            let binding = binding.clone();
            let refresh = refresh.clone();
            move || {
                {
                    let mut binding = binding.borrow_mut();
                    let service = binding.model.service();
                    let pair = &mut binding.inputs[service];
                    pair.0.fetch();
                    pair.1.fetch();
                }
                refresh();
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |question| {
                if let Some(w) = weak.upgrade() {
                    w.set_tag_menu_question(question.into());
                }
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |error| {
                if let Some(w) = weak.upgrade() {
                    w.set_error(error.into());
                }
            }
        }),
    );
    crate::write_tag_menu::bind!(window, tag_menu);
    window.on_context_menu({
        let tag_menu = tag_menu.clone();
        let binding = binding.clone();
        let target = menu_target.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        move |right, i, x, y| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let mut binding = binding.borrow_mut();
                target.set((binding.model.service(), right));
                binding.input_mut(right).click(i, false, false);
                let entries = binding.input_mut(right).menu(i);
                drop(binding);
                refresh();
                tag_menu.open(&entries, x, y);
            }
        }
    });
    window.on_domain_menu({
        let menu = tag_menu.clone();
        let binding = binding.clone();
        let target = menu_target.clone();
        move |right, tags, x, y| {
            let mut binding = binding.borrow_mut();
            target.set((binding.model.service(), right));
            let entries = binding.input_mut(right).domain_menu(tags);
            drop(binding);
            menu.open(&entries, x, y);
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = slot.clone();
        let active = active.clone();
        let binding = binding.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            binding.borrow_mut().operation = None;
            tag_menu.close();
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
            let declined_paste = match &op {
                Operation::Paste { right, .. } if answers.first().is_some_and(Option::is_none) => {
                    Some(*right)
                }
                _ => None,
            };
            let result = match &op {
                Operation::Paste {
                    right,
                    tags,
                    message,
                } => {
                    if answers.is_empty() {
                        Err(Question {
                            message: message.clone(),
                            yes: "yes".into(),
                            no: "no".into(),
                            reason: false,
                        })
                    } else {
                        if answers[0].is_some() {
                            if let Err(e) = binding.model.paste_tags(*right, tags) {
                                window.set_error(e.into());
                            }
                        }
                        Ok(())
                    }
                }
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
            if let Some(right) = declined_paste {
                window.invoke_normal_paste(right);
            }
            refresh();
        }
    });
    let start = {
        let binding = binding.clone();
        let weak = window.as_weak();
        let run = run.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |op| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
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
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |yes| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_none() {
                return;
            }
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
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |i| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let mut b = binding.borrow_mut();
            let previous = b.model.service();
            b.inputs[previous].0.set_text(w.get_left_input().as_str());
            b.inputs[previous].1.set_text(w.get_right_input().as_str());
            if let Err(e) = b
                .model
                .choose_service_remembered(usize::try_from(i).unwrap_or(usize::MAX))
            {
                w.set_error(format!("could not remember tag service: {e}").into());
            }
            drop(b);
            refresh();
        }
    });
    window.on_enter_tags({
        let binding = binding.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |right, text| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let mut b = binding.borrow_mut();
            if b.input_mut(right).text() != text.as_str() {
                b.input_mut(right).set_text(&text);
            }
            let chosen = b.input_mut(right).chosen_tags(None);
            if !chosen.is_empty() {
                let chosen = chosen.join("\n");
                match b.model.enter_tags(right, &chosen) {
                    Ok(()) => {
                        w.set_error("".into());
                        b.input_mut(right).clear();
                    }
                    Err(e) => w.set_error(e.into()),
                }
            }
            drop(b);
            refresh();
        }
    });
    window.on_autocomplete_edited({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |right, text| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            binding.borrow_mut().input_mut(right).set_text(&text);
            refresh();
        }
    });
    window.on_autocomplete_tab({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |right, i| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            binding.borrow_mut().input_mut(right).set_tab(
                hydrus_gui_model::write_autocomplete::Tab::from_index(
                    usize::try_from(i).unwrap_or(0),
                ),
            );
            refresh();
        }
    });
    window.on_autocomplete_fetch({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |right| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            binding.borrow_mut().input_mut(right).fetch();
            refresh();
        }
    });
    window.on_autocomplete_move({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |right, by| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            binding
                .borrow_mut()
                .input_mut(right)
                .move_highlight(by as isize);
            refresh();
        }
    });
    window.on_autocomplete_clicked({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |right, i, ctrl, shift| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                binding.borrow_mut().input_mut(right).click(i, ctrl, shift);
                refresh();
            }
        }
    });
    window.on_autocomplete_chosen({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        let weak = window.as_weak();
        move |right, i| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            let mut b = binding.borrow_mut();
            let tags = b.input_mut(right).chosen_tags(usize::try_from(i).ok());
            if !tags.is_empty() {
                let tag = tags.join("\n");
                if let Err(e) = b.model.enter_tags(right, &tag)
                    && let Some(w) = weak.upgrade()
                {
                    w.set_error(e.into());
                }
                b.input_mut(right).clear();
            }
            drop(b);
            refresh();
        }
    });
    window.on_autocomplete_paste({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        let start = start.clone();
        let weak = window.as_weak();
        move |right, button| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return true;
            }
            let text = match crate::from_clipboard() {
                Ok(text) => text,
                Err(e) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_error(e.into());
                    }
                    return true;
                }
            };
            let options = binding.borrow_mut().input_mut(right).options();
            match hydrus_gui_model::write_autocomplete::paste(&text, button, &options) {
                Paste::Text => false,
                Paste::Confirm { message, tags } => {
                    start(Operation::Paste {
                        right,
                        message,
                        tags,
                    });
                    true
                }
                Paste::Tags(tags) => {
                    let mut b = binding.borrow_mut();
                    if let Err(e) = b.model.paste_tags(right, &tags)
                        && let Some(w) = weak.upgrade()
                    {
                        w.set_error(e.into());
                    }
                    drop(b);
                    refresh();
                    true
                }
            }
        }
    });
    window.on_remove_input({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |right, i| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
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
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
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
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
            binding.borrow_mut().model.wipe_workspace();
            refresh();
        }
    });
    window.on_row_clicked({
        let binding = binding.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |i, ctrl, shift| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
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
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |i| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
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
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move |c, asc| {
            if !active.get() || tag_menu.busy() || binding.borrow().operation.is_some() {
                return;
            }
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
