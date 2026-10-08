//! Help > debug's actions (hydrus-gui-model's `debug_actions`): messages,
//! popups, the delayed "modal" popup (an ordinary popup here, counting
//! down), saving the last session, the database checkpoint, the
//! environment dump, clearing the rendering caches and leaving the event
//! loop at once.
use std::cell::RefCell;
use std::io::Write as _;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hydrus_core::Sha256;
use hydrus_gui_model::debug_actions::{self as model, Action};
use hydrus_store::Store;
use hydrus_store::popups::{self, Job};

use crate::ChoiceButtonsWindow;
use slint::ComponentHandle as _;

thread_local! {
    /// The message shown, kept open until answered.
    static MESSAGE: RefCell<Option<ChoiceButtonsWindow>> = const { RefCell::new(None) };
}

/// The message window now shown, if any (for tests of the actions that say
/// something and wait for "ok").
pub fn message_window() -> Option<ChoiceButtonsWindow> {
    MESSAGE.with(|m| m.borrow().as_ref().map(ChoiceButtonsWindow::clone_strong))
}

/// Set by "simulate program exit signal", once the event loop is told to stop.
static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Whether "simulate program exit signal" has run in this process.
pub fn exit_requested() -> bool {
    EXIT_REQUESTED.load(Ordering::SeqCst)
}

/// What `HydrusData.DebugPrint` was given, oldest first: the debug actions
/// print to the console and keep the lines, for the tests.
static PRINTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The lines the debug actions have printed so far.
pub fn debug_printed() -> Vec<String> {
    PRINTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// `HydrusData.DebugPrint`: a line on the console, flushed at once.
fn debug_print(line: &str) {
    eprintln!("{line}");
    let _ = std::io::stderr().flush();
    PRINTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(line.to_owned());
}

/// Send the report modes' messages to the console and a popup in `store`
/// (`HydrusData.ShowText`), unless something already receives them.
pub(crate) fn install_report_sink(store: &std::sync::Arc<Store>) {
    // (a report can be made from inside a database job, so the popup is
    // written by a thread of its own rather than waited for)
    let (lines, queue) = std::sync::mpsc::channel::<String>();
    let store = std::sync::Arc::downgrade(store);
    std::thread::spawn(move || {
        for text in queue {
            let Some(store) = store.upgrade() else { break };
            post(&store, vec![Job::text(text, now())]);
        }
    });
    hydrus_core::debug_flags::set_sink_if_none(Box::new(move |text| {
        debug_print(text);
        let _ = lines.send(text.to_owned());
    }));
}

static CRASH_LOGGING: AtomicBool = AtomicBool::new(false);

/// Whether "use faulthandler to log crashes" is on.
pub fn crash_logging() -> bool {
    CRASH_LOGGING.load(Ordering::SeqCst)
}

/// "use faulthandler to log crashes" (`FlipCrashReporting`): while on, each
/// panic is written, with a backtrace, to a "client crash" log in the
/// database directory; turning it off puts the earlier panic hook back.
fn flip_crash_logging(dir: &std::path::Path) {
    static EARLIER: Mutex<Option<Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync>>> =
        Mutex::new(None);
    if CRASH_LOGGING.fetch_xor(true, Ordering::SeqCst) {
        let earlier = EARLIER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(earlier) = earlier {
            std::panic::set_hook(earlier);
        }
        return;
    }
    let path = dir.join(format!("client crash - {}.log", now() as i64));
    let earlier = std::panic::take_hook();
    // (the earlier hook is kept to be put back, and called meanwhile)
    let earlier = std::sync::Arc::new(earlier);
    *EARLIER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new({
        let earlier = earlier.clone();
        move |info| earlier(info)
    }));
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(
                file,
                "{info}\n{}\n",
                std::backtrace::Backtrace::force_capture()
            );
        }
        earlier(info);
    }));
}

