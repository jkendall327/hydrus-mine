//! Folder-manager acquisition without blocking the GUI event loop.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use hydrus_store::Store;
use hydrus_store::folder_activity::{Edit, Kind};
use slint::ComponentHandle as _;

use crate::FoldersWindow;

/// The manager made ready: its window, how it ends (the dialog helper's,
/// made as it opened), and its lease.
type Ready =
    Rc<dyn Fn(FoldersWindow, crate::dialog::Dialog<FoldersWindow>, Edit) -> Result<(), String>>;

/// Request the transient pause before reading any manager draft. Poll the
/// worker's OS lease on the UI timer; Cancel releases the request immediately.
pub(crate) fn open(
    store: &Arc<Store>,
    slot: &Rc<RefCell<Option<FoldersWindow>>>,
    kind: Kind,
    ready: Ready,
) -> Result<(), String> {
    if let Some(window) = slot.borrow().as_ref() {
        return window.show().map_err(|e| e.to_string());
    }
    let Some(mut edit) = Edit::request(store.dir(), kind).map_err(|e| e.to_string())? else {
        return Err(format!("Another {} manager is already open.", kind.title()));
    };
    let window = crate::app_title::new::<crate::FoldersWindow>().map_err(|e| e.to_string())?;
    // (an ordinary dialog: kept by `regular_dialog` when applied; while it
    // waits, only cancelled)
    let ended = crate::dialog::Dialog::new(&window);
    if edit.try_ready().map_err(|e| e.to_string())? {
        return ready(window, ended, edit);
    }
    window.set_window_title(kind.title().into());
    window.set_waiting(true);
    window.set_wait_message(kind.wait_text().into());
    let request = Rc::new(RefCell::new(Some(edit)));
    let timer = Rc::new(RefCell::new(None::<slint::Timer>));
    let closed = Rc::new(Cell::new(false));
    let close = Rc::new({
        let request = request.clone();
        let timer = timer.clone();
        let closed = closed.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if closed.replace(true) {
                return;
            }
            timer.borrow_mut().take();
            request.borrow_mut().take();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    window.on_cancel({
        let ended = ended.clone();
        move || {
            ended.cancel();
        }
    });
    // (whatever ends it while it waits, through the dialog helper; once
    // ready, this does nothing)
    ended.on_closed({
        let close = close.clone();
        move || close()
    });
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_millis(100), {
        let weak = window.as_weak();
        let timer = timer.clone();
        let closed = closed.clone();
        move || {
            if closed.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                close();
                return;
            };
            let result = request
                .borrow_mut()
                .as_mut()
                .expect("open wait owns a request")
                .try_ready();
            match result {
                Ok(false) => {}
                Ok(true) => {
                    timer.borrow_mut().take();
                    let edit = request.borrow_mut().take().expect("ready request");
                    // Retained callbacks from the wait phase cannot cancel the
                    // acquired manager, whose bindings now own the lease.
                    closed.set(true);
                    window.set_waiting(false);
                    let made = ready(window.clone_strong(), ended.clone(), edit);
                    // (the waiting window, now the list, is fitted again)
                    if made.is_ok() {
                        ended.refit();
                    }
                    if let Err(error) = made {
                        window.invoke_cancel();
                        closed.set(false);
                        close();
                        eprintln!("could not open the folder manager: {error}");
                    }
                }
                Err(error) => {
                    timer.borrow_mut().take();
                    request.borrow_mut().take();
                    window.set_wait_message(error.to_string().into());
                }
            }
        }
    });
    *timer.borrow_mut() = Some(poll);
    if let Err(error) = window.show() {
        window.invoke_cancel();
        return Err(error.to_string());
    }
    *slot.borrow_mut() = Some(window);
    Ok(())
}
