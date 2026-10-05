//! Bind manual export previews and a background worker to the export window.
use std::cell::{Cell, RefCell};
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
    tags: export_files::tags::Tags,
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
    let active = Rc::new(Cell::new(true));
    crate::gui_colours::bind(window.global::<crate::Theme<'_>>(), store, active.clone());
    window.set_pattern_shortcuts(ModelRc::new(VecModel::from(
        export_files::PATTERN_SHORTCUTS
            .iter()
            .map(|(label, _)| (*label).into())
            .collect::<Vec<_>>(),
    )));
    window.set_pattern_heading(export_files::PATTERN_SHORTCUT_HEADING.into());
    window.on_pattern_chosen({
        let active = active.clone();
        let weak = window.as_weak();
        let sidecars = slots.sidecars.clone();
        move |index| {
            if active.get()
                && sidecars.routers.borrow().is_none()
                && weak
                    .upgrade()
                    .is_some_and(|window| !window.get_working() && !window.get_asking())
                && let Some(phrase) = export_files::pattern_shortcut(index)
            {
                crate::copy_to_clipboard(phrase);
            }
        }
    });
    let preferences: Preferences = store.read(settings::get).unwrap_or_default();
    let naming: settings::ExportSettings = store.read(settings::get).unwrap_or_default();
    let destination = export_files::default_directory(store, &naming);
    if naming.default_directory.is_none() && !destination.is_empty() {
        std::fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    }
    window.set_destination(destination.into());
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
        tags: export_files::tags::Tags::new(store.clone())
            .map_err(|error| slint::PlatformError::Other(error.to_string()))?,
    }));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
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
            let selected = state.selection.in_order(&state.files);
            let files = if selected.is_empty() {
                state.files.clone()
            } else {
                selected
            };
            if let Err(error) = state.tags.refresh(&files) {
                window.set_status(error.to_string().into());
            }
            window.set_tags(ModelRc::new(VecModel::from(
                state
                    .tags
                    .rows()
                    .iter()
                    .map(|row| crate::list_text(&row.text, row.colour))
                    .collect::<Vec<_>>(),
            )));
            window.set_tag_selected(ModelRc::new(VecModel::from(
                state
                    .tags
                    .rows()
                    .iter()
                    .map(|row| state.tags.selection.is_selected(row.id))
                    .collect::<Vec<_>>(),
            )));
            window.set_tag_sort_type(
                i32::try_from(
                    crate::options::TAG_SORT_TYPES
                        .iter()
                        .position(|(_, sort_type)| *sort_type == state.tags.sort.sort_type)
                        .unwrap_or(0),
                )
                .unwrap_or(0),
            );
            let (orders, selected) = crate::options::tag_sort_orders(&state.tags.sort);
            window.set_tag_sort_orders(ModelRc::new(VecModel::from(
                orders.map(slint::SharedString::from).to_vec(),
            )));
            window.set_tag_sort_order(i32::try_from(selected).unwrap_or(0));
            window.set_tag_sort_group(
                i32::try_from(
                    crate::options::TAG_SORT_GROUPS
                        .iter()
                        .position(|(_, group)| *group == state.tags.sort.group_by)
                        .unwrap_or(0),
                )
                .unwrap_or(0),
            );
        }
    });
    refresh();
    let tags_editable: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        let active = active.clone();
        let sidecars = slots.sidecars.clone();
        move || {
            active.get()
                && sidecars.routers.borrow().is_none()
                && weak
                    .upgrade()
                    .is_some_and(|w| !w.get_working() && !w.get_asking())
        }
    });
    let tag_menu = crate::write_tag_menu::TagMenu::new(
        store.clone(),
        tags_editable.clone(),
        Rc::new(|_| {}),
        refresh.clone(),
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
                    w.set_status(error.into());
                }
            }
        }),
    );
    crate::write_tag_menu::bind!(window, tag_menu);
    window.on_tag_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        let editable = tags_editable.clone();
        let menu = tag_menu.clone();
        move |index, ctrl, shift| {
            if editable()
                && !menu.busy()
                && let Ok(index) = usize::try_from(index)
            {
                state.borrow_mut().tags.click(index, ctrl, shift);
                refresh();
            }
        }
    });
    window.on_tag_sort_chosen({
        let state = state.clone();
        let refresh = refresh.clone();
        let editable = tags_editable.clone();
        let menu = tag_menu.clone();
        move |part, index| {
            if editable()
                && !menu.busy()
                && let (Ok(part), Ok(index)) = (usize::try_from(part), usize::try_from(index))
            {
                state.borrow_mut().tags.sort_chosen(part, index);
                refresh();
            }
        }
    });
    window.on_tag_copy({
        let state = state.clone();
        let editable = tags_editable.clone();
        let menu = tag_menu.clone();
        move || {
            if editable() && !menu.busy() {
                let text = state
                    .borrow()
                    .tags
                    .copy(export_files::tags::CopyOptions::default());
                if !text.is_empty() {
                    crate::copy_to_clipboard(&text);
                }
            }
        }
    });
    window.on_tag_select_all({
        let state = state.clone();
        let refresh = refresh.clone();
        let editable = tags_editable.clone();
        let menu = tag_menu.clone();
        move || {
            if editable() && !menu.busy() {
                state.borrow_mut().tags.select_all();
                refresh();
            }
        }
    });
    window.on_tag_context_menu({
        let state = state.clone();
        let refresh = refresh.clone();
        let editable = tags_editable.clone();
        let menu = tag_menu.clone();
        move |index, x, y| {
            if editable() && !menu.busy() {
                let mut state = state.borrow_mut();
                if let Ok(index) = usize::try_from(index)
                    && let Some(row) = state.tags.rows().get(index)
                    && !state.tags.selection.is_selected(row.id)
                {
                    state.tags.click(index, false, false);
                }
                let entries = state.tags.menu();
                drop(state);
                refresh();
                menu.open(&entries, x, y);
            }
        }
    });
    window.on_tag_middle_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        let editable = tags_editable.clone();
        let menu = tag_menu.clone();
        move |index, ctrl, shift| {
            if editable()
                && !menu.busy()
                && let Ok(index) = usize::try_from(index)
            {
                state.borrow_mut().tags.click(index, ctrl, shift);
                refresh();
                let action = state.borrow().tags.launch(shift);
                if let Some(action) = action {
                    menu.choose(Some(crate::popup_menu::Chosen::Action(action)));
                }
            }
        }
    });
    let close = Rc::new({
        let weak = window.as_weak();
        let slots = slots.clone();
        let state = state.clone();
        let active = active.clone();
        let tag_menu = tag_menu.clone();
        move || {
            active.set(false);
            tag_menu.close();
            state.borrow().cancel.store(true, Ordering::Release);
            slots.timer.stop();
            slots.sidecars.cancel();
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
            slots.sidecars.set_test_objects(
                state
                    .borrow()
                    .files
                    .iter()
                    .copied()
                    .map(crate::sidecar_editors::TestObject::Media)
                    .collect(),
            );
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
            let phrase: String = w.get_phrase().into();
            if let Err(e) = store.write(move |ctx| {
                settings::set(ctx.conn(), &preferences)?;
                let mut naming: settings::ExportSettings = settings::get(ctx.conn())?;
                naming.phrase = phrase;
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