/// What the actions work with.
pub(crate) struct Context {
    pub pages: Rc<RefCell<crate::Pages>>,
    pub ask: crate::menu_bar::Ask,
    /// Clear the image, tile and thumbnail caches.
    pub clear_caches: Rc<dyn Fn()>,
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

#[allow(clippy::cast_possible_truncation)] // (seconds)
fn post(store: &Store, jobs: Vec<Job>) {
    let at = now() as i64;
    let done = store.write(move |ctx| {
        for job in &jobs {
            popups::add(ctx.conn(), job, at)?;
        }
        Ok(())
    });
    if let Err(e) = done {
        eprintln!("could not show the debug popups: {e}");
    }
}

/// An information or warning message with an "ok" button.
pub(crate) fn message(title: &str, text: &str) {
    message_then(title, text, Box::new(|| {}));
}

/// A message with an "ok" button; `then` runs once it is dismissed.
pub(crate) fn message_then(title: &str, text: &str, then: Box<dyn FnOnce()>) {
    let ask = crate::choice_buttons::Ask {
        title,
        message: text,
        choices: Vec::new(),
        no_label: "ok",
    };
    match crate::choice_buttons::open(&ask, move |_| then()) {
        Ok(window) => MESSAGE.with(|m| *m.borrow_mut() = window),
        Err(e) => eprintln!("could not show the message: {e}"),
    }
}

/// `_DebugMakeDelayedModalPopup`'s thread: after five seconds, ten seconds
/// counting down (stopping if cancelled), then gone.
#[allow(clippy::cast_possible_truncation)] // (seconds)
fn modal(store: &Store, cancellable: bool) {
    std::thread::sleep(Duration::from_secs(5));
    let mut job = Job::new(false, cancellable, now());
    job.status_title = Some(model::MODAL_TITLE.into());
    let key = job.key;
    post(store, vec![job]);
    for i in 0..10 {
        let at = now() as i64;
        let cancelled = store
            .write(move |ctx| {
                // (gone, as dismissed, counts as cancelled)
                Ok(popups::update(ctx.conn(), &key, at, |job| {
                    job.status_text_1 = Some(model::modal_text(i));
                    job.popup_gauge_1 = Some((i, 10));
                    job.cancelled
                })?
                .unwrap_or(true))
            })
            .unwrap_or(true);
        if cancelled {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let at = now() as i64;
    let _ = store.write(move |ctx| {
        popups::update(ctx.conn(), &key, at, |job| job.finish_and_dismiss(None, at)).map(|_| ())
    });
}

pub(crate) fn run(context: &Context, action: Action) {
    let store = context.pages.borrow().store().clone();
    match action {
        Action::ProfileInfo => message("Information", model::PROFILE_MESSAGE),
        Action::MessageBox => message("Warning", model::MESSAGE_BOX_TEXT),
        Action::SomePopups => {
            post(
                &store,
                model::some_popups(now(), || Sha256(rand::random::<[u8; 32]>())),
            );
            for i in 1..4 {
                let store = store.clone();
                slint::Timer::single_shot(Duration::from_millis(500 * i as u64), move || {
                    post(&store, vec![Job::text(model::delayed_popup_text(i), now())]);
                });
            }
        }
        Action::ModalPopup { cancellable } => {
            std::thread::spawn(move || modal(&store, cancellable));
        }
        // (hydrus-rs keeps no column widths to reset)
        Action::ResetColumns => (context.ask)(model::RESET_COLUMNS_QUESTION.into(), Rc::new(|| {})),
        Action::SaveLastSession => {
            #[allow(clippy::cast_possible_truncation)] // (seconds)
            let at = now() as i64;
            if let Err(e) = context.pages.borrow_mut().save(at) {
                eprintln!("could not save the last session: {e}");
            }
        }
        Action::FlushLog => {
            debug_print(model::FLUSH_LOG);
        }
        Action::ForceCommit => {
            // (pausing moves the whole write-ahead log into the database file)
            if let Err(e) = store.pause() {
                eprintln!("could not commit the database: {e}");
            }
        }
        Action::ShowEnv => {
            let separator = if cfg!(windows) { ';' } else { ':' };
            let text = model::env_text(std::env::vars(), separator);
            debug_print(&text);
            post(&store, vec![Job::text(text, now())]);
        }
        Action::Exit => {
            EXIT_REQUESTED.store(true, Ordering::SeqCst);
            let _ = slint::quit_event_loop();
        }
        Action::ClearRenderingCaches => (context.clear_caches)(),
        Action::ScanStorage => {
            let Some(path) = crate::pick(crate::Pick::Folder, "Select directory")
                .into_iter()
                .next()
            else {
                return;
            };
            let granularity = store.snapshot().storage.granularity();
            let started = std::time::Instant::now();
            let found = model::presumptive_subfolders(&path, granularity).map(|f| f.len());
            let text = model::scan_text(&found, started.elapsed().as_secs_f64());
            post(&store, vec![Job::text(text, now())]);
        }
        Action::FlipCrashLogging => flip_crash_logging(store.dir()),
    }
}
