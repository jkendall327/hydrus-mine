//! Each displayed native canvas owns one interval tracker. Navigation and final
//! close end intervals; a cancelled filtering decision still represents viewing.
use hydrus_core::{CanvasType, HashId, TimestampMs};
use hydrus_gui_model::viewing_statistics::Tracker;
use hydrus_store::Store;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub(crate) struct CanvasTracker {
    tracker: Rc<RefCell<Tracker>>,
    active: Rc<Cell<bool>>,
}
impl CanvasTracker {
    pub(crate) fn new(store: Arc<Store>, canvas: CanvasType) -> Self {
        Self {
            tracker: Rc::new(RefCell::new(Tracker::new(store, canvas))),
            active: Rc::new(Cell::new(true)),
        }
    }
    pub(crate) fn active(&self) -> bool {
        self.active.get()
    }
    pub(crate) fn show(&self, file: Option<HashId>) {
        if self.active.get()
            && let Err(error) = self.tracker.borrow_mut().show(file, TimestampMs::now().0)
        {
            eprintln!("could not save viewing statistics: {error}");
        }
    }
    pub(crate) fn close(&self) {
        if self.active.replace(false)
            && let Err(error) = self.tracker.borrow_mut().close(TimestampMs::now().0)
        {
            eprintln!("could not save viewing statistics: {error}");
        }
    }
}
