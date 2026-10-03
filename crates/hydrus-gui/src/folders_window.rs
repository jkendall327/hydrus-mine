//! The manage import folders and manage export folders dialogs, bound
//! (file > import/export folders): each list read from the store into a
//! [`Named`] list, shown as the reference writes it, with add, edit and
//! delete, and "apply", which writes the list back; and the edit dialogs
//! ([`ImportFolderWindow`], [`ExportFolderWindow`]), which check what the
//! reference checks on "apply" and give the folder back to the list.
//! Import folders are import queues (their import options and file logs
//! are kept); export folders are a setting.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString, VecModel};

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_parse::folders::{ExportFolder, ExportType, FolderAction};
use hydrus_search::{TextContext, parse_api_search, predicate_text};
use hydrus_store::Store;
use hydrus_store::settings::{self, ExportFolders};
use hydrus_store::{import_folders, queues};

use crate::folders::{
    ACTION_CHOICES, DELETE_QUESTION, DELETE_WARNING, EXPORT_COLUMNS, EXPORT_INTRO,
    EXPORT_TYPES_TEXT, IMPORT_COLUMNS, IMPORT_INTRO, IMPORT_WARNING, ImportFolderEdit, Named,
    OUTCOMES, SIDECARS_TEXT, Sensitive, action_index, check_export_folder, check_import_folder,
    export_folder_row, import_folder_row, new_export_folder,
};
use crate::options::{Unit, duration_fields, duration_seconds};
use crate::subscriptions_dialog::DELETE_QUESTION as REMOVE_SELECTED;
use crate::{
    DurationField, ExportFolderWindow, FolderActionRow, FoldersWindow, ImportFolderWindow,
    TableColumn, TableRow,
};

/// Change a dialog's state, and show it again.
type Change<S> = Rc<dyn Fn(&dyn Fn(&mut S))>;

/// The folders dialogs while they are open.
#[derive(Clone, Default)]
pub struct Slots {
    pub import_list: Rc<RefCell<Option<FoldersWindow>>>,
    pub import_edit: Rc<RefCell<Option<ImportFolderWindow>>>,
    pub export_list: Rc<RefCell<Option<FoldersWindow>>>,
    pub export_edit: Rc<RefCell<Option<ExportFolderWindow>>>,
    /// The filename tagging options editor.
    pub filename_tagging: Rc<RefCell<Option<crate::FilenameTaggingWindow>>>,
    /// The import options editor.
    pub import_options: Rc<RefCell<Option<crate::ImportOptionsWindow>>>,
    /// An import folder's file log, and where it shows files.
    pub log: Rc<RefCell<Option<crate::FileLogWindow>>>,
    pub open_files: crate::file_log_window::OpenFiles,
}

impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("import_list", &self.import_list.borrow().is_some())
            .field("import_edit", &self.import_edit.borrow().is_some())
            .field("export_list", &self.export_list.borrow().is_some())
            .field("export_edit", &self.export_edit.borrow().is_some())
            .field("import_options", &self.import_options.borrow().is_some())
            .field(
                "filename_tagging",
                &self.filename_tagging.borrow().is_some(),
            )
            .field("log", &self.log.borrow().is_some())
            .field("open_files", &self.open_files)
            .finish()
    }
}

