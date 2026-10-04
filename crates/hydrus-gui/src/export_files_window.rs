//! Bind manual export previews and a background worker to the export window.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crate::{ExportFilesWindow, TableRow};
use hydrus_core::HashId;
use hydrus_gui_model::{
    export_files::{self, Plan, Preferences, Progress},
    list_selection::ListSelection,
    sidecar_editors::Context,
};
use hydrus_store::{Store, settings};
use slint::{ComponentHandle, ModelRc, VecModel};

/// Keep the dialog, its sidecar editor and progress timer alive while open.
#[derive(Clone, Default)]
pub struct Slots {
    /// Current manual export dialog.
    pub window: Rc<RefCell<Option<ExportFilesWindow>>>,
    /// Existing sidecar routers editor.
    pub sidecars: crate::sidecars_window::Slots,
    timer: Rc<slint::Timer>,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExportSlots")
            .field("open", &self.window.borrow().is_some())
            .finish_non_exhaustive()
    }
}
struct State {
    files: Vec<HashId>,
    selection: ListSelection<HashId>,
    preferences: Preferences,
    pending: Option<bool>,
    remove: bool,
    close_after: bool,
    cancel: Arc<AtomicBool>,
    receiver: Option<std::sync::mpsc::Receiver<Progress>>,
}

