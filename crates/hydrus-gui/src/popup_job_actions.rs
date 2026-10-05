//! Concrete popup job actions use stable keys and current producer-owned payloads.
use crate::MainWindow;
use hydrus_store::{
    Store,
    popup_actions::{self, Request},
    popups,
};
use slint::ComponentHandle as _;
use std::{cell::Cell, rc::Rc, sync::Arc};
fn key(text: &str) -> Option<[u8; 32]> {
    hex::decode(text).ok()?.try_into().ok()
}
fn now() -> i64 {
    hydrus_core::TimestampMs::now().0 / 1000
}
pub(crate) fn bind(
    window: &MainWindow,
    store: Arc<Store>,
    active: Rc<Cell<bool>>,
    gui_owner: [u8; 32],
    refresh: Rc<dyn Fn()>,
) {
    let valid: Rc<dyn Fn(&str) -> bool> = Rc::new({
        let weak = window.as_weak();
        move |source| {
            key(source) == Some(gui_owner)
                && active.get()
                && weak
                    .upgrade()
                    .is_some_and(|w| crate::popup_freeze::accepts_input(w.window()))
        }
    });
    window.on_popup_copy_payload({
        let valid = valid.clone();
        let store = store.clone();
        move |job_key, owner, source| {
            if !valid(&source) {
                return;
            }
            let Some(key) = key(&job_key) else {
                return;
            };
            let owner = if owner.is_empty() {
                None
            } else {
                let Some(owner) = self::key(&owner) else {
                    return;
                };
                Some(owner)
            };
            if let Ok(Some(job)) = store.read(move |conn| popups::get(conn, &key, now()))
                && job.action_owner == owner
                && let Some((_, text)) = job.popup_clipboard
            {
                crate::copy_to_clipboard(&text);
            }
        }
    });
    window.on_popup_call({
        let valid = valid.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        move |job_key, owner, source| {
            if !valid(&source) {
                return;
            }
            let (Some(key), Some(owner)) = (key(&job_key), key(&owner)) else {
                return;
            };
            if let Err(error) = store.write(move |ctx| {
                popup_actions::request_from_gui(
                    ctx.conn(),
                    &key,
                    &owner,
                    &gui_owner,
                    now(),
                    Request::Call,
                )
            }) {
                eprintln!("Could not dispatch popup command: {error}");
            }
            refresh();
        }
    });
    window.on_popup_answer(move |job_key, owner, question, source, answer| {
        if !valid(&source) {
            return;
        }
        let (Some(key), Some(owner), Some(question)) = (key(&job_key), key(&owner), key(&question))
        else {
            return;
        };
        if let Err(error) = store.write(move |ctx| {
            popup_actions::request_from_gui(
                ctx.conn(),
                &key,
                &owner,
                &gui_owner,
                now(),
                Request::Answer { question, answer },
            )
        }) {
            eprintln!("Could not answer popup question: {error}");
        }
        refresh();
    });
}
