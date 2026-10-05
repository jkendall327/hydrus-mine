//! Session browser and detached cookie/header editors over native network storage.
use crate::{CookieImportWindow, EditNetworkValueWindow, NetworkDataWindow, TableColumn, TableRow};
use hydrus_gui_model::{
    list_selection::ListSelection,
    network_sessions::{self as model, CookieDraft, HeaderDraft, HeaderRow},
};
use hydrus_store::{
    Store,
    network::{self, Approval, Cookie, NetworkContext},
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

/// Retain windows for a bounded main-window lifetime. Callbacks hold weak owners.
#[derive(Clone, Default)]
pub struct Slots {
    owner: Rc<SlotsOwner>,
}
/// Windows owned by the final `Slots` handle; dropping it cancels all drafts.
#[derive(Default)]
pub struct SlotsOwner {
    pub browser: Rc<RefCell<Option<NetworkDataWindow>>>,
    pub cookies: Rc<RefCell<Option<NetworkDataWindow>>>,
    pub headers: Rc<RefCell<Option<NetworkDataWindow>>>,
}
impl std::ops::Deref for Slots {
    type Target = SlotsOwner;
    fn deref(&self) -> &Self::Target {
        &self.owner
    }
}
impl Drop for SlotsOwner {
    fn drop(&mut self) {
        for slot in [&self.browser, &self.cookies, &self.headers] {
            let window = slot.borrow().as_ref().map(ComponentHandle::clone_strong);
            if let Some(window) = window {
                window.invoke_cancel_clicked();
            }
        }
    }
}
impl std::fmt::Debug for SlotsOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotsOwner").finish_non_exhaustive()
    }
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots").finish_non_exhaustive()
    }
}
thread_local! {
    static LAST_IMPORT: RefCell<Option<slint::Weak<CookieImportWindow>>> = const { RefCell::new(None) };
    static LAST_EDIT: RefCell<Option<slint::Weak<EditNetworkValueWindow>>> = const { RefCell::new(None) };
}
/// Most recently opened child editor, for interaction tests.
pub fn last_edit_opened() -> Option<EditNetworkValueWindow> {
    LAST_EDIT
        .with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
/// Most recently opened import choice/confirmation dialog, for interaction tests.
pub fn last_import_opened() -> Option<CookieImportWindow> {
    LAST_IMPORT
        .with(|s| s.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}
fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}
#[derive(Clone)]
enum Data {
    Sessions(Vec<(NetworkContext, Vec<Cookie>)>),
    Cookies(CookieDraft),
    Headers(HeaderDraft),
}
impl Data {
    fn cells(&self, formatting: &hydrus_store::settings::GuiFormatting) -> Vec<Vec<String>> {
        match self {
            Self::Sessions(rows) => rows
                .iter()
                .map(|(s, c)| model::session_cells_with_format(s, c, now(), formatting))
                .collect(),
            Self::Cookies(d) => d
                .cookies
                .iter()
                .map(|c| model::cookie_cells_with_format(c, now(), formatting))
                .collect(),
            Self::Headers(d) => d.rows.iter().map(model::header_cells).collect(),
        }
    }
    fn compare(&self, a: usize, b: usize, column: usize) -> Option<std::cmp::Ordering> {
        match self {
            Self::Sessions(rows) if column == 1 => Some(rows[a].1.len().cmp(&rows[b].1.len())),
            Self::Sessions(rows) if column == 2 => {
                let expiry = |i: usize| {
                    rows[i]
                        .1
                        .iter()
                        .filter_map(|c| c.expires)
                        .max()
                        .unwrap_or(if rows[i].1.is_empty() { -1 } else { 0 })
                };
                Some(expiry(a).cmp(&expiry(b)))
            }
            Self::Cookies(d) if column == 4 => {
                Some(d.cookies[a].expires.cmp(&d.cookies[b].expires))
            }
            Self::Cookies(d) if column == 5 => Some(d.cookies[a].secure.cmp(&d.cookies[b].secure)),
            _ => None,
        }
    }
}
struct State {
    store: Arc<Store>,
    data: Data,
    selection: ListSelection<usize>,
    order: Vec<usize>,
    column: usize,
    ascending: bool,
}
fn refresh(window: &NetworkDataWindow, state: &mut State) {
    let cells = state
        .data
        .cells(&hydrus_gui_model::gui_format::preferences(&state.store));
    let filter = window.get_filter().to_lowercase();
    state.order = (0..cells.len())
        .filter(|&i| {
            cells[i].iter().any(|c| c.to_lowercase().contains(&filter))
                && (!window.get_browser() || window.get_show_empty() || cells[i][1] != "0")
        })
        .collect();
    state.order.sort_by(|&a, &b| {
        let ord = state
            .data
            .compare(a, b, state.column)
            .unwrap_or_else(|| cells[a][state.column].cmp(&cells[b][state.column]));
        if state.ascending { ord } else { ord.reverse() }
    });
    window.set_rows(ModelRc::new(VecModel::from(
        state
            .order
            .iter()
            .map(|&i| TableRow {
                cells: ModelRc::new(VecModel::from(
                    cells[i].iter().map(SharedString::from).collect::<Vec<_>>(),
                )),
                selected: state.selection.is_selected(i),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_sort_column(i32::try_from(state.column).unwrap_or(0));
    window.set_ascending(state.ascending);
    window.set_can_edit(
        state
            .selection
            .one()
            .is_some_and(|i| state.order.contains(&i)),
    );
    window.set_can_delete(!state.selection.in_order(&state.order).is_empty());
}
fn sessions(store: &Store) -> Result<Data, String> {
    store
        .read(|c| {
            network::sessions(c)?
                .into_iter()
                .map(|s| Ok((s.clone(), network::cookies(c, &s)?)))
                .collect()
        })
        .map(Data::Sessions)
        .map_err(|e| e.to_string())
}
fn columns(titles: &[&str]) -> ModelRc<TableColumn> {
    ModelRc::new(VecModel::from(
        titles
            .iter()
            .map(|&s| TableColumn {
                title: s.into(),
                width: match s {
                    "value" | "header" | "current state" => 260.,
                    "network context" => 180.,
                    "expires" => 220.,
                    "secure" => 60.,
                    "cookies" => 80.,
                    "path" => 100.,
                    _ => 145.,
                },
                stretch: s == "header" || s == "value" || s == "network context",
            })
            .collect::<Vec<_>>(),
    ))
}

/// Open the session browser (`false`) or HTTP-header manager (`true`).
pub fn open(store: &Arc<Store>, slots: &Slots, headers: bool) -> Result<NetworkDataWindow, String> {
    let slot = if headers {
        slots.headers.clone()
    } else {
        slots.browser.clone()
    };
    if let Some(w) = slot.borrow().as_ref() {
        w.show().map_err(|e| e.to_string())?;
        return Ok(w.clone_strong());
    }
    let data = if headers {
        Data::Headers(HeaderDraft::new(store).map_err(|e| e.to_string())?)
    } else {
        sessions(store)?
    };
    open_data(store, slots, &slot, data)
}
fn open_data(
    store: &Arc<Store>,
    slots: &Slots,
    slot: &Rc<RefCell<Option<NetworkDataWindow>>>,
    data: Data,
) -> Result<NetworkDataWindow, String> {
    let window = NetworkDataWindow::new().map_err(|e| e.to_string())?;
    let (title, headings, description) = match &data {
        Data::Sessions(_) => ("review session cookies", vec!["network context", "cookies", "expires"], "Network sessions keep cookies in separate domain silos. Select a session to review its cookies.".into()),
        Data::Cookies(d) => ("review network session", vec!["name", "value", "domain", "path", "expires", "secure"], format!("{}\nCookies for another domain in this silo will not be sent there. Changes are saved when you apply.", d.session.to_human_string())),
        Data::Headers(_) => ("manage http headers", vec!["network context", "header", "current state", "reason"], "Global and domain headers are saved when you apply. Only approved headers are sent.".into()),
    };
    window.set_window_title(title.into());
    window.set_columns(columns(&headings));
    window.set_description(description.into());
    window.set_browser(matches!(&data, Data::Sessions(_)));
    window.set_cookies(matches!(&data, Data::Cookies(_)));
    let state = Rc::new(RefCell::new(State {
        store: store.clone(),
        data,
        selection: ListSelection::default(),
        order: Vec::new(),
        column: 0,
        ascending: true,
    }));
    refresh(&window, &mut state.borrow_mut());
    let active = Rc::new(Cell::new(true));
    let child: Rc<RefCell<Option<EditNetworkValueWindow>>> = Rc::default();
    let import_child: Rc<RefCell<Option<CookieImportWindow>>> = Rc::default();
    let close = Rc::new({
        let weak = window.as_weak();
        let child = child.clone();
        let import_child = import_child.clone();
        let active = active.clone();
        let owner = Rc::downgrade(&slots.owner);
        let slot = Rc::downgrade(slot);
        move || {
            if !active.replace(false) {
                return;
            }
            let edit = child.borrow().as_ref().map(ComponentHandle::clone_strong);
            if let Some(w) = edit {
                w.invoke_cancel_clicked();
            }
            let importing = import_child
                .borrow()
                .as_ref()
                .map(ComponentHandle::clone_strong);
            if let Some(w) = importing {
                w.invoke_action("cancel".into());
            }
            if let Some(w) = weak.upgrade() {
                if w.get_browser()
                    && let Some(owner) = owner.upgrade()
                {
                    let cookie_window = owner
                        .cookies
                        .borrow()
                        .as_ref()
                        .map(ComponentHandle::clone_strong);
                    if let Some(c) = cookie_window {
                        c.invoke_cancel_clicked();
                    }
                }
                let _ = w.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
        }
    });
    window.on_cancel_clicked({
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
    window.on_row_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, c, s| {
            if let (Some(w), Ok(i)) = (weak.upgrade(), usize::try_from(i)) {
                let mut st = state.borrow_mut();
                let order = st.order.clone();
                st.selection.click(&order, i, c, s);
                refresh(&w, &mut st);
            }
        }
    });
    window.on_sort({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, a| {
            if let (Some(w), Ok(i)) = (weak.upgrade(), usize::try_from(i)) {
                let mut st = state.borrow_mut();
                if st
                    .data
                    .cells(&hydrus_gui_model::gui_format::preferences(&st.store))
                    .first()
                    .is_some_and(|c| i < c.len())
                {
                    st.column = i;
                    st.ascending = a;
                    refresh(&w, &mut st);
                }
            }
        }
    });
    window.on_filter_changed({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                refresh(&w, &mut state.borrow_mut());
            }
        }
    });
    let launch: Rc<dyn Fn(bool)> = Rc::new({
        let weak = window.as_weak();
        let state = state.clone();
        let child = child.clone();
        let store = store.clone();
        let active = active.clone();
        let owner = Rc::downgrade(&slots.owner);
        move |editing| {
            let Some(owner) = owner.upgrade() else {
                return;
            };
            let slots = Slots { owner };
            let Some(w) = weak.upgrade() else {
                return;
            };
            if !active.get() || child.borrow().is_some() || !w.get_question().is_empty() {
                return;
            }
            let index = if editing {
                state.borrow().selection.one()
            } else {
                None
            };
            if editing && (index.is_none() || !w.get_can_edit()) {
                return;
            }
            if let Data::Sessions(rows) = &state.borrow().data
                && let Some(i) = index
            {
                let old = slots
                    .cookies
                    .borrow()
                    .as_ref()
                    .map(ComponentHandle::clone_strong);
                if let Some(old) = old {
                    old.invoke_cancel_clicked();
                }
                match CookieDraft::new(&store, rows[i].0.clone())
                    .map_err(|e| e.to_string())
                    .and_then(|d| open_data(&store, &slots, &slots.cookies, Data::Cookies(d)))
                {
                    Ok(_) => {}
                    Err(e) => w.set_error(e.into()),
                }
                return;
            }
            match edit_value(&w, &state, index, store.clone(), &child, active.clone()) {
                Ok(()) => {}
                Err(e) => w.set_error(e.into()),
            }
        }
    });
    window.on_add_clicked({
        let launch = launch.clone();
        move || launch(false)
    });
    window.on_edit_clicked(move || launch(true));
    window.on_export_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        let active = active.clone();
        move || {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if !active.get() || w.get_editing() || !w.get_question().is_empty() {
                return;
            }
            let st = state.borrow();
            let selected = st.selection.in_order(&st.order);
            let cookies: Vec<_> = match &st.data {
                Data::Cookies(d) => selected.iter().map(|&i| d.cookies[i].clone()).collect(),
                Data::Sessions(rows) => selected.iter().flat_map(|&i| rows[i].1.clone()).collect(),
                Data::Headers(_) => return,
            };
            if cookies.is_empty() {
                return;
            }
            match model::export_cookies(&cookies) {
                Ok(text) => {
                    crate::copy_to_clipboard(&text);
                    w.set_error("".into());
                }
                Err(e) => w.set_error(e.into()),
            }
        }
    });
    window.on_import_clipboard_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let child = import_child.clone();
        let active = active.clone();
        move || {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if !active.get() || w.get_editing() || !w.get_question().is_empty() {
                return;
            }
            let result = crate::from_clipboard()
                .and_then(|text| model::import_cookie_clipboard(&text))
                .and_then(|cookies| {
                    show_cookie_import(&w, &state, store.clone(), &child, active.clone(), cookies)
                });
            match result {
                Ok(()) => w.set_error("".into()),
                Err(e) => w.set_error(e.into()),
            }
        }
    });
    window.on_import_file_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let active = active.clone();
        move || {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if !active.get() || w.get_editing() || !w.get_question().is_empty() {
                return;
            }
            let paths = crate::pick(crate::Pick::Files, "select cookies.txt");
            if paths.is_empty() {
                return;
            }
            let result = (|| -> Result<(), String> {
                let mut cookies = Vec::new();
                for path in paths {
                    use std::io::Read as _;
                    let file = std::fs::File::open(&path)
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    let mut text = String::new();
                    file.take(u64::try_from(model::COOKIE_EXCHANGE_LIMIT).unwrap_or(u64::MAX) + 1)
                        .read_to_string(&mut text)
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    cookies.extend(
                        model::import_netscape_cookies(&text)
                            .map_err(|e| format!("{}: {e}", path.display()))?,
                    );
                }
                accept_cookie_import(&w, &mut state.borrow_mut(), &store, cookies)
            })();
            match result {
                Ok(()) => w.set_error("".into()),
                Err(e) => w.set_error(e.into()),
            }
        }
    });
    window.on_delete_clicked({
        let weak = window.as_weak();
        let active = active.clone();
        move || {
            if active.get()
                && let Some(w) = weak.upgrade()
                && !w.get_editing()
                && w.get_can_delete()
            {
                w.set_question(
                    if w.get_browser() {
                        model::CLEAR_QUESTION
                    } else if w.get_cookies() {
                        model::DELETE_QUESTION
                    } else {
                        "Remove all selected?"
                    }
                    .into(),
                );
            }
        }
    });
    window.on_answered({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let active = active.clone();
        move |yes| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if !active.get() || w.get_question().is_empty() {
                return;
            }
            w.set_question("".into());
            if !yes {
                return;
            }
            let mut st = state.borrow_mut();
            let selected = st.selection.in_order(&st.order);
            match &mut st.data {
                Data::Sessions(rows) => {
                    let deleting = selected
                        .iter()
                        .map(|&i| rows[i].0.clone())
                        .collect::<Vec<_>>();
                    let result = store.write(move |ctx| {
                        for s in deleting {
                            network::clear_session(ctx.conn(), &s)?;
                        }
                        Ok(())
                    });
                    match result {
                        Ok(()) => {
                            if let Ok(d) = sessions(&store) {
                                st.data = d;
                            }
                        }
                        Err(e) => w.set_error(e.to_string().into()),
                    }
                }
                Data::Cookies(d) => {
                    let mut i = 0;
                    d.cookies.retain(|_| {
                        let keep = !selected.contains(&i);
                        i += 1;
                        keep
                    });
                }
                Data::Headers(d) => {
                    let mut i = 0;
                    d.rows.retain(|_| {
                        let keep = !selected.contains(&i);
                        i += 1;
                        keep
                    });
                }
            }
            st.selection = ListSelection::default();
            refresh(&w, &mut st);
        }
    });
    window.on_duplicate_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                let mut st = state.borrow_mut();
                let selected = st.selection.in_order(&st.order);
                if let Data::Headers(d) = &mut st.data {
                    for i in selected {
                        let mut row = d.rows[i].clone();
                        let base = row.header.name.clone();
                        let mut suffix = 2;
                        while d
                            .rows
                            .iter()
                            .any(|r| r.header.name.eq_ignore_ascii_case(&row.header.name))
                        {
                            row.header.name = format!("{base}-{suffix}");
                            suffix += 1;
                        }
                        if let Err(e) = d.edit(None, row) {
                            w.set_error(e.into());
                        }
                    }
                }
                refresh(&w, &mut st);
            }
        }
    });
    window.on_refresh_clicked({
        let weak = window.as_weak(); let state = state.clone(); let store = store.clone();
        move || { if let Some(w) = weak.upgrade() { let mut st = state.borrow_mut();
            if w.get_browser() { match sessions(&store) { Ok(d) => { st.data = d; st.selection = ListSelection::default(); }, Err(e) => w.set_error(e.into()) } }
            else { w.set_error("Reopen this editor to reload persisted data. Refresh preserves your draft.".into()); }
            refresh(&w, &mut st);
        } }
    });
    window.on_apply_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        let active = active.clone();
        move || {
            if !active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            if w.get_editing() || !w.get_question().is_empty() {
                return;
            }
            let result = match &state.borrow().data {
                Data::Cookies(d) => d.apply(&store),
                Data::Headers(d) => d.apply(&store),
                Data::Sessions(_) => Ok(()),
            };
            match result {
                Ok(()) => close(),
                Err(e) => w.set_error(e.to_string().into()),
            }
        }
    });
    *slot.borrow_mut() = Some(window.clone_strong());
    // The caller's retained slot owns the component; callbacks hold weak handles.
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
fn accept_cookie_import(
    window: &NetworkDataWindow,
    state: &mut State,
    store: &Store,
    cookies: Vec<Cookie>,
) -> Result<(), String> {
    let count = cookies.len();
    match &mut state.data {
        Data::Cookies(draft) => {
            draft.import(cookies)?;
        }
        Data::Sessions(_) => {
            model::import_cookie_sessions(store, cookies).map_err(|e| e.to_string())?;
            state.data = sessions(store)?;
        }
        Data::Headers(_) => return Err("Headers cannot import cookies.".into()),
    }
    state.selection = ListSelection::default();
    window.set_message(format!("Added {count} cookies!").into());
    refresh(window, state);
    Ok(())
}
fn show_cookie_import(
    parent: &NetworkDataWindow,
    state: &Rc<RefCell<State>>,
    store: Arc<Store>,
    child: &Rc<RefCell<Option<CookieImportWindow>>>,
    parent_active: Rc<Cell<bool>>,
    cookies: Vec<Cookie>,
) -> Result<(), String> {
    let (browser, matching) = match &state.borrow().data {
        Data::Sessions(_) => (true, cookies.clone()),
        Data::Cookies(draft) => (false, model::matching_cookies(&cookies, &draft.session)),
        Data::Headers(_) => return Ok(()),
    };
    if cookies.is_empty() {
        parent.set_message(
            if browser {
                "There were no cookies in the clipboard!"
            } else {
                "There were no cookies in the clipboard for this domain!"
            }
            .into(),
        );
        return Ok(());
    }
    let window = CookieImportWindow::new().map_err(|e| e.to_string())?;
    let choosing = matching.len() != cookies.len();
    window.set_choosing(choosing);
    window.set_message(if choosing {
        format!("Of the {} cookies in your clipboard, {} match this domain. What do you want to import?", cookies.len(), matching.len())
    } else { model::cookie_import_question(&cookies, browser) }.into());
    window.set_matching_label(
        if matching.is_empty() {
            "nothing to import--bail out now"
        } else {
            "import only the matching cookies"
        }
        .into(),
    );
    let pending = Rc::new(RefCell::new(cookies));
    let active = Rc::new(Cell::new(true));
    let close = Rc::new({
        let weak = window.as_weak();
        let parent = parent.as_weak();
        let active = active.clone();
        let child = child.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if let Some(p) = parent.upgrade() {
                p.set_editing(false);
            }
            child.borrow_mut().take();
        }
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let parent = parent.as_weak();
        let state = state.clone();
        let close = close.clone();
        move |action| {
            if !active.get() {
                return;
            }
            if action == "cancel" {
                close();
                return;
            }
            if !parent_active.get() {
                close();
                return;
            }
            let (Some(w), Some(parent)) = (weak.upgrade(), parent.upgrade()) else {
                close();
                return;
            };
            if w.get_choosing() {
                if action == "matching" {
                    pending.borrow_mut().clone_from(&matching);
                } else if action != "all" {
                    return;
                }
                if pending.borrow().is_empty() {
                    close();
                    return;
                }
                w.set_choosing(false);
                w.set_message(model::cookie_import_question(&pending.borrow(), browser).into());
                return;
            }
            if action != "import" {
                return;
            }
            let result = accept_cookie_import(
                &parent,
                &mut state.borrow_mut(),
                &store,
                pending.borrow().clone(),
            );
            match result {
                Ok(()) => parent.set_error("".into()),
                Err(e) => parent.set_error(e.into()),
            }
            close();
        }
    });
    parent.set_message("".into());
    parent.set_editing(true);
    *child.borrow_mut() = Some(window.clone_strong());
    LAST_IMPORT.with(|s| *s.borrow_mut() = Some(window.as_weak()));
    if let Err(e) = window.show() {
        close();
        return Err(e.to_string());
    }
    Ok(())
}

