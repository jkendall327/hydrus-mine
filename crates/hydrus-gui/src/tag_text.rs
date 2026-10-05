//! Owned live colour subscriptions for storage and write tag lists.
use hydrus_core::tag_presentation::{NamespaceColours, SiblingConnectorColours, TagPresentation};
use hydrus_store::{Store, settings};
use std::{cell::Cell, rc::Rc, sync::Arc};

pub(crate) fn watch(
    store: &Arc<Store>,
    valid: Rc<dyn Fn() -> bool>,
    changed: Rc<dyn Fn()>,
) -> Rc<slint::Timer> {
    let read = |store: &Store| {
        store.read(|conn| {
            Ok((
                settings::get::<SiblingConnectorColours>(conn)?,
                settings::get::<NamespaceColours>(conn)?,
                settings::get::<TagPresentation>(conn)?,
            ))
        })
    };
    let timer = Rc::new(slint::Timer::default());
    let store = store.clone();
    let revision = Cell::new(store.snapshot().revision);
    let mut previous = read(&store).ok();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        move || {
            if !valid() {
                return;
            }
            let current = store.snapshot().revision;
            if current == revision.get() {
                return;
            }
            if let Ok(settings) = read(&store) {
                revision.set(current);
                if previous.as_ref() != Some(&settings) {
                    previous = Some(settings);
                    changed();
                }
            }
        },
    );
    timer
}
