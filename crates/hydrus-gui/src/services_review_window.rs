//! The local service review window with live native statistics and key copying.
use crate::ServicesReviewWindow;
use hydrus_gui_model::services_review::{self, Action, Row};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

fn show(window: &ServicesReviewWindow, rows: &[Row], index: usize) {
    if let Some(row) = rows.get(index) {
        window
            .set_client_api(row.service_type == hydrus_core::ServiceType::ClientApiService.name());
        window.set_tag_service(matches!(
            row.service_type.as_str(),
            "local tag domain" | "hydrus tag repository"
        ));
        window.set_trash_service(
            row.service_type == hydrus_core::ServiceType::LocalFileTrashDomain.name(),
        );
        window.set_physical_storage(row.actions.contains(&Action::ClearDeletedRecords));
        window.set_trash_nonempty(row.actions.contains(&Action::ClearTrash));
        window.set_rating_service(
            row.actions
                .iter()
                .any(|a| matches!(a, Action::ClearRatings(_))),
        );
        window.set_selected(i32::try_from(index).unwrap_or(0));
        window.set_name_and_type(format!("{} - {}", row.name, row.service_type).into());
        window.set_statistics(row.statistics.as_str().into());
        window.set_unavailable(row.unavailable.as_str().into());
        window.set_database_id(SharedString::new());
    }
}

/// Open service review over a native store; refresh retains the selected service key.
pub fn open(store: Arc<Store>) -> Result<ServicesReviewWindow, String> {
    open_with_changed(store, Rc::new(|| {}))
}

