//! Saved policy is read at the physical wheel boundary, inside one GUI owner.
use std::{rc::Rc, sync::Arc};
pub(crate) fn bind(
    policy: crate::MenuChoicePolicy<'_>,
    store: &Arc<hydrus_store::Store>,
    valid: Rc<dyn Fn() -> bool>,
) {
    policy.on_owner_valid({
        let valid = valid.clone();
        move || valid()
    });
    policy.on_allowed({
        let store = Arc::downgrade(store);
        move || {
            valid()
                && store.upgrade().is_some_and(|store| {
                    store
                        .read(hydrus_store::menu_choice_wheel::load)
                        .is_ok_and(|value| value.enabled)
                })
        }
    });
    policy.on_next(|current, count, dy| {
        let (Ok(current), Ok(count)) = (usize::try_from(current), usize::try_from(count)) else {
            return -1;
        };
        hydrus_gui_model::menu_choice_wheel::next(current, count, dy)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    });
}

/// A modal owner slot blocks input without making its window own itself.
pub(crate) fn occupied<T: 'static>(
    slot: &Rc<std::cell::RefCell<Option<T>>>,
) -> Rc<dyn Fn() -> bool> {
    let slot = Rc::downgrade(slot);
    Rc::new(move || slot.upgrade().is_some_and(|slot| slot.borrow().is_some()))
}
