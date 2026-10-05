//! Owned asynchronous saved-snapshot reconstruction for Help > Debug.
use crate::{ChangePages, MainWindow, Pages};
use hydrus_core::pages::PageKey;
use hydrus_store::{Store, session_backups::Snapshot};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};

type Reply = Result<Snapshot, String>;
struct State {
    main: slint::Weak<MainWindow>,
    binding: Rc<Cell<bool>>,
    active: Cell<bool>,
    pages: Weak<RefCell<Pages>>,
    store: Arc<Store>,
    change: RefCell<Option<ChangePages>>,
    pending: RefCell<Vec<crossbeam_channel::Receiver<Reply>>>,
    timer: slint::Timer,
}
impl State {
    fn live(&self) -> bool {
        self.active.get() && self.binding.get() && self.main.upgrade().is_some()
    }
    fn retire(&self) {
        self.active.set(false);
        self.timer.stop();
        self.pending.borrow_mut().clear();
        self.change.borrow_mut().take();
    }
    fn poll(&self) {
        if !self.live() {
            self.retire();
            return;
        }
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        let mut replies = Vec::new();
        for receive in pending {
            match receive.try_recv() {
                Ok(reply) => replies.push(reply),
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    self.pending.borrow_mut().push(receive);
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    replies.push(Err("reload worker disconnected".into()));
                }
            }
        }
        for reply in replies {
            if !self.live() {
                self.retire();
                break;
            }
            match reply {
                Ok(snapshot) => {
                    let change = self.change.borrow().clone();
                    if let Some(change) = change {
                        change(&|pages| pages.reload_snapshot(&snapshot));
                    }
                }
                Err(error) => {
                    if let Some(main) = self.main.upgrade() {
                        main.set_note(format!("could not reload session: {error}").into());
                    }
                }
            }
        }
        if self.pending.borrow().is_empty() {
            self.timer.stop();
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.retire();
    }
}

/// Cloneable access to this binding's reload requests; it does not retain the lease.
#[derive(Clone)]
#[allow(missing_debug_implementations)]
pub struct Control(Rc<State>);
/// Final Bound-clone retirement lease, shared through Rc by the binding.
pub(crate) struct Owner(Control);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}
impl Control {
    pub(crate) fn new(
        main: &MainWindow,
        pages: &Rc<RefCell<Pages>>,
        binding: Rc<Cell<bool>>,
    ) -> Self {
        Self(Rc::new(State {
            main: main.as_weak(),
            binding,
            active: Cell::new(true),
            pages: Rc::downgrade(pages),
            store: pages.borrow().store().clone(),
            change: RefCell::new(None),
            pending: RefCell::new(Vec::new()),
            timer: slint::Timer::default(),
        }))
    }
    pub(crate) fn set_change(&self, change: ChangePages) {
        *self.0.change.borrow_mut() = Some(change);
    }
    pub(crate) fn owner(&self) -> Owner {
        Owner(self.clone())
    }
    /// Permanently reject old menu invocations and late asynchronous deliveries.
    pub fn retire(&self) {
        self.0.retire();
    }
    /// Number of admitted workers awaiting delivery in this live binding.
    pub fn pending(&self) -> usize {
        self.0.pending.borrow().len()
    }
    /// Deliver completed actual workers; exposed for native event-loop replays.
    pub fn poll(&self) {
        self.0.poll();
    }
    /// Capture now, save/load in the store worker, then force-close/reconstruct.
    /// Hidden Main refuses new requests; admitted work continues while merely hidden.
    pub fn start(&self) {
        if !self.0.live()
            || !self
                .0
                .main
                .upgrade()
                .is_some_and(|main| main.window().is_visible() && main.get_question().is_empty())
        {
            return;
        }
        let Some(pages) = self.0.pages.upgrade() else {
            return;
        };
        let snapshot = match pages.borrow_mut().snapshot_for_reload() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if let Some(main) = self.0.main.upgrade() {
                    main.set_note(format!("could not reload session: {error}").into());
                }
                return;
            }
        };
        let store = self.0.store.clone();
        let slot = format!(
            "temp_session_slot_for_reload_{}",
            PageKey::random().to_hex()
        );
        let (send, receive) = crossbeam_channel::bounded(1);
        let spawn = std::thread::Builder::new()
            .name("gui session reload".into())
            .spawn(move || {
                let saved_slot = slot.clone();
                let result = store
                    .write(move |ctx| {
                        hydrus_store::session_reload::save(ctx.conn(), &saved_slot, &snapshot)
                    })
                    .and_then(|()| {
                        let loaded_slot = slot.clone();
                        store.write(move |ctx| {
                            hydrus_store::session_reload::take(ctx.conn(), &loaded_slot)
                        })
                    });
                if result.is_err() {
                    let _ = store
                        .write(move |ctx| hydrus_store::session_reload::remove(ctx.conn(), &slot));
                }
                let _ = send.send(result.map_err(|error| error.to_string()));
            });
        if let Err(error) = spawn {
            if let Some(main) = self.0.main.upgrade() {
                main.set_note(format!("could not reload session: {error}").into());
            }
            return;
        }
        self.0.pending.borrow_mut().push(receive);
        let weak = Rc::downgrade(&self.0);
        self.0.timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(20),
            move || {
                if let Some(state) = weak.upgrade() {
                    state.poll();
                }
            },
        );
    }
}
