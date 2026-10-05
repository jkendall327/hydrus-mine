//! Owned image-cache policy notification, without global media preferences.
use hydrus_store::Store;
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};

struct State {
    store: Arc<Store>,
    normalise_icc: Cell<bool>,
    alive: Cell<bool>,
    timer: slint::Timer,
}
impl State {
    fn changed(&self) -> bool {
        if !self.alive.get() {
            return false;
        }
        match self.store.read(hydrus_store::image_colour::load) {
            Ok(policy) => self.normalise_icc.replace(policy.normalise_icc) != policy.normalise_icc,
            Err(error) => {
                eprintln!("could not refresh image ICC policy: {error}");
                false
            }
        }
    }
}
/// Retired owners cannot restart their timer or notify a successor.
#[derive(Clone)]
pub(crate) struct Watch(Rc<State>);
impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageColourWatch")
            .field("active", &self.0.alive.get())
            .finish_non_exhaustive()
    }
}
impl Watch {
    pub(crate) fn new(store: Arc<Store>) -> Self {
        let policy = store
            .read(hydrus_store::image_colour::load)
            .unwrap_or_default();
        Self(Rc::new(State {
            store,
            normalise_icc: Cell::new(policy.normalise_icc),
            alive: Cell::new(true),
            timer: slint::Timer::default(),
        }))
    }
    pub(crate) fn start(&self, owner: Rc<dyn Fn() -> bool>, refresh: Rc<dyn Fn()>) {
        if !self.0.alive.get() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        self.0.timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(100),
            move || {
                if owner()
                    && let Some(state) = weak.upgrade()
                    && state.changed()
                {
                    refresh();
                }
            },
        );
    }
    pub(crate) fn changed(&self) -> bool {
        self.0.changed()
    }
    pub(crate) fn close(&self) {
        self.0.alive.set(false);
        self.0.timer.stop();
    }
}
