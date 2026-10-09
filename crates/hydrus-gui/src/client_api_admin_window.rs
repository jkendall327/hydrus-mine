//! Native access-key review and detached permission editors, owned by service review.
use crate::{
    ApiPermissionRow, ApiRequestWindow, ClientApiKeysWindow, EditApiPermissionsWindow, TableColumn,
    TableRow,
};
use hydrus_core::ServiceKey;
use hydrus_gui_model::client_api_admin::{self as model, Editor};
use hydrus_store::{
    Store,
    api_permissions::{AccessPermissions, Permission},
    services::ServiceKind,
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

/// Child windows retained by the service review owner.
#[derive(Clone, Default)]
pub struct Slots {
    pub keys: Rc<RefCell<Option<ClientApiKeysWindow>>>,
    pub edit: Rc<RefCell<Option<EditApiPermissionsWindow>>>,
    /// "waiting for API access permissions request", while it is open.
    pub request: Rc<RefCell<Option<Waiting>>>,
    pub filter: crate::tag_filter_window::Slot,
}

/// The window waiting for a tool to ask for an access key, and the timer
/// that looks for its request.
pub struct Waiting {
    window: ApiRequestWindow,
    _timer: slint::Timer,
}
impl std::fmt::Debug for Waiting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Waiting").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots").finish_non_exhaustive()
    }
}
thread_local! {
    static LAST: RefCell<Option<slint::Weak<ClientApiKeysWindow>>> = const { RefCell::new(None) };
    static LAST_EDIT: RefCell<Option<slint::Weak<EditApiPermissionsWindow>>> = const { RefCell::new(None) };
    static LAST_REQUEST: RefCell<Option<slint::Weak<ApiRequestWindow>>> = const { RefCell::new(None) };
}
/// The window waiting for an API permissions request, for interaction tests.
pub fn last_request_opened() -> Option<ApiRequestWindow> {
    LAST_REQUEST
        .with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}

/// How long "add from api request" keeps registration open at a time. The
/// reference's flag lasts only while its dialog is open; this lease is
/// renewed by the window's timer, so a crash closes registration within
/// seconds instead of leaving it open (the `hydrus api-keys listen` CLI
/// shares the same row).
const REGISTRATION_LEASE_MS: i64 = 10_000;

/// Push the lease forward while registration is open and running out.
fn renew_registration(store: &Store) {
    let now = hydrus_core::time::TimestampMs::now().millis();
    let Ok(registration) =
        store.read(hydrus_store::settings::get::<hydrus_store::api_permissions::Registration>)
    else {
        return;
    };
    if registration
        .open_until_ms
        .is_none_or(|until| until - now >= REGISTRATION_LEASE_MS / 2)
    {
        return;
    }
    let _ = store.write(move |ctx| {
        let mut registration: hydrus_store::api_permissions::Registration =
            hydrus_store::settings::get(ctx.conn())?;
        if registration.open_until_ms.is_some() {
            registration.open_until_ms = Some(now + REGISTRATION_LEASE_MS);
            hydrus_store::settings::set(ctx.conn(), &registration)?;
        }
        Ok(())
    });
}

