//! Service migration controls, confirmations and cancellable background work.
use crate::TagMigrationWindow;
use hydrus_core::{HashId, HashKind, ServiceKey};
use hydrus_gui_model::tag_migration::{self as model, Action, Content, Migration};
mod progress;
use hydrus_store::Store;
pub use progress::last_opened as last_progress;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc, sync::Arc};
/// Retains a migration window while its parent stays open.
pub type Slot = Rc<RefCell<Option<TagMigrationWindow>>>;
thread_local! {static LAST: RefCell<Option<slint::Weak<TagMigrationWindow>>>=const {RefCell::new(None)};}
/// Latest migration window for integration regression tests.
pub fn last_opened() -> Option<TagMigrationWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
fn strings(labels: impl IntoIterator<Item = impl Into<SharedString>>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        labels.into_iter().map(Into::into).collect::<Vec<_>>(),
    ))
}
fn show(window: &TagMigrationWindow, model: &Migration) {
    window.set_services(strings(model.endpoint_labels()));
    window.set_count_services(strings(model.services.iter().map(|s| s.name.clone())));
    window.set_count_service(i32::try_from(model.count_service).unwrap_or(0));
    window.set_source_archive(model.source == model.services.len());
    window.set_destination_archive(model.destination == model.services.len());
    window.set_source_path(model.archive_path_label(true).into());
    window.set_destination_path(model.archive_path_label(false).into());
    window.set_hash_locked(model.destination_hash_locked);
    window.set_hash_kind(match model.archives.hash_kind {
        HashKind::Sha256 => 0,
        HashKind::Md5 => 1,
        HashKind::Sha1 => 2,
        HashKind::Sha512 => 3,
    });
    let source_hash = model
        .archives
        .source
        .as_ref()
        .and_then(|p| hydrus_store::tag_migration::archive::inspect(p, model.content).ok())
        .and_then(|m| {
            if let hydrus_store::tag_migration::archive::Metadata::Mappings(k) = m {
                Some(model::hash_label(k))
            } else {
                None
            }
        })
        .unwrap_or("unknown");
    window.set_source_hash(format!("hash type: {source_hash}").into());

    window.set_source(i32::try_from(model.source).unwrap_or(0));
    window.set_destination(i32::try_from(model.destination).unwrap_or(0));
    window.set_statuses(strings(
        model.statuses().into_iter().map(model::status_label),
    ));
    window.set_status(
        i32::try_from(
            model
                .statuses()
                .iter()
                .position(|s| s == &model.status)
                .unwrap_or(0),
        )
        .unwrap_or(0),
    );
    window.set_actions(strings(
        model.actions().into_iter().map(model::action_label),
    ));
    window.set_action(
        i32::try_from(
            model
                .actions()
                .iter()
                .position(|a| a == &model.action)
                .unwrap_or(0),
        )
        .unwrap_or(0),
    );
    window.set_petition(model.action == Action::Petition);
    window.set_left_label(
        format!(
            "{}: {}",
            if model.content == Content::Mappings {
                "tags taken"
            } else {
                "left"
            },
            model.left_filter.to_filter_string()
        )
        .into(),
    );
    window.set_right_label(format!("right: {}", model.right_filter.to_filter_string()).into());
    let tooltip = |filter, prefix| {
        hydrus_gui_model::tag_filter_editor::button_label(filter, false, prefix, true).1
    };
    window.set_left_tooltip(
        tooltip(
            &model.left_filter,
            if model.content == Content::Mappings {
                "tags taken: "
            } else {
                "left: "
            },
        )
        .into(),
    );
    window.set_right_tooltip(tooltip(&model.right_filter, "right: ").into());
    window.set_location_label(format!("file domain: {}", model.location_description()).into());
}
/// Open with an optional selected-file scope; notify consumers after committed work.
pub fn open(
    store: &Arc<Store>,
    key: &ServiceKey,
    files: Vec<HashId>,
    slot: &Slot,
    changed: Rc<dyn Fn()>,
) -> Result<TagMigrationWindow, String> {
    if let Some(window) = slot.borrow().as_ref() {
        window.show().map_err(|e| e.to_string())?;
        return Ok(window.clone_strong());
    }
    let settings = Rc::new(RefCell::new(
        Migration::new(store, key, files).map_err(|e| e.to_string())?,
    ));
    let window = TagMigrationWindow::new().map_err(|e| e.to_string())?;
    window.set_services(strings(
        settings.borrow().services.iter().map(|s| s.name.clone()),
    ));
    window.set_have_files(!settings.borrow().files.is_empty());
    window.set_selected_files(settings.borrow().selected_files);
    window.set_file_label(format!("{} selected files", settings.borrow().files.len()).into());
    show(&window, &settings.borrow());
    let confirmed: Rc<RefCell<Option<(model::Request, model::Options, String)>>> = Rc::default();
    let job_window: Rc<RefCell<Option<slint::Weak<crate::TagMigrationProgressWindow>>>> =
        Rc::default();
    let filter_slot = crate::tag_filter_window::Slot::default();
    let location_slot: Rc<RefCell<Option<crate::LocationsWindow>>> = Rc::default();
    window.on_choices_changed({
        let settings = settings.clone();
        let weak = window.as_weak();
        move || {
            let Some(w) = weak.upgrade() else { return };
            if w.get_running() || !w.get_question().is_empty() {
                return;
            }
            let mut m = settings.borrow_mut();
            let old_statuses = m.statuses();
            let old_actions = m.actions();
            m.status = usize::try_from(w.get_status())
                .ok()
                .and_then(|i| old_statuses.get(i).copied())
                .unwrap_or(m.status);
            m.action = usize::try_from(w.get_action())
                .ok()
                .and_then(|i| old_actions.get(i).copied())
                .unwrap_or(m.action);
            let old_source = m.source;
            m.source = usize::try_from(w.get_source()).unwrap_or(0);
            m.destination = usize::try_from(w.get_destination()).unwrap_or(0);
            let old_content = m.content;
            m.content = match w.get_content() {
                1 => Content::Siblings,
                2 => Content::Parents,
                _ => Content::Mappings,
            };
            if old_content != m.content {
                m.reset_archives();
            }
            m.count_service = usize::try_from(w.get_count_service())
                .unwrap_or(0)
                .min(m.services.len() - 1);
            if old_source != m.source && m.source < m.services.len() {
                m.count_service = m.source;
            }
            m.count_left = w.get_count_left();
            m.count_right = w.get_count_right();
            m.count_either = w.get_count_either();
            if old_content == m.content && !m.destination_hash_locked {
                m.archives.hash_kind = match w.get_hash_kind() {
                    1 => HashKind::Md5,
                    2 => HashKind::Sha1,
                    3 => HashKind::Sha512,
                    _ => HashKind::Sha256,
                };
            }
            m.selected_files = w.get_selected_files();
            m.normalize();
            show(&w, &m);
        }
    });
    window.on_edit_filter({
        let settings = settings.clone();
        let weak = window.as_weak();
        let store = store.clone();
        let filter_slot = filter_slot.clone();
        move |right| {
            let filter = if right {
                settings.borrow().right_filter.clone()
            } else {
                settings.borrow().left_filter.clone()
            };
            let chosen = Rc::new({
                let settings = settings.clone();
                let weak = weak.clone();
                move |filter| {
                    if right {
                        settings.borrow_mut().right_filter = filter;
                    } else {
                        settings.borrow_mut().left_filter = filter;
                    }
                    if let Some(w) = weak.upgrade() {
                        show(&w, &settings.borrow());
                    }
                }
            });
            if let Err(e) = crate::tag_filter_window::open(
                &store,
                &filter,
                false,
                if right {
                    "right pair filter"
                } else {
                    "tags taken / left pair filter"
                },
                "Only tags passing this filter are migrated.",
                &filter_slot,
                chosen,
            ) && let Some(w) = weak.upgrade()
            {
                w.set_error(e.to_string().into());
            }
        }
    });
    window.on_edit_location({
        let settings = settings.clone();
        let weak = window.as_weak();
        let store = store.clone();
        let location_slot = location_slot.clone();
        move || {
            let chosen = Rc::new({
                let settings = settings.clone();
                let weak = weak.clone();
                move |location| {
                    settings.borrow_mut().location = location;
                    if let Some(w) = weak.upgrade() {
                        show(&w, &settings.borrow());
                    }
                }
            });
            let location = settings.borrow().location.clone();
            if let Err(e) = crate::locations_window::open_for_autocomplete(
                &location_slot,
                store.clone(),
                &location,
                chosen,
            ) && let Some(w) = weak.upgrade()
            {
                w.set_error(e.into());
            }
        }
    });
    window.on_archive_path_chosen({
        let settings = settings.clone();
        let weak = window.as_weak();
        move |source, path| {
            let Some(w) = weak.upgrade() else { return };
            if w.get_running() || !w.get_question().is_empty() || path.is_empty() {
                return;
            }
            let mut m = settings.borrow_mut();
            match m.set_archive_path(source, std::path::Path::new(path.as_str())) {
                Ok(()) => {
                    w.set_error(SharedString::new());
                    show(&w, &m);
                }
                Err(e) => w.set_error(e.to_string().into()),
            }
        }
    });
    window.on_choose_archive({
        let weak = window.as_weak();
        move |source| {
            let Some(w) = weak.upgrade() else { return };
            if w.get_running() || !w.get_question().is_empty() {
                return;
            }
            let dialog = rfd::FileDialog::new().set_title(if source {
                model::SOURCE_ARCHIVE_PROMPT
            } else {
                model::DESTINATION_ARCHIVE_PROMPT
            });
            let path = if source {
                dialog.pick_file()
            } else {
                dialog.save_file()
            };
            if let Some(path) = path {
                w.invoke_archive_path_chosen(source, path.to_string_lossy().as_ref().into());
            }
        }
    });
    window.on_go({
        let settings = settings.clone();
        let confirmed = confirmed.clone();
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                if w.get_running() {
                    return;
                }
                settings.borrow_mut().reason = w.get_reason().to_string();
                if settings.borrow().action == Action::Petition
                    && settings.borrow().reason.trim().is_empty()
                {
                    w.set_error("a petition reason is required".into());
                    return;
                }
                let options = match settings.borrow().job_options() {
                    Ok(options) => options,
                    Err(e) => {
                        w.set_error(e.to_string().into());
                        return;
                    }
                };
                w.set_error(SharedString::new());
                w.set_last_chance(false);
                let confirmation = settings.borrow().confirmation();
                let title = confirmation
                    .split("\n\n")
                    .nth(1)
                    .unwrap_or("migrate tags")
                    .to_owned();
                w.set_question(confirmation.into());
                *confirmed.borrow_mut() = Some((settings.borrow().request(), options, title));
            }
        }
    });
    window.on_answer({
        let confirmed = confirmed.clone();
        let weak = window.as_weak();
        let store = store.clone();
        let job_window = job_window.clone();
        move |yes| {
            let Some(w) = weak.upgrade() else { return };
            if w.get_question().is_empty() || w.get_running() {
                return;
            }
            if !yes {
                confirmed.borrow_mut().take();
                w.set_question(SharedString::new());
                return;
            }
            let advanced = store
                .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
                .unwrap_or_default()
                .0;
            if !advanced && !w.get_last_chance() {
                w.set_last_chance(true);
                w.set_question(model::LAST_CHANCE.into());
                return;
            }
            let Some((request, options, title)) = confirmed.borrow_mut().take() else {
                w.set_question(SharedString::new());
                w.set_error("the migration confirmation expired; press Go again".into());
                return;
            };
            w.set_question(SharedString::new());
            w.set_running(true);
            w.set_progress("beginning work".into());
            match progress::start(
                store.clone(),
                request,
                options,
                &title,
                weak.clone(),
                changed.clone(),
            ) {
                Ok(job) => *job_window.borrow_mut() = Some(job.as_weak()),
                Err(error) => {
                    w.set_running(false);
                    w.set_error(error.into());
                }
            }
        }
    });
    window.on_pause_job({
        let job_window = job_window.clone();
        move || {
            if let Some(job) = job_window.borrow().as_ref().and_then(slint::Weak::upgrade) {
                job.invoke_pause_job();
            }
        }
    });
    window.on_cancel_job({
        let job_window = job_window.clone();
        move || {
            if let Some(job) = job_window.borrow().as_ref().and_then(slint::Weak::upgrade) {
                job.invoke_cancel_job();
            }
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            let filter = filter_slot
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(filter) = filter {
                filter.invoke_cancel();
            }
            if let Some(location) = location_slot.borrow().as_ref() {
                let _ = location.hide();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_close_clicked(close.clone());
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    *slot.borrow_mut() = Some(window.clone_strong());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
