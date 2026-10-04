//! Bad-shutdown recovery before constructing the main client and its workers.
use crate::{Pages, SessionDialog};
use hydrus_gui_model::session_lifecycle::RecoveryQuestion;
use hydrus_store::{
    Store,
    settings::{self, GuiSessionSettings},
};
use slint::ComponentHandle as _;
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

const RUNNING_FILE: &str = "gui_running";

/// The running marker is removed only after the client saves and stops its work.
/// Dropping it leaves recovery evidence, including an interrupted startup.
#[derive(Debug)]
pub struct Run {
    path: PathBuf,
    bad: bool,
}
impl Run {
    /// Call while holding the native GUI store lock.
    pub fn begin(dir: &Path) -> std::io::Result<Self> {
        let path = dir.join(RUNNING_FILE);
        let bad = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => false,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => true,
            Err(error) => return Err(error),
        };
        Ok(Self { path, bad })
    }
    pub fn bad(&self) -> bool {
        self.bad
    }
    pub fn finish(self) -> std::io::Result<()> {
        match std::fs::remove_file(self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    }
}

type Ready = Rc<dyn Fn(hydrus_store::Result<Pages>)>;
struct Inner {
    window: SessionDialog,
    store: Arc<Store>,
    ready: Ready,
    name: String,
    answered: Cell<bool>,
    deadline: i64,
    timer: slint::Timer,
}

#[derive(Clone)]
pub struct Recovery(Rc<Inner>);
impl std::fmt::Debug for Recovery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StartupRecovery")
            .field("answered", &self.0.answered.get())
            .field("deadline", &self.0.deadline)
            .finish_non_exhaustive()
    }
}

/// Prompt only when a bad shutdown and a real configured saved session coexist.
/// Blank, missing and clean-shutdown startup load directly. `ready` shows the
/// main window before this dialog hides so the event loop remains alive.
pub fn prepare(store: Arc<Store>, bad: bool, ready: Ready) -> Result<Option<Recovery>, String> {
    let settings: GuiSessionSettings = store
        .read(settings::get)
        .map_err(|error| error.to_string())?;
    let name = settings.startup.filter(|name| {
        bad && store
            .read(|conn| hydrus_store::sessions::load(conn, name))
            .is_ok_and(|session| session.is_some())
    });
    let Some(name) = name else {
        ready(Pages::open_startup(store));
        return Ok(None);
    };
    let question = RecoveryQuestion::for_session(&name);
    let window = SessionDialog::new().map_err(|error| error.to_string())?;
    window.set_window_title(question.title.into());
    window.set_message(question.message.into());
    window.set_yes_label(question.yes.into());
    window.set_no_label(question.no.into());
    window.set_asking_name(false);
    let recovery = Recovery(Rc::new(Inner {
        window,
        store,
        ready,
        name,
        answered: Cell::new(false),
        deadline: hydrus_core::TimestampMs::now().0
            + i64::try_from(question.auto_yes_seconds).unwrap_or(15) * 1_000,
        timer: slint::Timer::default(),
    }));
    recovery.0.window.on_answered({
        let weak = Rc::downgrade(&recovery.0);
        move |yes| {
            if let Some(inner) = weak.upgrade() {
                Recovery(inner).answer(yes);
            }
        }
    });
    recovery.0.window.on_cancelled({
        let weak = Rc::downgrade(&recovery.0);
        move || {
            if let Some(inner) = weak.upgrade() {
                Recovery(inner).answer(false);
            }
        }
    });
    recovery.0.window.window().on_close_requested({
        let weak = Rc::downgrade(&recovery.0);
        move || {
            if let Some(inner) = weak.upgrade() {
                Recovery(inner).answer(false);
            }
            slint::CloseRequestResponse::HideWindow
        }
    });
    recovery.0.timer.start(
        slint::TimerMode::SingleShot,
        std::time::Duration::from_secs(question.auto_yes_seconds),
        {
            let weak = Rc::downgrade(&recovery.0);
            move || {
                if let Some(inner) = weak.upgrade() {
                    Recovery(inner).answer(true);
                }
            }
        },
    );
    recovery
        .0
        .window
        .show()
        .map_err(|error| error.to_string())?;
    Ok(Some(recovery))
}
impl Recovery {
    pub fn window(&self) -> SessionDialog {
        self.0.window.clone_strong()
    }
    pub fn deadline(&self) -> i64 {
        self.0.deadline
    }
    pub fn poll_at(&self, now_ms: i64) {
        if now_ms >= self.0.deadline {
            self.answer(true);
        }
    }
    fn answer(&self, load_default: bool) {
        if self.0.answered.replace(true) {
            return;
        }
        self.0.timer.stop();
        (self.0.ready)(Pages::open_startup_named(
            self.0.store.clone(),
            load_default.then_some(self.0.name.as_str()),
        ));
        let _ = self.0.window.hide();
    }
}