/// Stop taking requests (the reference's `api_request_dialog_open = False`).
fn close_registration(store: &Store) {
    if let Err(e) = store.write(|ctx| {
        let mut registration: hydrus_store::api_permissions::Registration =
            hydrus_store::settings::get(ctx.conn())?;
        registration.open_until_ms = None;
        registration.requests.clear();
        hydrus_store::settings::set(ctx.conn(), &registration)
    }) {
        eprintln!("could not stop taking API permission requests: {e}");
    }
}
/// Current keys window, exposed for interaction tests.
pub fn last_opened() -> Option<ClientApiKeysWindow> {
    LAST.with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
/// Current permission editor, exposed for interaction tests.
pub fn last_edit_opened() -> Option<EditApiPermissionsWindow> {
    LAST_EDIT
        .with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
impl Slots {
    /// Cancel all nested windows when their service-review owner closes.
    pub fn close(&self) {
        let keys = self
            .keys
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong);
        if let Some(w) = keys {
            w.invoke_cancel_clicked();
        }
    }
}
fn show(window: &ClientApiKeysWindow, editor: &Editor) {
    window.set_sort_column(i32::try_from(editor.sort_column).unwrap_or(0));
    window.set_ascending(editor.ascending);
    window.set_rows(ModelRc::new(VecModel::from(
        editor
            .rows
            .iter()
            .map(|r| TableRow {
                cells: ModelRc::new(VecModel::from(r.cells().map(SharedString::from).to_vec())),
                selected: editor.selection.is_selected(r.token),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_can_edit(editor.selected().is_some());
    window.set_can_delete(!editor.selection.is_empty());
}
fn show_permissions(window: &EditApiPermissionsWindow, key: &AccessPermissions) {
    let mut permissions = Permission::ALL.to_vec();
    permissions.sort_by_key(|p| p.description());
    window.set_permissions(ModelRc::new(VecModel::from(
        permissions
            .into_iter()
            .map(|p| ApiPermissionRow {
                code: i32::from(u8::from(p)),
                label: p.description().into(),
                checked: key.basic.contains(&p),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_can_check_all(
        !window.get_permits_everything() && key.basic.len() < Permission::ALL.len(),
    );
    window.set_can_filter(
        !window.get_permits_everything() && key.basic.contains(&Permission::SearchFiles),
    );
    window.set_filter_description(
        format!(
            "permitted tags: {}",
            key.search_filter.to_permitted_string()
        )
        .into(),
    );
}
fn edit(
    store: Arc<Store>,
    key: AccessPermissions,
    slots: &Slots,
    parent_active: Rc<Cell<bool>>,
    done: Rc<dyn Fn(AccessPermissions) -> Result<(), String>>,
    closed: Rc<dyn Fn()>,
) -> Result<(), String> {
    if let Some(w) = slots.edit.borrow().as_ref() {
        w.show().map_err(|e| e.to_string())?;
        return Ok(());
    }
    let window = EditApiPermissionsWindow::new().map_err(|e| e.to_string())?;
    window.set_access_key(hex::encode(&key.access_key).into());
    window.set_name(key.name.as_str().into());
    window.set_permits_everything(key.permits_everything);
    window.set_key_warning(model::CHANGE_KEY_WARNING.into());
    show_permissions(&window, &key);
    let key = Rc::new(RefCell::new(key));
    let active = Rc::new(Cell::new(true));
    let close = Rc::new({
        let weak = window.as_weak();
        let slots = slots.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let filter = slots
                .filter
                .borrow()
                .as_ref()
                .map(ComponentHandle::clone_strong);
            if let Some(filter) = filter {
                filter.invoke_cancel();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slots.edit.borrow_mut().take();
            closed();
        }
    });
    window.on_permission_toggled({
        let key = key.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move |code, checked| {
            if !active.get() {
                return;
            }
            if let (Some(w), Ok(code)) = (weak.upgrade(), u8::try_from(code))
                && let Ok(permission) = Permission::try_from(code)
            {
                if checked {
                    key.borrow_mut().basic.insert(permission);
                } else {
                    key.borrow_mut().basic.remove(&permission);
                }
                show_permissions(&w, &key.borrow());
            }
        }
    });
    window.on_full_toggled({
        let key = key.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(w) = weak.upgrade()
            {
                show_permissions(&w, &key.borrow());
            }
        }
    });
    window.on_check_all({
        let key = key.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(w) = weak.upgrade()
            {
                key.borrow_mut().basic = Permission::ALL.into();
                show_permissions(&w, &key.borrow());
            }
        }
    });
    window.on_change_key({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                w.set_new_key(w.get_access_key());
                w.set_changing_key(true);
            }
        }
    });
    window.on_generate_key({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                w.set_new_key(hex::encode(rand::random::<[u8; 32]>()).into());
            }
        }
    });
    window.on_accept_key({
        let weak = window.as_weak();
        let key = key.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                match model::parse_key(&w.get_new_key()) {
                    Ok(bytes) => {
                        key.borrow_mut().access_key = bytes;
                        w.set_access_key(hex::encode(&key.borrow().access_key).into());
                        w.set_changing_key(false);
                        w.set_error(SharedString::new());
                    }
                    Err(e) => w.set_error(e.into()),
                }
            }
        }
    });
    window.on_cancel_key({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                w.set_changing_key(false);
            }
        }
    });
    window.on_edit_filter({
        let weak = window.as_weak();
        let key = key.clone();
        let active = active.clone();
        let parent_active = parent_active.clone();
        let slot = slots.filter.clone();
        move || {
            if !active.get() || !parent_active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if !w.get_can_filter() || w.get_filtering() {
                return;
            }
            let applied = Rc::new({
                let weak = weak.clone();
                let key = key.clone();
                let active = active.clone();
                let parent_active = parent_active.clone();
                move |filter| {
                    if active.get()
                        && parent_active.get()
                        && let Some(w) = weak.upgrade()
                    {
                        key.borrow_mut().search_filter = filter;
                        show_permissions(&w, &key.borrow());
                    }
                }
            });
            let filter = key.borrow().search_filter.clone();
            match crate::tag_filter_window::open(
                &store,
                &filter,
                false,
                "tag search permissions",
                "The API will only permit searching for tags that pass through this filter.",
                &slot,
                applied,
            ) {
                Ok(filter) => {
                    w.set_filtering(true);
                    filter.on_closed({
                        let weak = weak.clone();
                        move || {
                            if let Some(w) = weak.upgrade() {
                                w.set_filtering(false);
                            }
                        }
                    });
                    *slot.borrow_mut() = Some(filter);
                }
                Err(e) => w.set_error(e.to_string().into()),
            }
        }
    });
    window.on_apply_clicked({
        let weak = window.as_weak();
        let key = key.clone();
        let close = close.clone();
        let active = active.clone();
        move || {
            if !active.get() || !parent_active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if w.get_filtering() || w.get_changing_key() {
                return;
            }
            let mut edited = key.borrow().clone();
            edited.name = w.get_name().to_string();
            edited.permits_everything = w.get_permits_everything();
            match done(edited) {
                Ok(()) => close(),
                Err(e) => w.set_error(e.into()),
            }
        }
    });
    window.on_cancel_clicked({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    LAST_EDIT.with(|s| *s.borrow_mut() = Some(window.as_weak()));
    *slots.edit.borrow_mut() = Some(window);
    Ok(())
}
/// Open detached native key administration for the selected Client API service.
pub fn open(
    store: Arc<Store>,
    service_key: ServiceKey,
    slots: &Slots,
) -> Result<ClientApiKeysWindow, String> {
    if let Some(w) = slots.keys.borrow().as_ref() {
        w.show().map_err(|e| e.to_string())?;
        return Ok(w.clone_strong());
    }
    let window = ClientApiKeysWindow::new().map_err(|e| e.to_string())?;
    let editor = Rc::new(RefCell::new(
        Editor::new(&store).map_err(|e| e.to_string())?,
    ));
    let active = Rc::new(Cell::new(true));
    window.set_columns(ModelRc::new(VecModel::from(
        model::COLUMNS
            .iter()
            .enumerate()
            .map(|(i, t)| TableColumn {
                title: (*t).into(),
                width: if i == 0 { 170.0 } else { 350.0 },
                stretch: i == 1,
            })
            .collect::<Vec<_>>(),
    )));
    let config = store
        .snapshot()
        .services
        .by_key(&service_key)
        .map_err(|e| e.to_string())?
        .kind
        .clone();
    if let ServiceKind::ClientApi(c) = &config {
        window.set_status(match c.port {
            None => "The client api is not running.".into(),
            Some(port) => format!("The client api should be running on port {port}.").into(),
        });
    }
    if let Ok(status) =
        store.read(hydrus_store::settings::get::<hydrus_store::settings::ClientApiStatus>)
    {
        window.set_status(
            format!(
                "Configured service: {}\nDaemon: {:?}",
                window.get_status(),
                status.state
            )
            .into(),
        );
    }
    show(&window, &editor.borrow());
    let close = Rc::new({
        let slots = slots.clone();
        let weak = window.as_weak();
        let active = active.clone();
        let store = store.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let edit = slots
                .edit
                .borrow()
                .as_ref()
                .map(ComponentHandle::clone_strong);
            if let Some(edit) = edit {
                edit.invoke_cancel_clicked();
            }
            let waiting = slots.request.borrow_mut().take();
            if let Some(waiting) = waiting {
                close_registration(&store);
                let _ = waiting.window.hide();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slots.keys.borrow_mut().take();
        }
    });
    window.on_row_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move |i, c, s| {
            if active.get()
                && let (Some(w), Ok(i)) = (weak.upgrade(), usize::try_from(i))
            {
                if w.get_editing() || !w.get_question().is_empty() {
                    return;
                }
                let order = editor.borrow().order();
                editor.borrow_mut().selection.click(&order, i, c, s);
                show(&w, &editor.borrow());
            }
        }
    });
    window.on_sort({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move |i, a| {
            if active.get()
                && let (Some(w), Ok(i)) = (weak.upgrade(), usize::try_from(i))
            {
                editor.borrow_mut().sort(i, a);
                show(&w, &editor.borrow());
            }
        }
    });
    let open_edit = Rc::new({
        let slots = slots.clone();
        let editor = editor.clone();
        let weak = window.as_weak();
        let active = active.clone();
        let store = store.clone();
        move |adding: bool, preset: Option<AccessPermissions>| {
            if !active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if w.get_editing() || !w.get_question().is_empty() {
                return;
            }
            let (token, key) = if adding {
                (
                    None,
                    preset.unwrap_or_else(|| AccessPermissions {
                        access_key: rand::random::<[u8; 32]>().to_vec(),
                        name: "new api permissions".into(),
                        permits_everything: true,
                        basic: std::collections::BTreeSet::default(),
                        search_filter: hydrus_core::TagFilter::default(),
                    }),
                )
            } else {
                let Some(row) = editor.borrow().selected().cloned() else {
                    return;
                };
                (Some(row.token), row.permissions)
            };
            let done = Rc::new({
                let editor = editor.clone();
                let weak = weak.clone();
                let active = active.clone();
                move |key| {
                    if !active.get() {
                        return Err("The access key list has closed.".into());
                    }
                    editor.borrow_mut().edit(token, key)?;
                    if let Some(w) = weak.upgrade() {
                        show(&w, &editor.borrow());
                    }
                    Ok(())
                }
            });
            let closed = Rc::new({
                let weak = weak.clone();
                move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_editing(false);
                    }
                }
            });
            match edit(store.clone(), key, &slots, active.clone(), done, closed) {
                Ok(()) => w.set_editing(true),
                Err(e) => w.set_error(e.into()),
            }
        }
    });
    window.on_add_clicked({
        let open_edit = open_edit.clone();
        move || open_edit(true, None)
    });
    // "add > from api request": wait for a tool to ask (`_AddFromAPI`), then
    // edit what it asked for
    window.on_add_from_api_clicked({
        let open_edit = open_edit.clone();
        let weak = window.as_weak();
        let store = store.clone();
        let slots = slots.clone();
        let active = active.clone();
        let service_key = service_key.clone();
        move || {
            if !active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else { return };
            if w.get_editing() || !w.get_question().is_empty() {
                return;
            }
            let port = store
                .snapshot()
                .services
                .by_key(&service_key)
                .ok()
                .and_then(|s| match &s.kind {
                    ServiceKind::ClientApi(c) => c.port,
                    _ => None,
                });
            if port.is_none() {
                w.set_error(
                    "The service is not running, so you cannot add new access via the API!".into(),
                );
                return;
            }
            let Ok(waiting) = ApiRequestWindow::new() else {
                return;
            };
            let until = hydrus_core::time::TimestampMs::now().millis() + REGISTRATION_LEASE_MS;
            if let Err(e) = store.write(move |ctx| {
                let mut registration: hydrus_store::api_permissions::Registration =
                    hydrus_store::settings::get(ctx.conn())?;
                registration.open_until_ms = Some(until);
                registration.requests.clear();
                hydrus_store::settings::set(ctx.conn(), &registration)
            }) {
                w.set_error(e.to_string().into());
                return;
            }
            let finish = Rc::new({
                let slot = Rc::downgrade(&slots.request);
                let weak = weak.clone();
                let store = store.clone();
                move || {
                    close_registration(&store);
                    if let Some(slot) = slot.upgrade() {
                        let waiting = slot.borrow_mut().take();
                        if let Some(waiting) = waiting {
                            let _ = waiting.window.hide();
                        }
                    }
                    if let Some(w) = weak.upgrade() {
                        w.set_editing(false);
                    }
                }
            });
            waiting.on_cancel_clicked({
                let finish = finish.clone();
                move || finish()
            });
            waiting.window().on_close_requested({
                let finish = finish.clone();
                move || {
                    finish();
                    slint::CloseRequestResponse::HideWindow
                }
            });
            let timer = slint::Timer::default();
            timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(500),
                {
                    let store = store.clone();
                    let finish = finish.clone();
                    let open_edit = open_edit.clone();
                    let slot = Rc::downgrade(&slots.request);
                    move || {
                        renew_registration(&store);
                        let Ok(registration) = store.read(
                            hydrus_store::settings::get::<
                                hydrus_store::api_permissions::Registration,
                            >,
                        ) else {
                            return;
                        };
                        let Some((key, name, everything, basic)) =
                            registration.requests.first().cloned()
                        else {
                            return;
                        };
                        let Ok(access_key) = hex::decode(&key) else {
                            return;
                        };
                        // (the reference stops listening on the first request)
                        finish();
                        if slot.upgrade().is_none() {
                            return;
                        }
                        crate::debug_actions::message("Information", "Got request!");
                        open_edit(
                            true,
                            Some(AccessPermissions {
                                access_key,
                                name,
                                permits_everything: everything,
                                basic: basic.into_iter().collect(),
                                search_filter: hydrus_core::TagFilter::default(),
                            }),
                        );
                    }
                },
            );
            if waiting.show().is_ok() {
                LAST_REQUEST.with(|s| *s.borrow_mut() = Some(waiting.as_weak()));
                w.set_editing(true);
                *slots.request.borrow_mut() = Some(Waiting {
                    window: waiting,
                    _timer: timer,
                });
            }
        }
    });
    window.on_edit_clicked(move || open_edit(false, None));
    window.on_duplicate_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(w) = weak.upgrade()
            {
                if w.get_editing() || !w.get_question().is_empty() {
                    return;
                }
                if let Err(e) = editor
                    .borrow_mut()
                    .duplicate(|| rand::random::<[u8; 32]>().to_vec())
                {
                    w.set_error(e.into());
                }
                show(&w, &editor.borrow());
            }
        }
    });
    window.on_delete_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(w) = weak.upgrade()
                && !editor.borrow().selection.is_empty()
                && !w.get_editing()
            {
                w.set_question(model::DELETE_QUESTION.into());
            }
        }
    });
    window.on_answered({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move |yes| {
            if active.get()
                && let Some(w) = weak.upgrade()
                && !w.get_question().is_empty()
            {
                if yes {
                    editor.borrow_mut().delete_selected();
                }
                w.set_question(SharedString::new());
                show(&w, &editor.borrow());
            }
        }
    });
    window.on_copy_key({
        let editor = editor.clone();
        move || {
            if let Some(row) = editor.borrow().selected() {
                crate::copy_to_clipboard(&hex::encode(&row.permissions.access_key));
            }
        }
    });
    window.on_open_base_url({
        let weak = window.as_weak();
        let store = store.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                let snap = store.snapshot();
                let result = snap
                    .services
                    .by_key(&service_key)
                    .map_err(|e| e.to_string())
                    .and_then(|s| {
                        if let ServiceKind::ClientApi(c) = &s.kind {
                            let status = store
                                .read(
                                    hydrus_store::settings::get::<
                                        hydrus_store::settings::ClientApiStatus,
                                    >,
                                )
                                .map_err(|e| e.to_string())?;
                            model::reported_base_url(c, &status.state)
                        } else {
                            Err("The selected service is no longer a Client API.".into())
                        }
                    });
                match result {
                    Ok(url) => crate::launch(&url),
                    Err(e) => w.set_error(e.into()),
                }
            }
        }
    });
    window.on_apply_clicked({
        let weak = window.as_weak();
        let editor = editor.clone();
        let close = close.clone();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(w) = weak.upgrade()
            {
                if w.get_editing() || !w.get_question().is_empty() {
                    return;
                }
                match editor.borrow().apply(&store) {
                    Ok(()) => close(),
                    Err(e) => w.set_error(e.to_string().into()),
                }
            }
        }
    });
    window.on_cancel_clicked({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show().map_err(|e| e.to_string())?;
    LAST.with(|s| *s.borrow_mut() = Some(window.as_weak()));
    *slots.keys.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
