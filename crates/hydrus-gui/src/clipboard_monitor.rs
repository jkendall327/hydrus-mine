//! Desktop clipboard polling, kept alive by the bound main window.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use hydrus_gui_model::clipboard_urls::Watcher;
use hydrus_store::settings::{self, ClipboardUrls};
use slint::ComponentHandle as _;

use crate::{MainWindow, Pages};

/// A running monitor; dropping its last owner stops the clipboard timer.
#[derive(Clone)]
pub struct Monitor(Rc<Inner>);

struct Inner {
    watcher: RefCell<Watcher>,
    pages: Rc<RefCell<Pages>>,
    window: slint::Weak<MainWindow>,
    changed: Rc<dyn Fn()>,
    timer: slint::Timer,
}

impl std::fmt::Debug for Monitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Monitor").finish_non_exhaustive()
    }
}

impl Monitor {
    /// Poll the clipboard once per second while either menu switch is enabled.
    pub fn bind(window: &MainWindow, pages: Rc<RefCell<Pages>>, changed: Rc<dyn Fn()>) -> Self {
        let inner = Rc::new(Inner {
            watcher: RefCell::default(),
            pages,
            window: window.as_weak(),
            changed,
            timer: slint::Timer::default(),
        });
        let weak = Rc::downgrade(&inner);
        inner.timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(1),
            move || {
                if let Some(inner) = weak.upgrade() {
                    Self(inner).poll();
                }
            },
        );
        Self(inner)
    }

    fn error(&self, error: String) {
        eprintln!("{error}");
        let store = self.0.pages.borrow().store().clone();
        let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |elapsed| elapsed.as_secs_f64());
        let message = hydrus_store::popups::Job::text(error, created);
        if let Err(error) =
            store.write(move |ctx| hydrus_store::popups::add(ctx.conn(), &message, now))
        {
            eprintln!("Could not show the clipboard monitoring message: {error}");
        }
    }

    /// Persist one menu toggle and retry the current clipboard on the next poll.
    pub fn toggle(&self, watchers: bool) {
        let store = self.0.pages.borrow().store().clone();
        let saved = store.write(move |ctx| {
            let mut flags: ClipboardUrls = settings::get(ctx.conn())?;
            let flag = if watchers {
                &mut flags.watchers
            } else {
                &mut flags.other_recognised
            };
            *flag = !*flag;
            settings::set(ctx.conn(), &flags)
        });
        match saved {
            Ok(()) => self.0.watcher.borrow_mut().reset(),
            Err(error) => self.error(format!(
                "Could not change clipboard URL monitoring: {error}"
            )),
        }
    }

    /// Perform one timer tick, also exposed for deterministic desktop tests.
    pub fn poll(&self) {
        if self.0.window.upgrade().is_none() {
            return;
        }
        let store = self.0.pages.borrow().store().clone();
        let Ok(flags) = store.read(settings::get::<ClipboardUrls>) else {
            return;
        };
        if !self.0.watcher.borrow_mut().reading(flags) {
            return;
        }
        let text = match crate::clipboard_text() {
            Ok(text) => text,
            Err(error) => {
                self.0.watcher.borrow_mut().failed();
                self.error(format!("Could not access the clipboard: {error}"));
                return;
            }
        };
        let snapshot = store.snapshot();
        let routed = self
            .0
            .watcher
            .borrow_mut()
            .changed(text.as_deref(), &snapshot.url_classes);
        if routed.is_empty() {
            return;
        }
        for url in &routed {
            let imported = self.0.pages.borrow_mut().import_clipboard_url(url);
            if let Err(error) = imported {
                self.error(error);
                break;
            }
        }
        (self.0.changed)();
    }
}