fn edit_value(
    parent: &NetworkDataWindow,
    state: &Rc<RefCell<State>>,
    index: Option<usize>,
    store: Arc<Store>,
    child: &Rc<RefCell<Option<EditNetworkValueWindow>>>,
    parent_active: Rc<Cell<bool>>,
) -> Result<(), String> {
    let window = EditNetworkValueWindow::new().map_err(|e| e.to_string())?;
    let data = state.borrow().data.clone();
    let original_cookie = match &data {
        Data::Cookies(d) => Some(index.map_or_else(
            || Cookie {
                name: "name".into(),
                value: Some("123".into()),
                domain: format!(".{}", d.session.data),
                path: "/".into(),
                expires: Some(now() + 30 * 86400),
                secure: false,
                rest: Vec::new(),
            },
            |i| d.cookies[i].clone(),
        )),
        _ => None,
    };
    window.set_session_only(matches!(&data, Data::Sessions(_)));
    window.set_cookie(original_cookie.is_some());
    window.set_window_title(
        if window.get_session_only() {
            "enter new network context"
        } else if original_cookie.is_some() {
            "edit cookie"
        } else {
            "edit header"
        }
        .into(),
    );
    if let Some(c) = &original_cookie {
        window.set_name(c.name.as_str().into());
        window.set_value(c.value.as_deref().unwrap_or_default().into());
        window.set_domain(c.domain.as_str().into());
        window.set_path(c.path.as_str().into());
        window.set_secure(c.secure);
        window.set_session_cookie(c.expires.is_none());
        window.set_expires(c.expires.map(|n| n.to_string()).unwrap_or_default().into());
    } else if let Data::Headers(d) = &data {
        let row = index.map_or_else(
            || HeaderRow {
                context: NetworkContext::domain("hostname.com"),
                header: network::CustomHeader {
                    name: "Authorization".into(),
                    value: "Basic dXNlcm5hbWU6cGFzc3dvcmQ=".into(),
                    approval: Approval::Approved,
                    reason: String::new(),
                },
            },
            |i| d.rows[i].clone(),
        );
        window.set_global(row.context.kind == network::CONTEXT_GLOBAL);
        window.set_domain(row.context.data.into());
        window.set_name(row.header.name.into());
        window.set_value(row.header.value.into());
        window.set_reason(row.header.reason.into());
        window.set_approval(match row.header.approval {
            Approval::Approved => 0,
            Approval::Denied => 1,
            Approval::Pending => 2,
        });
    } else {
        window.set_domain("example.com".into());
    }
    parent.set_editing(true);
    let active = Rc::new(Cell::new(true));
    let close = Rc::new({
        let weak = window.as_weak();
        let parent = parent.as_weak();
        let active = active.clone();
        let child = child.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if let Some(p) = parent.upgrade() {
                p.set_editing(false);
            }
            child.borrow_mut().take();
        }
    });
    window.on_cancel_clicked({
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
    window.on_delta_clicked({
        let weak = window.as_weak();
        move || {
            if let Some(w) = weak.upgrade() {
                match w.get_expiry_delta().trim().parse::<i64>() {
                    Ok(delta) if (1200..=366 * 200 * 86400).contains(&delta) => {
                        w.set_expires((now() + delta).to_string().into());
                        w.set_session_cookie(false);
                        w.set_error("".into());
                    }
                    _ => w.set_error("Enter a time delta from 1200 seconds to 200 years.".into()),
                }
            }
        }
    });
    window.on_apply_clicked({
        let weak = window.as_weak();
        let parent = parent.as_weak();
        let state = state.clone();
        move || {
            if !active.get() || !parent_active.get() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let result = (|| -> Result<(), String> {
                let mut st = state.borrow_mut();
                match &mut st.data {
                    Data::Sessions(_) => {
                        let domain = w.get_domain().trim().to_ascii_lowercase();
                        model::validate_context_domain(&domain)?;
                        store
                            .write(move |ctx| {
                                let session = network::session_for(
                                    ctx.conn(),
                                    &NetworkContext::domain(domain),
                                )?;
                                network::create_session(ctx.conn(), &session)
                            })
                            .map_err(|e| e.to_string())?;
                        st.data = sessions(&store)?;
                        if let Some(p) = parent.upgrade() {
                            p.set_show_empty(true);
                        }
                    }
                    Data::Cookies(d) => {
                        let mut c = original_cookie.clone().ok_or("No cookie draft.")?;
                        c.name = w.get_name().into();
                        c.value = if c.value.is_none() && w.get_value().is_empty() {
                            None
                        } else {
                            Some(w.get_value().into())
                        };
                        c.domain = w.get_domain().into();
                        c.path = w.get_path().into();
                        c.secure = w.get_secure();
                        c.expires = if w.get_session_cookie() {
                            None
                        } else {
                            Some(w.get_expires().trim().parse::<i64>().map_err(
                                |_| "Enter an expiry as UTC seconds, or select session cookie.",
                            )?)
                        };
                        d.edit(index, c)?;
                    }
                    Data::Headers(d) => {
                        let context = if w.get_global() {
                            NetworkContext::global()
                        } else {
                            NetworkContext::domain(w.get_domain().trim().to_ascii_lowercase())
                        };
                        d.edit(
                            index,
                            HeaderRow {
                                context,
                                header: network::CustomHeader {
                                    name: w.get_name().into(),
                                    value: w.get_value().into(),
                                    approval: match w.get_approval() {
                                        1 => Approval::Denied,
                                        2 => Approval::Pending,
                                        _ => Approval::Approved,
                                    },
                                    reason: w.get_reason().into(),
                                },
                            },
                        )?;
                    }
                }
                st.selection = ListSelection::default();
                if let Some(p) = parent.upgrade() {
                    refresh(&p, &mut st);
                    p.set_error("".into());
                }
                Ok(())
            })();
            match result {
                Ok(()) => close(),
                Err(e) => w.set_error(e.into()),
            }
        }
    });
    LAST_EDIT.with(|s| *s.borrow_mut() = Some(window.as_weak()));
    *child.borrow_mut() = Some(window.clone_strong());
    window.show().map_err(|e| e.to_string())?;
    Ok(())
}
