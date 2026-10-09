//! Owned per-service most-used drafts, with the existing write-tag child.
use crate::MostUsedTagsWindow;
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};

pub type Tags = BTreeMap<String, Vec<String>>;
/// Explicit Options-owned inspection/cancellation slots; no registry.
#[derive(Clone, Default)]
pub struct Slots {
    pub editor: Rc<RefCell<Option<MostUsedTagsWindow>>>,
    pub tags: crate::write_tag_window::Slot,
    pub weights: crate::related_weights_window::Slot,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots").finish_non_exhaustive()
    }
}
/// Force-cancel descendants before their owner retires.
pub fn cancel(slots: &Slots) {
    crate::related_weights_window::cancel(&slots.weights);
    let child = slots
        .editor
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(child) = child {
        child.invoke_cancel();
    }
}
/// Select real tag services independently; only accepted lists change the draft.
pub fn open(
    store: &Arc<Store>,
    slots: &Slots,
    initial: Tags,
    parent_active: Rc<Cell<bool>>,
    applied: Rc<dyn Fn(Tags)>,
) -> Result<MostUsedTagsWindow, slint::PlatformError> {
    if let Some(w) = slots.editor.borrow().as_ref() {
        w.show()?;
        return Ok(w.clone_strong());
    }
    let w = crate::app_title::new::<crate::MostUsedTagsWindow>()?;
    let mut services: Vec<_> = store
        .snapshot()
        .services
        .tag_services()
        .map(|s| (s.key.clone(), s.name.clone()))
        .collect();
    services.sort_by_cached_key(|(_, name)| hydrus_core::sort::human_sort_key(name));
    let services = Rc::new(services);
    let draft = Rc::new(RefCell::new(initial));
    let selected = Rc::new(Cell::new(0usize));
    let active = Rc::new(Cell::new(true));
    crate::gui_colours::bind(w.global::<crate::Theme<'_>>(), store, active.clone());
    w.set_services(ModelRc::new(VecModel::from(
        services
            .iter()
            .map(|(_, name)| SharedString::from(name.as_str()))
            .collect::<Vec<_>>(),
    )));
    let refresh = Rc::new({
        let weak = w.as_weak();
        let services = services.clone();
        let draft = draft.clone();
        let selected = selected.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                w.set_service(i32::try_from(selected.get()).unwrap_or(0));
                let mut tags = services
                    .get(selected.get())
                    .and_then(|(key, _)| draft.borrow().get(&key.to_hex()).cloned())
                    .unwrap_or_default();
                hydrus_core::sort::human_sort(&mut tags);
                w.set_tags(ModelRc::new(VecModel::from(
                    tags.into_iter().map(SharedString::from).collect::<Vec<_>>(),
                )));
            }
        }
    });
    w.on_service_chosen({
        let active = active.clone();
        let parent = parent_active.clone();
        let slots = slots.clone();
        let selected = selected.clone();
        let services = services.clone();
        let refresh = refresh.clone();
        move |i| {
            if !active.get() || !parent.get() || slots.tags.borrow().is_some() {
                return;
            }
            if let Ok(i) = usize::try_from(i)
                && i < services.len()
            {
                selected.set(i);
                refresh();
            }
        }
    });
    w.on_edit_tags({
        let active = active.clone();
        let parent = parent_active.clone();
        let slots = slots.clone();
        let store = store.clone();
        let selected = selected.clone();
        let services = services.clone();
        let draft = draft.clone();
        let refresh = refresh.clone();
        let weak = w.as_weak();
        move || {
            if !active.get() || !parent.get() || slots.tags.borrow().is_some() {
                return;
            }
            let Some((key, _)) = services.get(selected.get()) else {
                return;
            };
            let key = key.clone();
            let hex = key.to_hex();
            let initial = draft.borrow().get(&hex).cloned().unwrap_or_default();
            let accepted = Rc::new({
                let active = active.clone();
                let parent = parent.clone();
                let draft = draft.clone();
                let refresh = refresh.clone();
                move |tags: Vec<String>, _: Vec<String>| {
                    if active.get() && parent.get() {
                        draft.borrow_mut().insert(hex.clone(), tags);
                        refresh();
                    }
                }
            });
            let closed = Rc::new({
                let weak = weak.clone();
                move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_child_open(false);
                    }
                }
            });
            if let Some(w) = weak.upgrade() {
                w.set_child_open(true);
            }
            if let Err(error) = crate::write_tag_window::open_additions(
                &store,
                key,
                &initial,
                "most used tags",
                &slots.tags,
                accepted,
                closed,
            ) {
                if let Some(w) = weak.upgrade() {
                    w.set_child_open(false);
                }
                eprintln!("could not edit most used tags: {error}");
            }
        }
    });
    let close = Rc::new({
        let active = active.clone();
        let weak = w.as_weak();
        let slots = slots.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let child = slots
                .tags
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(child) = child {
                child.invoke_cancel();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            slots.editor.borrow_mut().take();
        }
    });
    w.on_apply({
        let active = active.clone();
        let close = close.clone();
        let slots = slots.clone();
        move || {
            if active.get() && parent_active.get() && slots.tags.borrow().is_none() {
                applied(draft.borrow().clone());
                close();
            }
        }
    });
    w.on_cancel({
        let close = close.clone();
        move || close()
    });
    w.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    w.show()?;
    *slots.editor.borrow_mut() = Some(w.clone_strong());
    Ok(w)
}