/// Open a manual export window over selected local files.
pub fn open(
    store: &Arc<Store>,
    files: Vec<HashId>,
    slots: &Slots,
    changed: Rc<dyn Fn()>,
) -> Result<ExportFilesWindow, slint::PlatformError> {
    if let Some(window) = slots.window.borrow().as_ref() {
        window.show()?;
        return Ok(window.clone_strong());
    }
    let window = ExportFilesWindow::new()?;
    let preferences: Preferences = store.read(settings::get).unwrap_or_default();
    let naming: settings::ExportSettings = store.read(settings::get).unwrap_or_default();
    window.set_destination(preferences.destination.clone().into());
    window.set_phrase(naming.phrase.into());
    window.set_trash(preferences.trash);
    let state = Rc::new(RefCell::new(State {
        files,
        selection: ListSelection::default(),
        preferences,
        pending: None,
        remove: false,
        close_after: false,
        cancel: Arc::new(AtomicBool::new(false)),
        receiver: None,
    }));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let state = state.borrow();
            match export_files::preview(
                &store,
                &state.files,
                &window.get_destination(),
                &window.get_phrase(),
            ) {
                Ok(rows) => {
                    let rows: Vec<_> = rows
                        .into_iter()
                        .map(|r| TableRow {
                            cells: ModelRc::new(VecModel::from(vec![
                                r.number.to_string().into(),
                                r.mime.into(),
                                r.destination.to_string_lossy().into_owned().into(),
                            ])),
                            selected: state.selection.is_selected(r.file),
                        })
                        .collect();
                    window.set_rows(ModelRc::new(VecModel::from(rows)));
                    window.set_status("".into());
                }
                Err(e) => {
                    window.set_rows(ModelRc::default());
                    window.set_status(e.into());
                }
            }
            window.set_sidecars(
                crate::sidecars::button_label(
                    &state.preferences.routers,
                    &crate::sidecars_window::namer(&store),
                )
                .0
                .into(),
            );
        }
    });
    refresh();
    let close = Rc::new({
        let weak = window.as_weak();
        let slots = slots.clone();
        let state = state.clone();
        move || {
            state.borrow().cancel.store(true, Ordering::Release);
            slots.timer.stop();
            macro_rules! hide_slot {
                ($slot:expr) => {
                    if let Some(w) = $slot.borrow_mut().take() {
                        let _ = w.hide();
                    }
                };
            }
            hide_slot!(slots.sidecars.node);
            hide_slot!(slots.sidecars.router);
            hide_slot!(slots.sidecars.routers);
            hide_slot!(slots.sidecars.strings.processor);
            hide_slot!(slots.sidecars.strings.step);
            hide_slot!(slots.sidecars.strings.converter);
            hide_slot!(slots.sidecars.strings.conversion);
            hide_slot!(slots.sidecars.strings.tag_filter);
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slots.window.borrow_mut().take();
        }
    });
    window.on_dismissed({
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
    window.on_update({
        let refresh = refresh.clone();
        move || refresh()
    });
    window.on_browse({
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move || {
            if let (Some(w), Some(p)) = (
                weak.upgrade(),
                crate::pick(crate::Pick::Folder, "Select an export folder.").first(),
            ) {
                w.set_destination(p.to_string_lossy().into_owned().into());
                refresh();
            }
        }
    });
    window.on_open_location({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                let path = w.get_destination();
                if std::path::Path::new(path.as_str()).is_dir() {
                    crate::launch(&path);
                } else {
                    w.set_status("That location does not seem to exist!".into());
                }
            }
        }
    });
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |row, ctrl, shift| {
            let mut s = state.borrow_mut();
            let order = s.files.clone();
            s.selection.click(
                &order,
                usize::try_from(row).unwrap_or(usize::MAX),
                ctrl,
                shift,
            );
            drop(s);
            refresh();
        }
    });
    window.on_remove({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            let mut s = state.borrow_mut();
            if s.files.iter().any(|f| s.selection.is_selected(*f)) {
                s.remove = true;
                if let Some(w) = weak.upgrade() {
                    w.set_question(export_files::REMOVE_QUESTION.into());
                    w.set_asking(true);
                }
            }
        }
    });
    window.on_edit_sidecars({
        let store = store.clone();
        let slots = slots.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let routers = state.borrow().preferences.routers.clone();
            let state = state.clone();
            let refresh = refresh.clone();
            let store_saved = store.clone();
            match crate::sidecars_window::open_routers(
                &store,
                Context::Export,
                routers,
                &slots.sidecars,
                Rc::new(move |routers| {
                    state.borrow_mut().preferences.routers = routers;
                    let value = state.borrow().preferences.clone();
                    let _ = store_saved.write(move |ctx| settings::set(ctx.conn(), &value));
                    refresh();
                }),
            ) {
                Ok(window) => *slots.sidecars.routers.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open export sidecars: {e}"),
            }
        }
    });
    let start: Rc<dyn Fn(bool)> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        move |close_after| {
            refresh();
            let Some(w) = weak.upgrade() else {
                return;
            };
            let mut s = state.borrow_mut();
            let rows = match export_files::preview(
                &store,
                &s.files,
                &w.get_destination(),
                &w.get_phrase(),
            ) {
                Ok(rows) if !rows.is_empty() => rows,
                Ok(_) => {
                    w.set_status("No files to export.".into());
                    return;
                }
                Err(e) => {
                    w.set_status(e.into());
                    return;
                }
            };
            let directory = export_files::directory_path(&w.get_destination()).unwrap_or_default();
            let plan = Plan {
                directory,
                rows,
                routers: s.preferences.routers.clone(),
                trash: w.get_trash(),
                symlinks: w.get_symlinks() && !w.get_trash(),
            };
            if let Some(question) = plan.confirmation(close_after)
                && s.pending.is_none()
            {
                s.pending = Some(close_after);
                w.set_question(question.into());
                w.set_asking(true);
                return;
            }
            s.pending = None;
            s.preferences.destination = w.get_destination().into();
            s.preferences.trash = w.get_trash();
            let preferences = s.preferences.clone();
            let mut naming: settings::ExportSettings =
                store.read(settings::get).unwrap_or_default();
            naming.phrase = w.get_phrase().into();
            if let Err(e) = store.write(move |ctx| {
                settings::set(ctx.conn(), &preferences)?;
                settings::set(ctx.conn(), &naming)
            }) {
                w.set_status(e.to_string().into());
                return;
            }
            let (sender, receiver) = std::sync::mpsc::channel();
            s.receiver = Some(receiver);
            s.close_after = close_after;
            s.cancel.store(false, Ordering::Release);
            let cancel = s.cancel.clone();
            let store = store.clone();
            w.set_working(true);
            w.set_progress(0.0);
            w.set_status("Exporting: 0".into());
            std::thread::spawn(move || {
                export_files::run(&store, &plan, &cancel, |progress| {
                    let _ = sender.send(progress);
                });
            });
        }
    });
    window.on_export({
        let start = start.clone();
        move |close_after| start(close_after)
    });
    window.on_answer({
        let state = state.clone();
        let weak = window.as_weak();
        let start = start.clone();
        let refresh = refresh.clone();
        let close = close.clone();
        move |answer| {
            if let Some(w) = weak.upgrade() {
                w.set_asking(false);
            }
            let mut s = state.borrow_mut();
            if s.remove {
                s.remove = false;
                if answer == 0 {
                    let kept: Vec<_> = s
                        .files
                        .iter()
                        .copied()
                        .filter(|f| !s.selection.is_selected(*f))
                        .collect();
                    s.files = kept;
                    s.selection = ListSelection::default();
                }
                drop(s);
                refresh();
            } else if let Some(close_after) = s.pending {
                if answer == 0 {
                    drop(s);
                    start(close_after);
                } else {
                    s.pending = None;
                    drop(s);
                    if close_after {
                        close();
                    }
                }
            }
        }
    });
    window.on_cancel({
        let state = state.clone();
        move || state.borrow().cancel.store(true, Ordering::Release)
    });
    slots
        .timer
        .start(slint::TimerMode::Repeated, Duration::from_millis(40), {
            let weak = window.as_weak();
            let state = state.clone();
            let close = close.clone();
            move || {
                let latest = {
                    let s = state.borrow();
                    s.receiver.as_ref().and_then(|r| r.try_iter().last())
                };
                let Some(p) = latest else {
                    return;
                };
                let Some(w) = weak.upgrade() else {
                    return;
                };
                #[allow(clippy::cast_precision_loss)]
                w.set_progress(if p.total == 0 {
                    0.0
                } else {
                    p.completed as f32 / p.total as f32
                });
                w.set_status(
                    p.error
                        .clone()
                        .unwrap_or_else(|| {
                            if p.cancelled {
                                format!("Cancelled: {} / {} exported", p.completed, p.total)
                            } else if p.finished {
                                "done!".into()
                            } else {
                                format!("Exporting: {} / {}", p.completed, p.total)
                            }
                        })
                        .into(),
                );
                if p.finished {
                    w.set_working(false);
                    let close_after = {
                        let mut s = state.borrow_mut();
                        s.receiver = None;
                        s.close_after
                    };
                    if p.trashed > 0 {
                        changed();
                    }
                    if close_after && p.error.is_none() && !p.cancelled {
                        close();
                    }
                }
            }
        });
    window.show()?;
    *slots.window.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
