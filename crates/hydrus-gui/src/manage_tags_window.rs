//! The manage tags window, bound to its model ([`ManageTags`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::manage_tags::ManageTags;
use crate::{ListText, ManageTagsWindow};

/// Open the window on `model`; it forgets itself from `slot` when closed,
/// and calls `applied` once changes are written.
pub(crate) fn open(
    model: ManageTags,
    slot: &Rc<RefCell<Option<ManageTagsWindow>>>,
    incremental_slot: &crate::incremental_tagging_window::Slot,
    applied: Rc<dyn Fn()>,
) -> Result<ManageTagsWindow, slint::PlatformError> {
    let window = ManageTagsWindow::new()?;
    window
        .global::<crate::TagTextHistory<'_>>()
        .on_record(crate::write_tag_history::record);
    window
        .global::<crate::TagTextHistory<'_>>()
        .on_undo(crate::write_tag_history::undo);
    window
        .global::<crate::TagTextHistory<'_>>()
        .on_redo(crate::write_tag_history::redo);
    window.set_use_listbook(model.dialog_preferences().use_listbook);
    let names: Vec<SharedString> = model
        .service_names()
        .iter()
        .map(|n| n.as_str().into())
        .collect();
    window.set_service_names(ModelRc::new(VecModel::from(names)));
    window.set_incremental_available(model.files().len() > 1);
    let incremental_open = Rc::new(Cell::new(false));
    let model = Rc::new(RefCell::new(model));
    let refresh = {
        let model = model.clone();
        let weak = window.as_weak();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let model = model.borrow();
            let (file, tags) = model.write_input().domain_labels();
            window.set_file_label(file.into());
            window.set_tag_label(tags.into());
            let colours: hydrus_core::tag_presentation::NamespaceColours = model
                .store()
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            window.set_autocomplete_height(
                i32::try_from(
                    model
                        .autocomplete_options()
                        .autocomplete_list_height
                        .clamp(1, 128),
                )
                .unwrap_or(11),
            );
            window.set_deleted_count_label(model.deleted_count_label().into());
            window.set_deleted_count_visible(model.deleted_count() > 0);
            window.set_show_deleted(model.show_deleted());
            window.set_service_index(i32::try_from(model.service()).unwrap_or(0));
            let sort = model.sort_control().value;
            window.set_sort_type(
                i32::try_from(
                    hydrus_gui_model::options::TAG_SORT_TYPES
                        .iter()
                        .position(|(_, kind)| *kind == sort.order.sort_type)
                        .unwrap_or(0),
                )
                .unwrap_or(0),
            );
            window.set_sort_order(
                i32::try_from(hydrus_gui_model::options::tag_sort_orders(&sort.order).1)
                    .unwrap_or(0),
            );
            window.set_sort_group(
                i32::try_from(
                    hydrus_gui_model::options::TAG_SORT_GROUPS
                        .iter()
                        .position(|(_, group)| *group == sort.order.group_by)
                        .unwrap_or(0),
                )
                .unwrap_or(0),
            );
            window.set_sort_siblings(i32::from(!sort.use_siblings));
            let tags: Vec<ListText> = model
                .display_rows()
                .iter()
                .map(|row| {
                    crate::styled_list_text(&row.label, colours.tag(&row.colour_tag), &row.parts)
                })
                .collect();
            window.set_tags(ModelRc::new(VecModel::from(tags)));
            window
                .set_autocomplete_tab(i32::try_from(model.autocomplete_tab().index()).unwrap_or(0));
            let suggestions: Vec<ListText> = model
                .suggestion_rows()
                .iter()
                .map(|r| crate::styled_list_text(&r.label, colours.tag(&r.colour_tag), &r.parts))
                .collect();
            window.set_suggestions(ModelRc::new(VecModel::from(suggestions)));
            window.set_suggestion_selected(ModelRc::new(VecModel::from(
                model.write_input().selection_mask(),
            )));
            window.set_highlighted(
                model
                    .highlighted()
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1),
            );
            if window.get_text().as_str() != model.text() {
                window.set_text(model.text().into());
            }
        }
    };
    let active = Rc::new(Cell::new(true));
    crate::gui_colours::bind(
        window.global::<crate::Theme<'_>>(),
        model.borrow().store(),
        active.clone(),
    );
    let side_lists = Rc::new(RefCell::new([
        hydrus_gui_model::tag_suggestions::List::default(),
        hydrus_gui_model::tag_suggestions::List::default(),
        hydrus_gui_model::tag_suggestions::List::default(),
    ]));
    let prefs = model.borrow().suggestion_preferences().clone();
    window.set_suggested_columns(prefs.columns);
    window.set_suggested_width(prefs.width.clamp(20, 65535) as f32);
    window.set_recent_tags_enabled(prefs.recent_limit.is_some());
    window.set_suggested_page(i32::from(
        prefs.default_page == "recent" && prefs.recent_limit.is_some(),
    ));
    let side_services: hydrus_store::settings::TagAutocompleteTabs = model
        .borrow()
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let side_services: std::collections::BTreeSet<_> = side_services
        .most_used
        .into_iter()
        .filter_map(|(key, tags)| (!tags.is_empty()).then_some(key))
        .collect();
    let related_worker = match crate::related_tags_worker::Worker::new(model.borrow().store()) {
        Ok(worker) => Some(Rc::new(worker)),
        Err(error) => {
            window
                .set_related_status(format!("could not start related-tag search: {error}").into());
            None
        }
    };
    let related_settings: hydrus_store::related_tags::Settings = model
        .borrow()
        .store()
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    window.set_related_tags_enabled(related_settings.enabled);
    if prefs.default_page == "related" && related_settings.enabled {
        window.set_suggested_page(2);
    }
    let related_request = Rc::new(RefCell::new(None::<hydrus_store::related_tags::Query>));
    let related_results = Rc::new(RefCell::new(
        Vec::<hydrus_store::related_tags::Suggestion>::new(),
    ));
    let side_pages = Rc::new(RefCell::new(std::collections::BTreeMap::<
        hydrus_core::ServiceKey,
        i32,
    >::new()));
    let shown_side_service = Rc::new(RefCell::new(None::<hydrus_core::ServiceKey>));
    let default_side = window.get_suggested_page();
    let refresh_sides = Rc::new({
        let model = model.clone();
        let weak = window.as_weak();
        let lists = side_lists.clone();
        let active = active.clone();
        let worker = related_worker.clone();
        let last = related_request.clone();
        let results = related_results.clone();
        let pages = side_pages.clone();
        let shown = shown_side_service.clone();
        move || {
            if !active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let m = model.borrow();
            let key = m.migration_service_key();
            let enabled = key
                .as_ref()
                .is_some_and(|key| side_services.contains(&key.to_hex()));
            w.set_most_used_enabled(enabled);
            if *shown.borrow() != key {
                if let Some(previous) = shown.borrow().as_ref() {
                    pages
                        .borrow_mut()
                        .insert(previous.clone(), w.get_suggested_page());
                }
                w.set_suggested_page(
                    key.as_ref()
                        .and_then(|key| pages.borrow().get(key).copied())
                        .unwrap_or(default_side),
                );
                shown.borrow_mut().clone_from(&key);
            }
            let valid = match w.get_suggested_page() {
                0 => enabled,
                1 => w.get_recent_tags_enabled(),
                2 => w.get_related_tags_enabled(),
                _ => false,
            };
            if !valid {
                w.set_suggested_page(if enabled {
                    0
                } else if w.get_related_tags_enabled() {
                    2
                } else {
                    1
                });
            }
            if w.get_related_tags_enabled()
                && let Some(worker) = &worker
            {
                match m.related_query(w.get_related_local(), w.get_related_display()) {
                    Ok(query) => {
                        if last.borrow().as_ref() != Some(&query) {
                            worker.request(query.clone());
                            *last.borrow_mut() = Some(query);
                            results.borrow_mut().clear();
                            w.set_related_status("searching…".into());
                        }
                        if let Some(result) = worker.poll() {
                            match result {
                                Ok(rows) => {
                                    w.set_related_status(
                                        if rows.is_empty() {
                                            "no related tags found!"
                                        } else {
                                            "ready"
                                        }
                                        .into(),
                                    );
                                    *results.borrow_mut() = rows;
                                }
                                Err(error) => {
                                    results.borrow_mut().clear();
                                    w.set_related_status(error.into());
                                }
                            }
                        }
                    }
                    Err(error) => {
                        w.set_related_status(error.to_string().into());
                    }
                }
            }
            let mut lists = lists.borrow_mut();
            for (i, list) in lists.iter_mut().enumerate().take(2) {
                list.update(key.clone(), m.side_suggestions(i == 1));
            }
            lists[2].update(
                key.clone(),
                m.useful_related(results.borrow().iter().map(|row| row.tag.clone()).collect()),
            );
            let presentation: hydrus_core::tag_presentation::TagPresentation = m
                .store()
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            let rows = |list: &hydrus_gui_model::tag_suggestions::List| {
                ModelRc::new(VecModel::from(
                    list.tags
                        .iter()
                        .zip(list.mask())
                        .map(|(tag, selected)| crate::TableRow {
                            cells: ModelRc::new(VecModel::from(vec![SharedString::from(
                                presentation.render(tag),
                            )])),
                            selected,
                        })
                        .collect::<Vec<_>>(),
                ))
            };
            w.set_most_used_rows(rows(&lists[0]));
            w.set_recent_tag_rows(rows(&lists[1]));
            w.set_related_tag_rows(ModelRc::new(VecModel::from(
                lists[2]
                    .tags
                    .iter()
                    .zip(lists[2].mask())
                    .map(|(tag, selected)| crate::TableRow {
                        cells: ModelRc::new(VecModel::from(vec![SharedString::from(format!(
                            "{} ({})",
                            presentation.render(tag),
                            hydrus_core::numbers::human_int(
                                results
                                    .borrow()
                                    .iter()
                                    .find(|r| &r.tag == tag)
                                    .map_or(0, |r| r.score)
                            )
                        ))])),
                        selected,
                    })
                    .collect::<Vec<_>>(),
            )));
        }
    });
    window.on_related_search({
        let active = active.clone();
        let last = related_request.clone();
        let refresh = refresh_sides.clone();
        let incremental = incremental_open.clone();
        move || {
            if active.get() && !incremental.get() {
                last.borrow_mut().take();
                refresh();
            }
        }
    });
    let base_refresh = refresh.clone();
    let refresh = {
        let sides = refresh_sides.clone();
        move || {
            base_refresh();
            sides();
        }
    };
    let side_timer = Rc::new(slint::Timer::default());
    side_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(200),
        {
            let sides = refresh_sides.clone();
            move || sides()
        },
    );
    let pending_paste: Rc<RefCell<Option<Vec<String>>>> = Rc::default();
    let tag_menu = crate::write_tag_menu::TagMenu::new(
        model.borrow().store().clone(),
        Rc::new({
            let active = active.clone();
            let incremental_open = incremental_open.clone();
            let pending = pending_paste.clone();
            move || active.get() && !incremental_open.get() && pending.borrow().is_none()
        }),
        Rc::new({
            let model = model.clone();
            move |action| {
                if let hydrus_gui_model::write_tag_menu::Action::Decorate { tab, kind, value } =
                    action
                {
                    model
                        .borrow_mut()
                        .write_input_mut()
                        .decorate(tab, kind, value);
                } else if let hydrus_gui_model::write_tag_menu::Action::Domain(choice) = action {
                    model.borrow_mut().write_input_mut().choose_domain(choice);
                }
            }
        }),
        Rc::new({
            let model = model.clone();
            let refresh = refresh.clone();
            move || {
                model.borrow_mut().fetch();
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
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        move |i, x, y| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let entries = {
                    let mut model = model.borrow_mut();
                    model.write_input_mut().click(i, false, false);
                    model.write_input().menu(i)
                };
                refresh();
                tag_menu.open(&entries, x, y);
            }
        }
    });
    window.on_domain_menu({
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let menu = tag_menu.clone();
        let model = model.clone();
        move |tags, x, y| {
            if !active.get() || incremental_open.get() {
                return;
            }
            let entries = model.borrow().write_input().domain_menu(tags);
            menu.open(&entries, x, y);
        }
    });
    window.on_side_clicked({
        let active = active.clone();
        let pending = pending_paste.clone();
        let menu = tag_menu.clone();
        let incremental = incremental_open.clone();
        let lists = side_lists.clone();
        let refresh = refresh_sides.clone();
        move |p, i, c, s| {
            if !active.get() || incremental.get() || pending.borrow().is_some() || menu.busy() {
                return;
            }
            if let Ok(p) = usize::try_from(p)
                && let Ok(i) = usize::try_from(i)
                && let Some(list) = lists.borrow_mut().get_mut(p)
            {
                list.click(i, c, s);
            }
            refresh();
        }
    });
    window.on_side_activated({
        let active = active.clone();
        let pending = pending_paste.clone();
        let menu = tag_menu.clone();
        let incremental = incremental_open.clone();
        let lists = side_lists.clone();
        let model = model.clone();
        let refresh = refresh.clone();
        move |p, i| {
            if !active.get() || incremental.get() || pending.borrow().is_some() || menu.busy() {
                return;
            }
            let tags = if let Ok(p) = usize::try_from(p)
                && let Ok(i) = usize::try_from(i)
                && let Some(list) = lists.borrow_mut().get_mut(p)
            {
                if list.selected().is_empty() {
                    list.click(i, false, false);
                }
                list.selected()
            } else {
                Vec::new()
            };
            model.borrow_mut().add_side_suggestions(&tags);
            refresh();
        }
    });
    window.on_sort_chosen({
        let model = model.clone();
        let weak = window.as_weak();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        let refresh = refresh.clone();
        move |part, index| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
                || !weak.upgrade().is_some_and(|window| {
                    window.window().is_visible() && window.get_question().is_empty()
                })
            {
                return;
            }
            if let (Ok(part), Ok(index)) = (usize::try_from(part), usize::try_from(index)) {
                model.borrow_mut().choose_sort(part, index);
                refresh();
            }
        }
    });
    let colour_updates = crate::tag_text::watch(
        model.borrow().store(),
        Rc::new({
            let weak = window.as_weak();
            let active = active.clone();
            let incremental_open = incremental_open.clone();
            let tag_menu = tag_menu.clone();
            move || {
                active.get()
                    && !incremental_open.get()
                    && !tag_menu.busy()
                    && weak.upgrade().is_some_and(|window| {
                        window.window().is_visible() && window.get_question().is_empty()
                    })
            }
        }),
        Rc::new({
            let model = model.clone();
            let refresh = refresh.clone();
            move || {
                model.borrow_mut().fetch();
                refresh();
            }
        }),
    );
    let preference_timer = Rc::new(slint::Timer::default());
    let close = {
        let incremental_slot = incremental_slot.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending_paste = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        let preference_timer = preference_timer.clone();
        let colour_updates = colour_updates.clone();
        let side_timer = side_timer.clone();
        let related_worker = related_worker.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            incremental_open.set(false);
            crate::incremental_tagging_window::cancel(&incremental_slot);
            preference_timer.stop();
            colour_updates.stop();
            side_timer.stop();
            if let Some(worker) = &related_worker {
                worker.close();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            tag_menu.close();
            pending_paste.borrow_mut().take();
            slot.borrow_mut().take();
        }
    };
    window.on_incremental_tags({
        let model = model.clone();
        let slot = incremental_slot.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let menu = tag_menu.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move || {
            if !active.get() || incremental_open.get() || pending.borrow().is_some() || menu.busy()
            {
                return;
            }
            let Some(editor) = model.borrow().incremental() else {
                return;
            };
            let service = model.borrow().service();
            let accepted: crate::incremental_tagging_window::Applied = Rc::new({
                let model = model.clone();
                let active = active.clone();
                move |pairs| {
                    if !active.get() {
                        return Err("The Manage Tags owner is closed.".into());
                    }
                    model.borrow_mut().apply_incremental(service, pairs)
                }
            });
            let closed: Rc<dyn Fn()> = Rc::new({
                let active = active.clone();
                let incremental_open = incremental_open.clone();
                let weak = weak.clone();
                let refresh = refresh.clone();
                move || {
                    incremental_open.set(false);
                    if active.get()
                        && let Some(window) = weak.upgrade()
                    {
                        window.set_incremental_open(false);
                        refresh();
                    }
                }
            });
            match crate::incremental_tagging_window::open(editor, &slot, accepted, closed) {
                Ok(_) => {
                    incremental_open.set(true);
                    if let Some(window) = weak.upgrade() {
                        window.set_incremental_open(true);
                    }
                }
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(error.to_string().into());
                    }
                }
            }
        }
    });
    let migration_slot = crate::tag_migration_window::Slot::default();
    window.on_migrate_tags({
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let model = model.clone();
        let slot = migration_slot.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        let applied = applied.clone();
        move || {
            if !active.get() || incremental_open.get() {
                return;
            }
            let m = model.borrow();
            let Some(key) = m.migration_service_key() else {
                if let Some(w) = weak.upgrade() {
                    w.set_error("the tag service was removed; reopen manage tags".into());
                }
                return;
            };
            let changed = Rc::new({
                let model = model.clone();
                let refresh = refresh.clone();
                let applied = applied.clone();
                move || {
                    model.borrow_mut().refresh_stored();
                    refresh();
                    applied();
                }
            });
            if let Err(e) = crate::tag_migration_window::open(
                m.store(),
                &key,
                m.files().to_vec(),
                &slot,
                changed,
            ) && let Some(w) = weak.upgrade()
            {
                w.set_error(e.into());
            }
        }
    });
    let apply = {
        let model = model.clone();
        let weak = window.as_weak();
        let close = close.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            if let Err(e) = model.borrow().apply() {
                if let Some(window) = weak.upgrade() {
                    window.set_error(format!("could not apply the changes: {e}").into());
                }
                return;
            }
            applied();
            close();
        }
    };
    window.on_flip_show_deleted({
        let model = model.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            if let Err(error) = model.borrow().flip_show_deleted()
                && let Some(window) = weak.upgrade()
            {
                window.set_error(
                    format!("could not remember the deleted-mapping display: {error}").into(),
                );
            }
            refresh();
        }
    });
    // Other owners of this store observe the global preference too. The timer
    // holds only a weak window; close stops it before a retained handle can act.
    preference_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(200),
        {
            let weak = window.as_weak();
            let model = model.clone();
            let refresh = refresh.clone();
            let active = active.clone();
            let incremental_open = incremental_open.clone();
            move || {
                if active.get()
                    && !incremental_open.get()
                    && let Some(window) = weak.upgrade()
                    && window.get_show_deleted() != model.borrow().show_deleted()
                {
                    refresh();
                }
            }
        },
    );
    window.on_service_chosen({
        let model = model.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |i| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            if let Err(error) = model
                .borrow_mut()
                .choose_service(usize::try_from(i).unwrap_or(usize::MAX))
                && let Some(window) = weak.upgrade()
            {
                window.set_error(format!("could not remember the tag service: {error}").into());
            }
            refresh();
        }
    });
    window.on_refresh_autocomplete({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            let mut model = model.borrow_mut();
            let text = model.text().to_owned();
            model.set_text(&text);
            drop(model);
            refresh();
        }
    });
    window.on_tab_chosen({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |i| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            model.borrow_mut().choose_autocomplete_tab(
                hydrus_gui_model::write_autocomplete::Tab::from_index(
                    usize::try_from(i).unwrap_or(0),
                ),
            );
            refresh();
        }
    });
    window.on_fetch({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            model.borrow_mut().fetch();
            refresh();
        }
    });
    window.on_text_edited({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |text| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            model.borrow_mut().set_text(&text);
            refresh();
        }
    });
    window.on_move_highlight({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |by| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            model.borrow_mut().move_highlight(by as isize);
            refresh();
        }
    });
    window.on_navigate({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |direction, ctrl, shift| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            model
                .borrow_mut()
                .write_input_mut()
                .navigate(direction, ctrl, shift);
            refresh();
        }
    });
    window.on_results_action({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |action| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return false;
            }
            let handled =
                crate::write_tag_menu::results_action(model.borrow_mut().write_input_mut(), action);
            refresh();
            handled
        }
    });
    window.on_selection_clicked({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |i, ctrl, shift| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                model.borrow_mut().write_input_mut().click(i, ctrl, shift);
                refresh();
            }
        }
    });
    window.on_entered({
        let model = model.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        let apply = apply.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move || {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            // (enter with nothing typed applies, as in the reference)
            let empty = {
                let model = model.borrow();
                model.text().trim().is_empty()
                    && model.autocomplete_tab() == hydrus_gui_model::write_autocomplete::Tab::Tags
            };
            if empty {
                apply();
                return;
            }
            let entered = model.borrow_mut().enter_input();
            if let Some(window) = weak.upgrade() {
                window.set_error(entered.err().unwrap_or_default().into());
            }
            refresh();
        }
    });
    window.on_tag_activated({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |i| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            model
                .borrow_mut()
                .toggle_row(usize::try_from(i).unwrap_or(usize::MAX));
            refresh();
        }
    });
    window.on_suggestion_chosen({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        move |i| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return;
            }
            let _ = model
                .borrow_mut()
                .choose_suggestion(usize::try_from(i).unwrap_or(usize::MAX));
            refresh();
        }
    });
    window.on_paste_requested({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let tag_menu = tag_menu.clone();
        let weak = window.as_weak();
        move |button| {
            if !active.get()
                || incremental_open.get()
                || pending.borrow().is_some()
                || tag_menu.busy()
            {
                return true;
            }
            let Some(w) = weak.upgrade() else {
                return true;
            };
            let text = match crate::from_clipboard() {
                Ok(text) => text,
                Err(error) => {
                    w.set_error(error.into());
                    return true;
                }
            };
            let decision = hydrus_gui_model::write_autocomplete::paste(
                &text,
                button,
                &model.borrow().autocomplete_options(),
            );
            match decision {
                hydrus_gui_model::write_autocomplete::Paste::Text => false,
                hydrus_gui_model::write_autocomplete::Paste::Confirm { message, tags } => {
                    *pending.borrow_mut() = Some(tags);
                    w.set_question(message.into());
                    true
                }
                hydrus_gui_model::write_autocomplete::Paste::Tags(tags) => {
                    // Release the options borrow before staging tags.
                    let result = model.borrow_mut().paste_tags(&tags);
                    w.set_error(result.err().unwrap_or_default().into());
                    refresh();
                    true
                }
            }
        }
    });
    window.on_paste_answered({
        let model = model.clone();
        let refresh = refresh.clone();
        let active = active.clone();
        let incremental_open = incremental_open.clone();
        let pending = pending_paste.clone();
        let weak = window.as_weak();
        move |yes| {
            if !active.get() || incremental_open.get() {
                return;
            }
            let Some(tags) = pending.borrow_mut().take() else {
                return;
            };
            if let Some(w) = weak.upgrade() {
                w.set_question("".into());
                if yes {
                    w.set_error(
                        model
                            .borrow_mut()
                            .paste_tags(&tags)
                            .err()
                            .unwrap_or_default()
                            .into(),
                    );
                } else {
                    w.invoke_normal_paste();
                }
            }
            refresh();
        }
    });
    window.on_apply(apply);
    window.on_cancel(close.clone());
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    window.show()?;
    Ok(window)
}