/// Open service review and refresh tag consumers after migrations commit.
pub fn open_with_changed(
    store: Arc<Store>,
    changed: Rc<dyn Fn()>,
) -> Result<ServicesReviewWindow, String> {
    let window = crate::app_title::new::<crate::ServicesReviewWindow>().map_err(|e| e.to_string())?;
    let active = Rc::new(Cell::new(true));
    let pending = Rc::new(RefCell::new(
        None::<(hydrus_core::ServiceKey, Action, bool)>,
    ));
    let content_changed = changed.clone();
    let migration_slot = crate::tag_migration_window::Slot::default();
    let slots = crate::client_api_admin_window::Slots::default();
    let rows = Rc::new(RefCell::new(
        services_review::rows(&store).map_err(|e| e.to_string())?,
    ));
    let names = |rows: &[Row]| {
        ModelRc::new(VecModel::from(
            rows.iter()
                .map(|r| SharedString::from(format!("{}: {}", r.service_type, r.name)))
                .collect::<Vec<_>>(),
        ))
    };
    window.set_services(names(&rows.borrow()));
    show(&window, &rows.borrow(), 0);
    window.on_selected_service({
        let rows = rows.clone();
        let weak = window.as_weak();
        let pending = pending.clone();
        move |i| {
            if let (Some(w), Ok(i)) = (weak.upgrade(), usize::try_from(i)) {
                if let Some((key, _, _)) = pending.borrow().as_ref() {
                    if let Some(index) = rows.borrow().iter().position(|r| &r.key == key) {
                        w.set_selected(i32::try_from(index).unwrap_or(0));
                    }
                    return;
                }
                w.set_rating_menu(false);
                show(&w, &rows.borrow(), i);
            }
        }
    });
    window.on_refresh_clicked({
        let rows = rows.clone();
        let weak = window.as_weak();
        let store = store.clone();
        move || {
            let Some(w) = weak.upgrade() else { return };
            if !w.get_question().is_empty() {
                return;
            }
            let key = usize::try_from(w.get_selected())
                .ok()
                .and_then(|i| rows.borrow().get(i).map(|r| r.key.clone()));
            match services_review::rows(&store) {
                Ok(fresh) => {
                    let selected = fresh
                        .iter()
                        .position(|r| Some(&r.key) == key.as_ref())
                        .unwrap_or(0);
                    w.set_services(names(&fresh));
                    *rows.borrow_mut() = fresh;
                    show(&w, &rows.borrow(), selected);
                    w.set_error(SharedString::new());
                }
                Err(e) => w.set_error(e.to_string().into()),
            }
        }
    });
    window.on_copy_key({
        let rows = rows.clone();
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade()
                && let Ok(i) = usize::try_from(w.get_selected())
                && let Some(row) = rows.borrow().get(i)
            {
                crate::copy_to_clipboard(&row.key.to_hex());
            }
        }
    });
    window.on_show_id({
        let rows = rows.clone();
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade()
                && let Ok(i) = usize::try_from(w.get_selected())
                && let Some(row) = rows.borrow().get(i)
            {
                w.set_database_id(format!("service id: {}", row.id).into());
            }
        }
    });
    window.on_migrate_tags({
        let weak = window.as_weak();
        let rows = rows.clone();
        let store = store.clone();
        let slot = migration_slot.clone();
        move || {
            if let Some(w) = weak.upgrade()
                && let Ok(i) = usize::try_from(w.get_selected())
                && let Some(row) = rows.borrow().get(i)
            {
                let changed = Rc::new({
                    let weak = weak.clone();
                    let changed = changed.clone();
                    move || {
                        if let Some(w) = weak.upgrade() {
                            w.invoke_refresh_clicked();
                        }
                        changed();
                    }
                });
                if let Err(e) =
                    crate::tag_migration_window::open(&store, &row.key, vec![], &slot, changed)
                {
                    w.set_error(e.into());
                }
            }
        }
    });
    window.on_rating_menu_open({
        let weak = window.as_weak();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(w) = weak.upgrade()
                && w.window().is_visible()
                && w.get_question().is_empty()
                && w.get_rating_service()
            {
                w.set_rating_menu(!w.get_rating_menu());
            }
        }
    });
    window.on_maintenance({
        let weak = window.as_weak();
        let rows = rows.clone();
        let active = active.clone();
        let pending = pending.clone();
        move |index| {
            if !active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() || pending.borrow().is_some() {
                return;
            }
            let Some(action) = Action::from_index(index) else {
                return;
            };
            let selected = usize::try_from(w.get_selected()).unwrap_or(usize::MAX);
            let rows = rows.borrow();
            let Some(row) = rows.get(selected) else {
                return;
            };
            if !row.actions.contains(&action) {
                return;
            }
            *pending.borrow_mut() = Some((row.key.clone(), action, false));
            w.set_rating_menu(false);
            w.set_yes_label("do it".into());
            w.set_no_label("forget it".into());
            w.set_question(action.question().into());
        }
    });
    window.on_answer({
        let weak = window.as_weak();
        let store = store.clone();
        let active = active.clone();
        let pending = pending.clone();
        move |accepted| {
            if !active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.window().is_visible() {
                return;
            }
            let Some((key, action, second)) = pending.borrow_mut().take() else {
                return;
            };
            if accepted
                && !second
                && let Some(question) = action.second_question()
            {
                *pending.borrow_mut() = Some((key, action, true));
                w.set_question(question.into());
                w.set_yes_label("yes, I am".into());
                w.set_no_label("no, I am not sure".into());
                return;
            }
            w.set_question(SharedString::new());
            if accepted {
                match services_review::apply(&store, key, action) {
                    Ok(()) => {
                        content_changed();
                    }
                    Err(error) => {
                        w.set_error(error.to_string().into());
                        return;
                    }
                }
            }
            w.invoke_refresh_clicked();
        }
    });
    window.on_manage_api_keys({
        let weak = window.as_weak();
        let rows = rows.clone();
        let slots = slots.clone();
        move || {
            if let Some(w) = weak.upgrade()
                && w.get_client_api()
                && let Ok(i) = usize::try_from(w.get_selected())
                && let Some(row) = rows.borrow().get(i)
                && let Err(e) =
                    crate::client_api_admin_window::open(store.clone(), row.key.clone(), &slots)
            {
                w.set_error(e.into());
            }
        }
    });
    window.on_close_clicked({
        let active = active.clone();
        let pending = pending.clone();
        let slots = slots.clone();
        let weak = window.as_weak();
        move || {
            active.set(false);
            pending.borrow_mut().take();
            if let Some(w) = weak.upgrade() {
                w.set_question(SharedString::new());
                w.set_rating_menu(false);
                slots.close();
                let _ = w.hide();
            }
        }
    });
    let weak = window.as_weak();
    window.window().on_close_requested(move || {
        active.set(false);
        pending.borrow_mut().take();
        if let Some(window) = weak.upgrade() {
            window.set_question(SharedString::new());
            window.set_rating_menu(false);
        }
        slots.close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