fn strings(items: Vec<String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

fn columns(titles: &[&str]) -> ModelRc<TableColumn> {
    let columns: Vec<TableColumn> = titles
        .iter()
        .enumerate()
        .map(|(i, t)| TableColumn {
            title: (*t).into(),
            width: 140.0,
            stretch: i == 1,
        })
        .collect();
    ModelRc::new(VecModel::from(columns))
}

fn table(rows: Vec<(Vec<String>, bool)>) -> ModelRc<TableRow> {
    let rows: Vec<TableRow> = rows
        .into_iter()
        .map(|(cells, selected)| TableRow {
            cells: strings(cells),
            selected,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// A time's fields, as the time widgets show them.
fn time_fields(seconds: i64, units: &[Unit]) -> ModelRc<DurationField> {
    #[allow(clippy::cast_precision_loss)] // (seconds a person types)
    let fields = duration_fields(seconds as f64, units);
    let fields: Vec<DurationField> = fields
        .iter()
        .zip(units)
        .map(|(&value, unit)| DurationField {
            value: i32::try_from(value).unwrap_or(i32::MAX),
            maximum: i32::try_from(unit.max()).unwrap_or(i32::MAX),
            label: unit.label().into(),
        })
        .collect();
    ModelRc::new(VecModel::from(fields))
}

/// A time with one field edited, held to at least `min`.
fn time_edited(seconds: i64, units: &[Unit], field: i32, value: i32, min: i64) -> i64 {
    #[allow(clippy::cast_precision_loss)] // (seconds a person types)
    let mut fields = duration_fields(seconds as f64, units);
    if let Some(f) = usize::try_from(field).ok().and_then(|f| fields.get_mut(f)) {
        *f = i64::from(value.max(0));
    }
    #[allow(clippy::cast_possible_truncation)] // (whole seconds)
    let seconds = duration_seconds(&fields, units) as i64;
    seconds.max(min)
}

const PERIOD_UNITS: [Unit; 3] = [Unit::Days, Unit::Hours, Unit::Minutes];
const SKIP_UNITS: [Unit; 4] = [Unit::Days, Unit::Hours, Unit::Minutes, Unit::Seconds];

/// The sidecar button's label (`_RefreshLabel`).
fn sidecars_label(n: usize) -> String {
    match n {
        0 => "no sidecars".into(),
        1 => "1 sidecar action".into(),
        n => format!(
            "{} sidecar actions",
            hydrus_core::numbers::human_int(n as u64)
        ),
    }
}

/// What a list or edit window's question panel waits on.
#[derive(Clone)]
enum Asking {
    /// "Remove all selected?"
    Delete,
    /// Messages with only "ok", the first shown, then what to do after.
    Messages(Vec<String>, After),
    /// "apply" on an export folder that deletes from the client.
    ConfirmDelete,
}

/// What follows the messages.
#[derive(Clone, Copy)]
enum After {
    Nothing,
    /// The edit is good: give it back.
    Done,
}

fn show_asking<F>(asking: Option<&Asking>, set: F)
where
    F: Fn(bool, &str, &str, Vec<String>),
{
    match asking {
        None => set(false, "", "", Vec::new()),
        Some(Asking::Delete) => set(
            true,
            "Are you sure?",
            REMOVE_SELECTED,
            vec!["yes".into(), "no".into()],
        ),
        Some(Asking::ConfirmDelete) => set(
            true,
            "Are you sure?",
            DELETE_QUESTION,
            vec!["yes".into(), "no".into()],
        ),
        Some(Asking::Messages(messages, _)) => set(
            true,
            "Warning",
            messages.first().map_or("", String::as_str),
            vec!["ok".into()],
        ),
    }
}

/// Where hydrus keeps its own files (`GetImportSensitiveDirectories`).
fn sensitive(store: &Store) -> Sensitive {
    let untouchable = store
        .read(hydrus_store::storage::FileStorage::load)
        .map(|s| {
            s.locations()
                .iter()
                .map(|l| l.path.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    Sensitive {
        allow_inside: vec![store.dir().to_string_lossy().into_owned()],
        untouchable,
    }
}

fn exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

// import folders --------------------------------------------------------------

/// The import folders list's state while it is open.
struct ImportList {
    list: Named<ImportFolderEdit>,
    /// Those deleted that are in the store.
    deleted: Vec<i64>,
    asking: Option<Asking>,
}

fn show_import_list(window: &FoldersWindow, open: &ImportList) {
    let rows = open
        .list
        .in_order()
        .into_iter()
        .map(|(key, f)| {
            (
                import_folder_row(&f.name, f.paused, &f.settings),
                open.list.selection.is_selected(key),
            )
        })
        .collect();
    window.set_rows(table(rows));
    window.set_any_selected(!open.list.selection.is_empty());
    window.set_one_selected(open.list.one_selected().is_some());
    show_asking(open.asking.as_ref(), |asking, title, message, choices| {
        window.set_asking(asking);
        window.set_asking_title(title.into());
        window.set_asking_message(message.into());
        window.set_asking_choices(strings(choices));
    });
}

/// Write the import folders as the list holds them.
fn write_import_folders(store: &Store, open: ImportList) -> hydrus_store::Result<()> {
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let deleted = open.deleted;
    let folders = open.list.into_items();
    store.write(move |ctx| {
        let conn = ctx.conn();
        for id in &deleted {
            queues::delete_queue(conn, *id)?;
        }
        for folder in &folders {
            match folder.id {
                Some(id) => {
                    queues::rename_queue(conn, id, &folder.name)?;
                    queues::set_paused(conn, id, Some(folder.paused), None)?;
                    import_folders::set_settings(conn, id, &folder.settings)?;
                    queues::set_queue_options(conn, id, &folder.options)?;
                }
                None => {
                    if import_folders::create_import_folder(
                        conn,
                        &folder.name,
                        &folder.settings,
                        &folder.options,
                        folder.paused,
                        now,
                    )?
                    .is_none()
                    {
                        eprintln!(
                            "could not add the import folder {:?}: the name is taken",
                            folder.name
                        );
                    }
                }
            }
        }
        Ok(())
    })
}

/// Open the manage import folders dialog on the store's import folders.
/// A folder from the system's picker ("browse", `PickDirectory`).
fn picked_folder() -> Option<String> {
    crate::pick(crate::Pick::Folder, "Select directory")
        .first()
        .map(|p| p.to_string_lossy().into_owned())
}

pub(crate) fn open_import_folders(store: &Arc<Store>, slots: &Slots) -> Result<(), String> {
    if let Some(window) = slots.import_list.borrow().as_ref() {
        return window.show().map_err(|e| e.to_string());
    }
    let folders = store
        .read(import_folders::import_folders)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|f| ImportFolderEdit {
            id: Some(f.id()),
            name: f.name().to_owned(),
            paused: f.paused(),
            options: store
                .read(|c| queues::queue(c, f.id()))
                .ok()
                .flatten()
                .map(|q| q.options)
                .unwrap_or_default(),
            settings: f.settings,
        })
        .collect();
    let state = Rc::new(RefCell::new(ImportList {
        list: Named::new(folders),
        deleted: Vec::new(),
        asking: None,
    }));
    let window = FoldersWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title("edit import folders".into());
    window.set_intro(IMPORT_INTRO.into());
    window.set_warning(IMPORT_WARNING.into());
    window.set_columns(columns(&IMPORT_COLUMNS));
    let change: Change<ImportList> = {
        let weak = window.as_weak();
        let state = state.clone();
        Rc::new(move |f: &dyn Fn(&mut ImportList)| {
            f(&mut state.borrow_mut());
            if let Some(window) = weak.upgrade() {
                show_import_list(&window, &state.borrow());
            }
        })
    };
    let close = {
        let weak = window.as_weak();
        let slots = slots.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slots.import_list.borrow_mut().take();
            if let Some(edit) = slots.import_edit.borrow_mut().take() {
                let _ = edit.hide();
            }
        }
    };
    // the edit dialog, on the one selected or a new one
    let edit: Rc<dyn Fn(Option<u64>)> = {
        let state = state.clone();
        let change = change.clone();
        let slots = slots.clone();
        let store = store.clone();
        Rc::new(move |key: Option<u64>| {
            if slots.import_edit.borrow().is_some() {
                return;
            }
            let folder = if let Some(k) = key {
                let Some(f) = state.borrow().list.get(k).cloned() else {
                    return;
                };
                f
            } else {
                ImportFolderEdit::new_folder()
            };
            let done: Rc<dyn Fn(ImportFolderEdit)> = {
                let change = change.clone();
                Rc::new(move |edited: ImportFolderEdit| {
                    change(&|open| match key {
                        Some(k) => open.list.replace(k, edited.clone()),
                        None => {
                            open.list.add(edited.clone());
                        }
                    });
                })
            };
            match open_import_folder(&store, folder, &slots, &done) {
                Ok(window) => *slots.import_edit.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the import folder: {e}"),
            }
        })
    };
    bind_list(
        &window,
        &change,
        |open: &mut ImportList| &mut open.asking,
        |open: &mut ImportList, row, ctrl, shift| open.list.click(row, ctrl, shift),
        |open: &ImportList| open.list.one_selected(),
        |open: &ImportList, row| open.list.order().get(row).copied(),
        |open: &mut ImportList| {
            let deleted = open.list.delete_selected();
            open.deleted.extend(deleted.iter().filter_map(|f| f.id));
        },
        &edit,
        &state,
    );
    window.on_apply({
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            let open = std::mem::replace(
                &mut *state.borrow_mut(),
                ImportList {
                    list: Named::new(Vec::new()),
                    deleted: Vec::new(),
                    asking: None,
                },
            );
            if let Err(e) = write_import_folders(&store, open) {
                eprintln!("could not save the import folders: {e}");
            }
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
    show_import_list(&window, &state.borrow());
    window.show().map_err(|e| e.to_string())?;
    *slots.import_list.borrow_mut() = Some(window);
    Ok(())
}

/// Bind a folders list's rows and buttons: clicks, double-clicks and
/// "edit" open the edit dialog (`edit`, on a key or none for "add"),
/// "delete" asks first.
#[allow(clippy::too_many_arguments)]
fn bind_list<S: 'static>(
    window: &FoldersWindow,
    change: &Change<S>,
    asking: fn(&mut S) -> &mut Option<Asking>,
    click: fn(&mut S, usize, bool, bool),
    one: fn(&S) -> Option<u64>,
    at_row: fn(&S, usize) -> Option<u64>,
    delete: fn(&mut S),
    edit: &Rc<dyn Fn(Option<u64>)>,
    state: &Rc<RefCell<S>>,
) {
    window.on_row_clicked({
        let change = change.clone();
        move |row, ctrl, shift| {
            if let Ok(row) = usize::try_from(row) {
                change(&|s| click(s, row, ctrl, shift));
            }
        }
    });
    window.on_row_activated({
        let edit = edit.clone();
        let state = state.clone();
        move |row| {
            let key = usize::try_from(row)
                .ok()
                .and_then(|r| at_row(&state.borrow(), r));
            if key.is_some() {
                edit(key);
            }
        }
    });
    window.on_add({
        let edit = edit.clone();
        move || edit(None)
    });
    window.on_edit({
        let edit = edit.clone();
        let state = state.clone();
        move || {
            let key = one(&state.borrow());
            if key.is_some() {
                edit(key);
            }
        }
    });
    window.on_delete({
        let change = change.clone();
        move || change(&|s| *asking(s) = Some(Asking::Delete))
    });
    window.on_chosen({
        let change = change.clone();
        move |index| {
            change(&|s| {
                if let Some(Asking::Delete) = asking(s).take()
                    && index == 0
                {
                    delete(s);
                }
            });
        }
    });
    window.on_cancelled({
        let change = change.clone();
        move || change(&|s| *asking(s) = None)
    });
}

/// The import folder edit dialog's state while it is open.
struct ImportEdit {
    folder: ImportFolderEdit,
    asking: Option<Asking>,
}

/// The action an outcome's row chose, with its location.
fn folder_action(index: i32, location: &str) -> FolderAction {
    match index {
        0 => FolderAction::Delete,
        2 => FolderAction::Move(location.to_owned()),
        _ => FolderAction::Ignore,
    }
}

fn action_rows(folder: &ImportFolderEdit) -> ModelRc<FolderActionRow> {
    let a = &folder.settings.actions;
    let rows: Vec<FolderActionRow> = [
        &a.successful_and_new,
        &a.successful_but_redundant,
        &a.deleted,
        &a.error,
    ]
    .into_iter()
    .zip(OUTCOMES)
    .map(|(action, (label, _))| FolderActionRow {
        label: label.into(),
        action: i32::try_from(action_index(action)).unwrap_or(1),
        location: match action {
            FolderAction::Move(l) => l.as_str().into(),
            _ => SharedString::new(),
        },
    })
    .collect();
    ModelRc::new(VecModel::from(rows))
}

/// The edit dialog's fields, into its folder.
fn read_import_fields(window: &ImportFolderWindow, folder: &mut ImportFolderEdit) {
    folder.name = window.get_name().into();
    folder.paused = window.get_paused();
    let s = &mut folder.settings;
    s.path = window.get_path().into();
    s.search_subdirectories = window.get_search_subdirectories();
    s.check_regularly = window.get_check_regularly();
    s.check_now = window.get_check_now();
    s.show_working_popup = window.get_show_popup();
    s.publish_files_to_popup_button = window.get_publish_popup_button();
    s.publish_files_to_page = window.get_publish_page();
    let rows = window.get_actions();
    let actions: Vec<FolderAction> = (0..rows.row_count())
        .filter_map(|i| rows.row_data(i))
        .map(|r| folder_action(r.action, &r.location))
        .collect();
    if let [new, redundant, deleted, error] = &actions[..] {
        s.actions.successful_and_new = new.clone();
        s.actions.successful_but_redundant = redundant.clone();
        s.actions.deleted = deleted.clone();
        s.actions.error = error.clone();
    }
}

/// The real tag services: (key hex, name).
fn tag_services(store: &Store) -> Vec<(String, String)> {
    store
        .snapshot()
        .services
        .all()
        .filter(|s| s.service_type().is_real_tag_service())
        .map(|s| (s.key.to_hex(), s.name.clone()))
        .collect()
}

/// The folder's filename tagging list (its services' names), and the
/// services that can be added.
fn show_filename_tagging(window: &ImportFolderWindow, store: &Store, folder: &ImportFolderEdit) {
    let services = tag_services(store);
    let name = |key: &str| {
        services
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| "unknown service".to_owned(), |(_, n)| n.clone())
    };
    window.set_filename_tagging(strings(
        folder
            .settings
            .filename_tagging
            .iter()
            .map(|(key, _)| name(key))
            .collect(),
    ));
    window.set_tagging_choices(strings(services.into_iter().map(|(_, n)| n).collect()));
}

/// The first file in a folder, for an example path (the reference's
/// sidecar test context's first example file).
fn example_path(folder: &str) -> String {
    let mut files: Vec<String> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect();
    files.sort();
    files.into_iter().next().unwrap_or_default()
}

/// Open the edit import folder dialog on a folder; on "apply" (once it
/// checks out) it gives the edited folder to `done`.
fn open_import_folder(
    store: &Arc<Store>,
    folder: ImportFolderEdit,
    slots: &Slots,
    done: &Rc<dyn Fn(ImportFolderEdit)>,
) -> Result<ImportFolderWindow, String> {
    let slot = &slots.import_edit;
    let window = ImportFolderWindow::new().map_err(|e| e.to_string())?;
    // its file log (a new folder has none yet)
    window.set_has_file_log(folder.id.is_some());
    if let Some(queue) = folder.id {
        let store = store.clone();
        let log = slots.log.clone();
        let open_files = slots.open_files.clone();
        window.on_file_log(move || {
            if let Some(old) = log.borrow_mut().take() {
                let _ = old.hide();
            }
            match crate::file_log_window::open(&store, queue, &log, &open_files.0) {
                Ok(window) => *log.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the file log: {e}"),
            }
        });
    }
    let s = &folder.settings;
    window.set_name(folder.name.clone().into());
    window.set_path(s.path.clone().into());
    window.set_search_subdirectories(s.search_subdirectories);
    window.set_paused(folder.paused);
    window.set_check_regularly(s.check_regularly);
    window.set_period(time_fields(s.period, &PERIOD_UNITS));
    window.set_skip(time_fields(s.last_modified_time_skip_period, &SKIP_UNITS));
    window.set_check_now(s.check_now);
    window.set_show_popup(s.show_working_popup);
    window.set_publish_popup_button(s.publish_files_to_popup_button);
    window.set_publish_page(s.publish_files_to_page);
    let seen = folder
        .id
        .and_then(|id| store.read(|c| queues::file_seed_counts(c, id)).ok())
        .unwrap_or_default();
    window.set_cached_paths(queues::file_log_status(&seen).into());
    window
        .set_import_options(crate::edit_subscription::import_options_label(&folder.options).into());
    window.set_action_choices(strings(
        ACTION_CHOICES.iter().map(|&c| c.to_owned()).collect(),
    ));
    window.set_actions(action_rows(&folder));
    show_filename_tagging(&window, store, &folder);
    window.set_sidecars(sidecars_label(s.routers.len()).into());
    let state = Rc::new(RefCell::new(ImportEdit {
        folder,
        asking: None,
    }));
    let show = {
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            show_asking(
                state.borrow().asking.as_ref(),
                |asking, title, message, choices| {
                    window.set_asking(asking);
                    window.set_asking_title(title.into());
                    window.set_asking_message(message.into());
                    window.set_asking_choices(strings(choices));
                },
            );
        }
    };
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
    window.on_period_edited({
        let weak = window.as_weak();
        let state = state.clone();
        move |field, value| {
            let mut open = state.borrow_mut();
            let s = &mut open.folder.settings;
            // (at least three minutes)
            s.period = time_edited(s.period, &PERIOD_UNITS, field, value, 180);
            if let Some(window) = weak.upgrade() {
                window.set_period(time_fields(s.period, &PERIOD_UNITS));
            }
        }
    });
    window.on_skip_edited({
        let weak = window.as_weak();
        let state = state.clone();
        move |field, value| {
            let mut open = state.borrow_mut();
            let s = &mut open.folder.settings;
            s.last_modified_time_skip_period = time_edited(
                s.last_modified_time_skip_period,
                &SKIP_UNITS,
                field,
                value,
                1,
            );
            if let Some(window) = weak.upgrade() {
                window.set_skip(time_fields(s.last_modified_time_skip_period, &SKIP_UNITS));
            }
        }
    });
    window.on_action_chosen({
        let weak = window.as_weak();
        move |row, index| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let rows = window.get_actions();
            if let Ok(r) = usize::try_from(row)
                && let Some(mut data) = rows.row_data(r)
            {
                data.action = index;
                rows.set_row_data(r, data);
            }
        }
    });
    window.on_browse_path({
        let weak = window.as_weak();
        move || {
            if let (Some(window), Some(path)) = (weak.upgrade(), picked_folder()) {
                window.set_path(path.into());
            }
        }
    });
    window.on_browse_location({
        let weak = window.as_weak();
        move |row| {
            let (Some(window), Some(path)) = (weak.upgrade(), picked_folder()) else {
                return;
            };
            // (a new model, so the row's text box shows it though typed in)
            let mut rows: Vec<_> = window.get_actions().iter().collect();
            if let Some(data) = usize::try_from(row).ok().and_then(|r| rows.get_mut(r)) {
                data.location = path.into();
            }
            window.set_actions(ModelRc::new(VecModel::from(rows)));
        }
    });
    window.on_location_edited({
        let weak = window.as_weak();
        move |row, text| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let rows = window.get_actions();
            if let Ok(r) = usize::try_from(row)
                && let Some(mut data) = rows.row_data(r)
            {
                data.location = text;
                rows.set_row_data(r, data);
            }
        }
    });
    let finish = {
        let state = state.clone();
        let close = close.clone();
        let done = done.clone();
        move || {
            let folder = state.borrow().folder.clone();
            done(folder);
            close();
        }
    };
    // its filename tagging: a tag service's options added (once a
    // service), edited, or deleted
    let edit_tagging = {
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let tagging_slot = slots.filename_tagging.clone();
        move |key: String, options: hydrus_parse::folders::FilenameTagging| {
            if tagging_slot.borrow().is_some() {
                return;
            }
            let name = tag_services(&store)
                .into_iter()
                .find(|(k, _)| *k == key)
                .map_or_else(|| "unknown service".to_owned(), |(_, n)| n);
            let example = weak
                .upgrade()
                .map(|w| example_path(&w.get_path()))
                .unwrap_or_default();
            let done: Rc<dyn Fn(hydrus_parse::folders::FilenameTagging)> = {
                let weak = weak.clone();
                let state = state.clone();
                let store = store.clone();
                let key = key.clone();
                Rc::new(move |options| {
                    let mut state = state.borrow_mut();
                    let list = &mut state.folder.settings.filename_tagging;
                    match list.iter_mut().find(|(k, _)| *k == key) {
                        Some(entry) => entry.1 = options,
                        None => list.push((key.clone(), options)),
                    }
                    if let Some(window) = weak.upgrade() {
                        show_filename_tagging(&window, &store, &state.folder);
                    }
                })
            };
            match crate::filename_tagging_window::open_options(
                (key, name),
                options,
                example,
                &tagging_slot,
                done,
            ) {
                Ok(dialog) => *tagging_slot.borrow_mut() = Some(dialog),
                Err(e) => eprintln!("could not open the filename tagging options: {e}"),
            }
        }
    };
    window.on_tagging_add({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let show = show.clone();
        let edit_tagging = edit_tagging.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let services = tag_services(&store);
            let Some((key, _)) = usize::try_from(window.get_tagging_choice())
                .ok()
                .and_then(|i| services.get(i))
            else {
                return;
            };
            let exists = state
                .borrow()
                .folder
                .settings
                .filename_tagging
                .iter()
                .any(|(k, _)| k == key);
            if exists {
                state.borrow_mut().asking = Some(Asking::Messages(
                    vec![
                        "You already have an entry for that service key! Please try editing it instead!"
                            .into(),
                    ],
                    After::Nothing,
                ));
                show();
                return;
            }
            edit_tagging(key.clone(), hydrus_parse::folders::FilenameTagging::default());
        }
    });
    window.on_tagging_edit({
        let state = state.clone();
        move |i| {
            let entry = usize::try_from(i).ok().and_then(|i| {
                state
                    .borrow()
                    .folder
                    .settings
                    .filename_tagging
                    .get(i)
                    .cloned()
            });
            if let Some((key, options)) = entry {
                edit_tagging(key, options);
            }
        }
    });
    window.on_tagging_delete({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        move |i| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if let Ok(i) = usize::try_from(i)
                && i < state.folder.settings.filename_tagging.len()
            {
                state.folder.settings.filename_tagging.remove(i);
            }
            show_filename_tagging(&window, &store, &state.folder);
        }
    });
    // its import options, in the editor (an import folder's defaults)
    window.on_edit_import_options({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let editor_slot = slots.import_options.clone();
        move || {
            if editor_slot.borrow().is_some() {
                return;
            }
            let own = state.borrow().folder.options.clone();
            let done: Rc<dyn Fn(ImportOptionsSlice)> = {
                let weak = weak.clone();
                let state = state.clone();
                Rc::new(move |options| {
                    if let Some(window) = weak.upgrade() {
                        window.set_import_options(
                            crate::edit_subscription::import_options_label(&options).into(),
                        );
                    }
                    state.borrow_mut().folder.options = options;
                })
            };
            match crate::import_options_window::open(
                &store,
                hydrus_core::import_options::CallerType::LocalImportFolder,
                &own,
                &editor_slot,
                done,
            ) {
                Ok(editor) => *editor_slot.borrow_mut() = Some(editor),
                Err(e) => eprintln!("could not open the import options: {e}"),
            }
        }
    });
    window.on_apply({
        let weak = window.as_weak();
        let state = state.clone();
        let show = show.clone();
        let store = store.clone();
        let finish = finish.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            read_import_fields(&window, &mut state.borrow_mut().folder);
            let checked = check_import_folder(&state.borrow().folder, &sensitive(&store), &exists);
            match checked {
                Err(veto) => {
                    state.borrow_mut().asking = Some(Asking::Messages(vec![veto], After::Nothing));
                }
                Ok(warnings) if warnings.is_empty() => {
                    finish();
                    return;
                }
                Ok(warnings) => {
                    state.borrow_mut().asking = Some(Asking::Messages(warnings, After::Done));
                }
            }
            show();
        }
    });
    window.on_chosen({
        let state = state.clone();
        let show = show.clone();
        let finish = finish.clone();
        move |_| {
            let asking = state.borrow_mut().asking.take();
            if let Some(Asking::Messages(mut messages, after)) = asking {
                messages.remove(0);
                if !messages.is_empty() {
                    state.borrow_mut().asking = Some(Asking::Messages(messages, after));
                } else if let After::Done = after {
                    finish();
                    return;
                }
            }
            show();
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let show = show.clone();
        move || {
            state.borrow_mut().asking = None;
            show();
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

// export folders --------------------------------------------------------------

/// The export folders list's state while it is open.
struct ExportList {
    list: Named<ExportFolder>,
    asking: Option<Asking>,
}

fn text_context(store: &Store) -> TextContext {
    let snapshot = store.snapshot();
    let viewing = store.read(settings::get).unwrap_or_default();
    TextContext::from_store(&snapshot.services, &viewing)
}

fn predicates(folder: &ExportFolder, text: &TextContext) -> Vec<String> {
    folder
        .search
        .predicates
        .iter()
        .map(|p| predicate_text(p, text))
        .collect()
}

fn show_export_list(window: &FoldersWindow, open: &ExportList, text: &TextContext) {
    let rows = open
        .list
        .in_order()
        .into_iter()
        .map(|(key, f)| {
            (
                export_folder_row(f, &predicates(f, text)),
                open.list.selection.is_selected(key),
            )
        })
        .collect();
    window.set_rows(table(rows));
    window.set_any_selected(!open.list.selection.is_empty());
    window.set_one_selected(open.list.one_selected().is_some());
    show_asking(open.asking.as_ref(), |asking, title, message, choices| {
        window.set_asking(asking);
        window.set_asking_title(title.into());
        window.set_asking_message(message.into());
        window.set_asking_choices(strings(choices));
    });
}

/// Open the manage export folders dialog on the store's export folders.
pub(crate) fn open_export_folders(store: &Arc<Store>, slots: &Slots) -> Result<(), String> {
    if let Some(window) = slots.export_list.borrow().as_ref() {
        return window.show().map_err(|e| e.to_string());
    }
    let ExportFolders(folders) = store.read(settings::get).map_err(|e| e.to_string())?;
    let text = Rc::new(text_context(store));
    let state = Rc::new(RefCell::new(ExportList {
        list: Named::new(folders),
        asking: None,
    }));
    let window = FoldersWindow::new().map_err(|e| e.to_string())?;
    window.set_window_title("edit export folders".into());
    window.set_intro(EXPORT_INTRO.into());
    window.set_columns(columns(&EXPORT_COLUMNS));
    let change: Change<ExportList> = {
        let weak = window.as_weak();
        let state = state.clone();
        let text = text.clone();
        Rc::new(move |f: &dyn Fn(&mut ExportList)| {
            f(&mut state.borrow_mut());
            if let Some(window) = weak.upgrade() {
                show_export_list(&window, &state.borrow(), &text);
            }
        })
    };
    let close = {
        let weak = window.as_weak();
        let slots = slots.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slots.export_list.borrow_mut().take();
            if let Some(edit) = slots.export_edit.borrow_mut().take() {
                let _ = edit.hide();
            }
        }
    };
    let edit: Rc<dyn Fn(Option<u64>)> = {
        let state = state.clone();
        let change = change.clone();
        let slots = slots.clone();
        let store = store.clone();
        let text = text.clone();
        Rc::new(move |key: Option<u64>| {
            if slots.export_edit.borrow().is_some() {
                return;
            }
            let folder = if let Some(k) = key {
                let Some(f) = state.borrow().list.get(k).cloned() else {
                    return;
                };
                f
            } else {
                let phrase = store
                    .read(settings::get::<settings::ExportSettings>)
                    .unwrap_or_default()
                    .phrase;
                let location = store
                    .read(settings::get::<settings::SearchDefaults>)
                    .unwrap_or_default()
                    .local_location;
                new_export_folder(
                    phrase,
                    hydrus_core::search::context::FileSearchContext {
                        location,
                        ..Default::default()
                    },
                )
            };
            let done: Rc<dyn Fn(ExportFolder)> = {
                let change = change.clone();
                Rc::new(move |edited: ExportFolder| {
                    change(&|open| match key {
                        Some(k) => open.list.replace(k, edited.clone()),
                        None => {
                            open.list.add(edited.clone());
                        }
                    });
                })
            };
            match open_export_folder(folder, &text, &slots.export_edit, &done) {
                Ok(window) => *slots.export_edit.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the export folder: {e}"),
            }
        })
    };
    bind_list(
        &window,
        &change,
        |open: &mut ExportList| &mut open.asking,
        |open: &mut ExportList, row, ctrl, shift| open.list.click(row, ctrl, shift),
        |open: &ExportList| open.list.one_selected(),
        |open: &ExportList, row| open.list.order().get(row).copied(),
        |open: &mut ExportList| {
            open.list.delete_selected();
        },
        &edit,
        &state,
    );
    window.on_apply({
        let state = state.clone();
        let store = store.clone();
        let close = close.clone();
        move || {
            let list = std::mem::replace(&mut state.borrow_mut().list, Named::new(Vec::new()));
            let folders = ExportFolders(list.into_items());
            if let Err(e) = store.write(move |ctx| settings::set(ctx.conn(), &folders)) {
                eprintln!("could not save the export folders: {e}");
            }
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
    show_export_list(&window, &state.borrow(), &text);
    window.show().map_err(|e| e.to_string())?;
    *slots.export_list.borrow_mut() = Some(window);
    Ok(())
}

/// The export folder edit dialog's state while it is open.
struct ExportEdit {
    folder: ExportFolder,
    asking: Option<Asking>,
}

fn read_export_fields(window: &ExportFolderWindow, folder: &mut ExportFolder) {
    folder.name = window.get_name().into();
    folder.path = window.get_path().into();
    folder.export_type = if window.get_export_type() == 1 {
        ExportType::Synchronise
    } else {
        ExportType::Regular
    };
    folder.delete_from_client_after_export =
        folder.export_type == ExportType::Regular && window.get_delete_from_client();
    folder.export_symlinks = window.get_symlinks();
    folder.run_regularly = window.get_run_regularly();
    folder.show_working_popup = window.get_show_popup();
    folder.run_now = window.get_run_now();
    folder.phrase = window.get_phrase().into();
    folder.overwrite_sidecars_on_next_run = window.get_overwrite_next();
    folder.always_overwrite_sidecars = window.get_overwrite_always();
}

/// Open the edit export folder dialog on a folder; on "apply" (once it
/// checks out, and asking first if it deletes from the client) it gives
/// the edited folder to `done`.
fn open_export_folder(
    folder: ExportFolder,
    text: &Rc<TextContext>,
    slot: &Rc<RefCell<Option<ExportFolderWindow>>>,
    done: &Rc<dyn Fn(ExportFolder)>,
) -> Result<ExportFolderWindow, String> {
    let window = ExportFolderWindow::new().map_err(|e| e.to_string())?;
    window.set_types_text(EXPORT_TYPES_TEXT.into());
    window.set_sidecars_text(SIDECARS_TEXT.into());
    window.set_name(folder.name.clone().into());
    window.set_path(folder.path.clone().into());
    window.on_browse_path({
        let weak = window.as_weak();
        move || {
            if let (Some(window), Some(path)) = (weak.upgrade(), picked_folder()) {
                window.set_path(path.into());
            }
        }
    });
    window.set_export_type(i32::from(folder.export_type == ExportType::Synchronise));
    window.set_delete_from_client(folder.delete_from_client_after_export);
    window.set_symlinks(folder.export_symlinks);
    window.set_run_regularly(folder.run_regularly);
    window.set_period(time_fields(folder.period, &PERIOD_UNITS));
    window.set_show_popup(folder.show_working_popup);
    window.set_run_now(folder.run_now);
    window.set_phrase(folder.phrase.clone().into());
    window.set_overwrite_next(folder.overwrite_sidecars_on_next_run);
    window.set_overwrite_always(folder.always_overwrite_sidecars);
    window.set_sidecars(sidecars_label(folder.routers.len()).into());
    window.set_predicates(strings(predicates(&folder, text)));
    let state = Rc::new(RefCell::new(ExportEdit {
        folder,
        asking: None,
    }));
    let show = {
        let weak = window.as_weak();
        let state = state.clone();
        let text = text.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let open = state.borrow();
            window.set_predicates(strings(predicates(&open.folder, &text)));
            show_asking(open.asking.as_ref(), |asking, title, message, choices| {
                window.set_asking(asking);
                window.set_asking_title(title.into());
                window.set_asking_message(message.into());
                window.set_asking_choices(strings(choices));
            });
        }
    };
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
    window.on_type_chosen({
        let weak = window.as_weak();
        move |index| {
            if let Some(window) = weak.upgrade() {
                window.set_export_type(index);
                // (synchronising never deletes from the client)
                if index == 1 {
                    window.set_delete_from_client(false);
                }
            }
        }
    });
    window.on_delete_toggled({
        let weak = window.as_weak();
        let state = state.clone();
        let show = show.clone();
        move || {
            if weak.upgrade().is_some_and(|w| w.get_delete_from_client()) {
                state.borrow_mut().asking = Some(Asking::Messages(
                    vec![DELETE_WARNING.into()],
                    After::Nothing,
                ));
                show();
            }
        }
    });
    window.on_period_edited({
        let weak = window.as_weak();
        let state = state.clone();
        move |field, value| {
            let mut open = state.borrow_mut();
            let folder = &mut open.folder;
            // (at least three minutes)
            folder.period = time_edited(folder.period, &PERIOD_UNITS, field, value, 180);
            if let Some(window) = weak.upgrade() {
                window.set_period(time_fields(folder.period, &PERIOD_UNITS));
            }
        }
    });
    window.on_typed_accepted({
        let weak = window.as_weak();
        let state = state.clone();
        let show = show.clone();
        let text = text.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let typed = window.get_typed().trim().to_owned();
            if typed.is_empty() {
                return;
            }
            match parse_api_search(&serde_json::json!([typed])) {
                Ok(parsed) => {
                    hydrus_search::enter_predicates(
                        &mut state.borrow_mut().folder.search.predicates,
                        &parsed,
                        &text,
                    );
                    window.set_typed(SharedString::new());
                    window.set_error(SharedString::new());
                }
                Err(e) => window.set_error(e.to_string().into()),
            }
            show();
        }
    });
    window.on_predicate_removed({
        let state = state.clone();
        let show = show.clone();
        move |i| {
            if let Ok(i) = usize::try_from(i) {
                let predicates = &mut state.borrow_mut().folder.search.predicates;
                if i < predicates.len() {
                    predicates.remove(i);
                }
            }
            show();
        }
    });
    // "apply": checked, then given back
    let check = {
        let state = state.clone();
        let show = show.clone();
        let close = close.clone();
        let done = done.clone();
        move || {
            let checked = check_export_folder(&state.borrow().folder);
            if let Err(veto) = checked {
                state.borrow_mut().asking = Some(Asking::Messages(vec![veto], After::Nothing));
                show();
            } else {
                let folder = state.borrow().folder.clone();
                done(folder);
                close();
            }
        }
    };
    window.on_apply({
        let weak = window.as_weak();
        let state = state.clone();
        let show = show.clone();
        let check = check.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            read_export_fields(&window, &mut state.borrow_mut().folder);
            if state.borrow().folder.delete_from_client_after_export {
                state.borrow_mut().asking = Some(Asking::ConfirmDelete);
                show();
            } else {
                check();
            }
        }
    });
    window.on_chosen({
        let state = state.clone();
        let show = show.clone();
        let check = check.clone();
        move |index| {
            let asking = state.borrow_mut().asking.take();
            match asking {
                Some(Asking::ConfirmDelete) => {
                    if index == 0 {
                        check();
                        return;
                    }
                }
                Some(Asking::Messages(mut messages, after)) => {
                    messages.remove(0);
                    if !messages.is_empty() {
                        state.borrow_mut().asking = Some(Asking::Messages(messages, after));
                    }
                }
                _ => {}
            }
            show();
        }
    });
    window.on_cancelled({
        let state = state.clone();
        let show = show.clone();
        move || {
            state.borrow_mut().asking = None;
            show();
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
