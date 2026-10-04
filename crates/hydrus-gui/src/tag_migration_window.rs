//! Service migration controls, confirmations and cancellable background work.
use crate::TagMigrationWindow;
use hydrus_core::{HashId, ServiceKey};
use hydrus_gui_model::tag_migration::{self as model, Action, Content, Migration, Progress};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
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
    window.set_location_label(format!("file domain: {}", model.location_description()).into());
}
/// Open with an optional selected-file scope; notify consumers after committed work.
pub fn open(
    store: Arc<Store>,
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
        Migration::new(&store, key, files).map_err(|e| e.to_string())?,
    ));
    let window = TagMigrationWindow::new().map_err(|e| e.to_string())?;
    window.set_services(strings(
        settings.borrow().services.iter().map(|s| s.name.clone()),
    ));
    window.set_have_files(!settings.borrow().files.is_empty());
    window.set_selected_files(settings.borrow().selected_files);
    window.set_file_label(format!("{} selected files", settings.borrow().files.len()).into());
    show(&window, &settings.borrow());
    let cancellation = Arc::new(AtomicBool::new(false));
    let confirmed: Rc<RefCell<Option<model::Request>>> = Rc::default();
    let close_requested = Rc::new(Cell::new(false));
    let timer = Rc::new(slint::Timer::default());
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
            m.source = usize::try_from(w.get_source()).unwrap_or(0);
            m.destination = usize::try_from(w.get_destination()).unwrap_or(0);
            m.content = match w.get_content() {
                1 => Content::Siblings,
                2 => Content::Parents,
                _ => Content::Mappings,
            };
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
            ) {
                if let Some(w) = weak.upgrade() {
                    w.set_error(e.to_string().into());
                }
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
            ) {
                if let Some(w) = weak.upgrade() {
                    w.set_error(e.into());
                }
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
                w.set_error(SharedString::new());
                w.set_last_chance(false);
                w.set_question(settings.borrow().confirmation().into());
                *confirmed.borrow_mut() = Some(settings.borrow().request());
            }
        }
    });
    window.on_answer({
        let confirmed = confirmed.clone();
        let weak = window.as_weak();
        let store = store.clone();
        let cancellation = cancellation.clone();
        let timer = timer.clone();
        let close_requested = close_requested.clone();
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
            let Some(request) = confirmed.borrow_mut().take() else {
                w.set_question(SharedString::new());
                w.set_error("the migration confirmation expired; press Go again".into());
                return;
            };
            w.set_question(SharedString::new());
            w.set_running(true);
            w.set_progress("beginning work".into());
            cancellation.store(false, Ordering::Release);
            let store = store.clone();
            let cancel = cancellation.clone();
            let (send, receive) = std::sync::mpsc::channel::<Result<Progress, String>>();
            std::thread::spawn(move || {
                let result =
                    hydrus_store::tag_migration::run(&store, &request, &cancel, 512, |p| {
                        let _ = send.send(Ok(p));
                    });
                let _ = send.send(result.map_err(|e| e.to_string()));
                // Dropping send is the completion signal, including failures.
            });
            let weak = weak.clone();
            let changed = changed.clone();
            let weak_timer = Rc::downgrade(&timer);
            let close_requested = close_requested.clone();
            timer.start(
                slint::TimerMode::Repeated,
                Duration::from_millis(80),
                move || {
                    let Some(w) = weak.upgrade() else { return };
                    loop {
                        match receive.try_recv() {
                            Ok(Ok(p)) => {
                                w.set_progress(
                                    format!(
                                        "{}; {} entries scanned, {} accepted",
                                        if p.cancelled {
                                            "cancelled (committed batches retained)"
                                        } else {
                                            "migrating"
                                        },
                                        p.scanned,
                                        p.accepted
                                    )
                                    .into(),
                                );
                            }
                            Ok(Err(e)) => w.set_error(e.into()),
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                if let Some(timer) = weak_timer.upgrade() {
                                    timer.stop();
                                }
                                if w.get_running() {
                                    w.set_running(false);
                                    if !w.get_progress().starts_with("cancelled")
                                        && w.get_error().is_empty()
                                    {
                                        w.set_progress(
                                            format!("done! {}", w.get_progress()).into(),
                                        );
                                    }
                                    changed();
                                }
                                if close_requested.get() {
                                    w.invoke_close_clicked();
                                }
                                break;
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        }
                    }
                },
            );
        }
    });
    window.on_cancel_job({
        let cancellation = cancellation.clone();
        move || {
            cancellation.store(true, Ordering::Release);
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let timer = timer.clone();
        move || {
            cancellation.store(true, Ordering::Release);
            if let Some(w) = weak.upgrade() {
                if w.get_running() {
                    close_requested.set(true);
                    w.set_progress("cancelling… (waiting for current batch)".into());
                    return;
                }
            }
            timer.stop();
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
    window.window().on_close_requested({
        let weak = window.as_weak();
        move || {
            let running = weak.upgrade().is_some_and(|w| w.get_running());
            close();
            if running {
                slint::CloseRequestResponse::KeepWindowShown
            } else {
                slint::CloseRequestResponse::HideWindow
            }
        }
    });
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    *slot.borrow_mut() = Some(window.clone_strong());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
