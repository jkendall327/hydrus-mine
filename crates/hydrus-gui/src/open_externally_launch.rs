//! Owned default launch consumers, resolving registered typed calls before dispatch.
use crate::SessionDialog;
use hydrus_core::{
    HashId,
    external_calls::{ActualCall, Manager, Parameter},
    open_externally::Routing,
};
use hydrus_gui_model::open_externally::{self as model, Launch};
use hydrus_store::{Store, settings};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type PendingLaunch = (crossbeam_channel::Receiver<Result<(), String>>, String);

struct State {
    valid: Rc<dyn Fn() -> bool>,
    notice: Rc<RefCell<Option<SessionDialog>>>,
    alive: Cell<bool>,
    timer: Timer,
    receivers: RefCell<Vec<PendingLaunch>>,
}
/// Owner-local launch boundary shared by that window's default buttons and menus.
#[derive(Clone)]
pub struct Launcher(Rc<State>);
impl std::fmt::Debug for Launcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalLauncher").finish_non_exhaustive()
    }
}
impl Launcher {
    pub fn new(valid: Rc<dyn Fn() -> bool>) -> Self {
        let state = Rc::new(State {
            valid,
            notice: Rc::default(),
            alive: Cell::new(true),
            timer: Timer::default(),
            receivers: RefCell::default(),
        });
        state
            .timer
            .start(TimerMode::Repeated, std::time::Duration::from_millis(50), {
                let weak = Rc::downgrade(&state);
                move || {
                    let Some(state) = weak.upgrade() else {
                        return;
                    };
                    let mut failed = Vec::new();
                    state.receivers.borrow_mut().retain(|(receiver, kind)| {
                        match receiver.try_recv() {
                            Ok(Err(error)) => {
                                failed.push(format!("Sorry, could not open that {kind}: {error}"));
                                false
                            }
                            Ok(Ok(())) | Err(crossbeam_channel::TryRecvError::Disconnected) => {
                                false
                            }
                            Err(crossbeam_channel::TryRecvError::Empty) => true,
                        }
                    });
                    if state.alive.get() && (state.valid)() {
                        for error in failed {
                            state.information(&error);
                        }
                    }
                }
            });
        Self(state)
    }
    pub fn notice(&self) -> Option<SessionDialog> {
        self.0
            .notice
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
    }
    pub fn cancel(&self) {
        self.0.alive.set(false);
        self.0.timer.stop();
        self.0.receivers.borrow_mut().clear();
        let notice = self.notice();
        if let Some(window) = notice {
            window.invoke_force_close();
        }
    }
    fn settings(store: &Store) -> Result<(Manager, Routing), String> {
        store
            .read(|conn| Ok((settings::get(conn)?, settings::get(conn)?)))
            .map_err(|error| error.to_string())
    }
    pub fn url(&self, store: &Store, url: &str) -> bool {
        if !self.0.alive.get() || !(self.0.valid)() || self.0.notice.borrow().is_some() {
            return false;
        }
        let plan = Self::settings(store)
            .and_then(|(manager, routing)| model::url(&manager, &routing, url));
        self.dispatch(plan, "URL")
    }
    pub fn file(&self, store: &Store, id: HashId) -> bool {
        if !self.0.alive.get() || !(self.0.valid)() || self.0.notice.borrow().is_some() {
            return false;
        }
        let plan = (|| {
            let (manager, routing) = Self::settings(store)?;
            let media = store
                .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
                .map_err(|error| error.to_string())?
                .into_iter()
                .next()
                .ok_or_else(|| "This file is not local--it cannot be opened!".to_owned())?;
            let local_storage = store
                .read(|conn| {
                    let services = hydrus_store::services::ServiceRegistry::load(conn)?;
                    Ok(hydrus_store::content::DomainRoles::new(&services)?.local_file_storage)
                })
                .map_err(|error| error.to_string())?;
            if !media
                .current
                .iter()
                .any(|location| location.service == local_storage)
            {
                return Err("This file is not local--it cannot be opened!".into());
            }
            let path = crate::thumbnail_menu::paths(store, &[id])
                .pop()
                .ok_or_else(|| "This file is not local--it cannot be opened!".to_owned())?;
            let mime = media
                .info
                .as_ref()
                .ok_or_else(|| "This file has no filetype!".to_owned())?
                .mime;
            model::file(
                &manager,
                &routing,
                mime,
                &path,
                &crate::file_url(&path),
                &media.hash.to_hex(),
                i64::from(id.0),
            )
        })();
        self.dispatch(plan, "file")
    }
    fn dispatch(&self, plan: Result<Launch, String>, kind: &str) -> bool {
        match plan {
            Err(error) => {
                self.0
                    .information(&format!("Sorry, could not open that {kind}: {error}"));
                false
            }
            Ok(Launch {
                call: ActualCall::DefaultFile,
                inputs,
            }) => {
                if let Some(path) = inputs
                    .get(&Parameter::Path)
                    .and_then(|values| values.first())
                {
                    crate::launch(path);
                    true
                } else {
                    false
                }
            }
            Ok(Launch {
                call: ActualCall::DefaultUrl,
                inputs,
            }) => {
                if let Some(url) = inputs
                    .get(&Parameter::Url)
                    .and_then(|values| values.first())
                {
                    crate::launch(url);
                    true
                } else {
                    false
                }
            }
            Ok(Launch {
                call: ActualCall::Process(process),
                inputs,
            }) => {
                if let Err(error) = process.command(&inputs) {
                    self.0
                        .information(&format!("Sorry, could not open that {kind}: {error}"));
                    return false;
                }
                let (send, receive) = crossbeam_channel::bounded(1);
                match std::thread::Builder::new()
                    .name("external launch".into())
                    .spawn(move || {
                        let _ = send.send(model::run_process(&process, &inputs));
                    }) {
                    Ok(_) => {
                        self.0.receivers.borrow_mut().push((receive, kind.into()));
                        true
                    }
                    Err(error) => {
                        self.0
                            .information(&format!("Sorry, could not open that {kind}: {error}"));
                        false
                    }
                }
            }
        }
    }
}
impl State {
    fn information(&self, text: &str) {
        let old = self
            .notice
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong);
        if let Some(window) = old {
            window.invoke_force_close();
        }
        let Ok(window) = SessionDialog::new() else {
            return;
        };
        window.set_window_title("Information".into());
        window.set_message(text.into());
        window.set_notice_only(true);
        window.set_notice_ok_label("OK".into());
        let alive = Rc::new(Cell::new(true));
        let close: Rc<dyn Fn()> = Rc::new({
            let weak = window.as_weak();
            let slot = Rc::downgrade(&self.notice);
            move || {
                if !alive.replace(false) {
                    return;
                }
                if let Some(window) = weak.upgrade() {
                    let _ = window.hide();
                }
                if let Some(slot) = slot.upgrade() {
                    slot.borrow_mut().take();
                }
            }
        });
        window.on_cancelled({
            let close = close.clone();
            let valid = self.valid.clone();
            move || {
                if valid() {
                    close();
                }
            }
        });
        window.on_force_close({
            let close = close.clone();
            move || close()
        });
        let valid = self.valid.clone();
        window.window().on_close_requested(move || {
            if valid() {
                close();
                slint::CloseRequestResponse::HideWindow
            } else {
                slint::CloseRequestResponse::KeepWindowShown
            }
        });
        *self.notice.borrow_mut() = Some(window.clone_strong());
        if let Err(error) = window.show() {
            window.invoke_force_close();
            eprintln!("could not show external launch information: {error}");
        }
    }
}
