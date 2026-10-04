//! Staged display/search and application windows, with owned nested editors.
use crate::{TableRow, TagDisplayWindow};
use hydrus_gui_model::tag_display::TagDisplayEditor;
use hydrus_store::tag_display::TagView;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(crate) fn open(
    mut model: TagDisplayEditor,
    application: bool,
    slot: &Rc<RefCell<Option<TagDisplayWindow>>>,
    applied: Rc<dyn Fn()>,
) -> Result<TagDisplayWindow, slint::PlatformError> {
    let window = TagDisplayWindow::new()?;
    window.set_application(application);
    if application {
        model.application_only();
    }
    let model = Rc::new(RefCell::new(model));
    if application {
        let i = model
            .borrow()
            .services()
            .iter()
            .position(|s| s.real)
            .unwrap_or(0);
        model.borrow_mut().choose(i);
    }
    let selected = Rc::new([Cell::new(usize::MAX), Cell::new(usize::MAX)]);
    let filter_slot = crate::tag_filter_window::Slot::default();
    let location_slot = Rc::new(RefCell::new(None::<crate::LocationsWindow>));
    let alive = Rc::new(Cell::new(true));
    let can_edit: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        let alive = alive.clone();
        move || {
            alive.get()
                && weak
                    .upgrade()
                    .is_some_and(|w| w.window().is_visible() && !w.get_child_open())
        }
    });
    let refresh: Rc<dyn Fn()> = Rc::new({
        let filter_slot = filter_slot.clone();
        let location_slot = location_slot.clone();
        let weak = window.as_weak();
        let model = model.clone();
        let selected = selected.clone();
        move || {
            let Some(w) = weak.upgrade() else { return };
            w.set_child_open(filter_slot.borrow().is_some() || location_slot.borrow().is_some());
            let m = model.borrow();
            let s = m.current();
            w.set_services(ModelRc::new(VecModel::from(
                m.services()
                    .iter()
                    .map(|s| s.name.clone().into())
                    .collect::<Vec<_>>(),
            )));
            w.set_service_index(i32::try_from(m.selected()).unwrap_or(0));
            w.set_real_service(s.real);
            w.set_single_label(s.single.to_permitted_string().into());
            w.set_selection_label(s.selection.to_permitted_string().into());
            w.set_fetch_automatically(s.autocomplete.fetch_automatically);
            w.set_threshold(i32::from(s.autocomplete.exact_match_threshold.unwrap_or(0)));
            w.set_override_location(s.autocomplete.override_location);
            w.set_write_service(
                i32::try_from(
                    m.services()
                        .iter()
                        .position(|v| v.key == s.autocomplete.write_tag_service)
                        .unwrap_or(0),
                )
                .unwrap_or(0),
            );
            let names = s
                .autocomplete
                .write_location
                .current()
                .iter()
                .chain(s.autocomplete.write_location.deleted())
                .map(|key| {
                    m.store()
                        .snapshot()
                        .services
                        .by_key(key)
                        .map_or_else(|_| "missing service".to_owned(), |s| s.name.clone())
                })
                .collect::<Vec<_>>()
                .join(", ");
            w.set_location_label(names.into());
            w.set_search_namespaces(s.rules.search_namespaces_into_full_tags);
            w.set_any_namespace(s.rules.unnamespaced_search_gives_any_namespace_wildcards);
            w.set_bare_fetch(s.rules.namespace_bare_fetch_all_allowed);
            w.set_namespace_fetch(s.rules.namespace_fetch_all_allowed);
            w.set_fetch_all(s.rules.fetch_all_allowed);
            w.set_source_services(ModelRc::new(VecModel::from(
                m.services()
                    .iter()
                    .filter(|s| s.real)
                    .map(|s| s.name.clone().into())
                    .collect::<Vec<_>>(),
            )));
            let rows = |parents: bool| {
                ModelRc::new(VecModel::from(
                    (if parents { &s.parents } else { &s.siblings })
                        .iter()
                        .enumerate()
                        .map(|(i, key)| TableRow {
                            cells: ModelRc::new(VecModel::from(vec![
                                m.services()
                                    .iter()
                                    .find(|s| &s.key == key)
                                    .map_or("missing service", |s| s.name.as_str())
                                    .into(),
                            ])),
                            selected: selected[usize::from(parents)].get() == i,
                        })
                        .collect::<Vec<_>>(),
                ))
            };
            w.set_sibling_rows(rows(false));
            w.set_parent_rows(rows(true));
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let alive = alive.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let filters = filter_slot.clone();
        let locations = location_slot.clone();
        move || {
            if !alive.replace(false) {
                return;
            }
            let filter = filters
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(w) = filter {
                w.invoke_cancel();
            }
            let location = locations
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(w) = location {
                w.invoke_cancel();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
        }
    });
    window.on_service_chosen({
        let can_edit = can_edit.clone();
        let model = model.clone();
        let refresh = refresh.clone();
        let selected = selected.clone();
        move |i| {
            if !can_edit() {
                return;
            }
            model
                .borrow_mut()
                .choose(usize::try_from(i).unwrap_or(usize::MAX));
            selected.iter().for_each(|s| s.set(usize::MAX));
            refresh();
        }
    });
    window.on_rule_changed({
        let can_edit = can_edit.clone();
        let model = model.clone();
        let refresh = refresh.clone();
        move |i, on| {
            if !can_edit() {
                return;
            }
            model
                .borrow_mut()
                .set_rule(usize::try_from(i).unwrap_or(usize::MAX), on);
            refresh();
        }
    });
    window.on_options_changed({
        let can_edit = can_edit.clone();
        let model = model.clone();
        let weak = window.as_weak();
        move || {
            if !can_edit() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            let mut m = model.borrow_mut();
            let key = m
                .services()
                .get(usize::try_from(w.get_write_service()).unwrap_or(usize::MAX))
                .map(|s| s.key.clone());
            let o = &mut m.current_mut().autocomplete;
            o.fetch_automatically = w.get_fetch_automatically();
            o.exact_match_threshold = u16::try_from(w.get_threshold()).ok().filter(|n| *n > 0);
            o.override_location = w.get_override_location();
            if let Some(key) = key {
                o.write_tag_service = key;
            }
        }
    });
    window.on_filter({
        let can_edit = can_edit.clone();
        let model = model.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        let slot = filter_slot.clone();
        move |selection| {
            if !can_edit() {
                return;
            }
            if slot.borrow().is_some() {
                return;
            }
            let m = model.borrow();
            let s = m.current();
            let key = s.key.clone();
            let view = if selection {
                TagView::SelectionList
            } else {
                TagView::SingleMedia
            };
            let callback = Rc::new({
                let model = model.clone();
                let refresh = refresh.clone();
                let weak = weak.clone();
                move |filter| {
                    if weak.upgrade().is_some_and(|w| w.window().is_visible()) {
                        model.borrow_mut().set_filter(&key, view, filter);
                        refresh();
                    }
                }
            });
            match crate::tag_filter_window::open(
                m.store(),
                if selection { &s.selection } else { &s.single },
                false,
                "tags shown",
                "Filter tags shown in this view.",
                &slot,
                callback,
            ) {
                Ok(w) => {
                    w.on_closed({
                        let refresh = refresh.clone();
                        move || refresh()
                    });
                    *slot.borrow_mut() = Some(w);
                    if let Some(w) = weak.upgrade() {
                        w.set_child_open(true);
                    }
                }
                Err(e) => eprintln!("could not open tag filter: {e}"),
            }
        }
    });
    window.on_location({
        let can_edit = can_edit.clone();
        let model = model.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        let slot = location_slot.clone();
        move || {
            if !can_edit() {
                return;
            }
            let m = model.borrow();
            let key = m.current().key.clone();
            let callback = Rc::new({
                let model = model.clone();
                let refresh = refresh.clone();
                let weak = weak.clone();
                move |location| {
                    if weak.upgrade().is_some_and(|w| w.window().is_visible()) {
                        if let Some(s) = model
                            .borrow_mut()
                            .services_mut()
                            .iter_mut()
                            .find(|s| s.key == key)
                        {
                            s.autocomplete.write_location = location;
                        }
                        refresh();
                    }
                }
            });
            if let Err(e) = crate::locations_window::open_for_autocomplete(
                &slot,
                m.store().clone(),
                &m.current().autocomplete.write_location,
                callback,
            ) {
                eprintln!("could not open location selector: {e}");
            } else if let Some(child) = slot.borrow().as_ref() {
                child.on_closed({
                    let refresh = refresh.clone();
                    move || refresh()
                });
                if let Some(w) = weak.upgrade() {
                    w.set_child_open(true);
                }
            }
        }
    });
    window.on_source_add({
        let can_edit = can_edit.clone();
        let model = model.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        move |parents| {
            if !can_edit() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            let key = model
                .borrow()
                .services()
                .iter()
                .filter(|s| s.real)
                .nth(usize::try_from(w.get_source_index()).unwrap_or(usize::MAX))
                .map(|s| s.key.clone());
            if let Some(key) = key {
                if !model.borrow_mut().add_source(parents, key) {
                    w.set_error("That service is already applied.".into());
                }
            }
            refresh();
        }
    });
    window.on_source_click({
        let can_edit = can_edit.clone();
        let selected = selected.clone();
        let refresh = refresh.clone();
        move |parents, i| {
            if !can_edit() {
                return;
            }
            selected[usize::from(parents)].set(usize::try_from(i).unwrap_or(usize::MAX));
            refresh();
        }
    });
    window.on_source_change({
        let can_edit = can_edit.clone();
        let model = model.clone();
        let selected = selected.clone();
        let refresh = refresh.clone();
        move |parents, by| {
            if !can_edit() {
                return;
            }
            model.borrow_mut().change_source(
                parents,
                selected[usize::from(parents)].get(),
                (by != 0).then_some(by as isize),
            );
            selected[usize::from(parents)].set(usize::MAX);
            refresh();
        }
    });
    window.on_apply({
        let can_edit = can_edit.clone();
        let alive = alive.clone();
        let model = model.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !can_edit() {
                return;
            }
            if !alive.get() {
                return;
            }
            match model.borrow().apply() {
                Ok(()) => {
                    applied();
                    close();
                }
                Err(e) => {
                    if let Some(w) = weak.upgrade() {
                        w.set_error(e.to_string().into());
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
