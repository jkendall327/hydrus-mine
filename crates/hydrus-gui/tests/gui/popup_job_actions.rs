//! Live producer → SQLite transport → actual native popup controls → effects.
use hydrus_download::popups::Working;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::popups;
use slint::{ComponentHandle as _, Model as _};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn current(store: &hydrus_store::Store) -> popups::Job {
    store
        .read(|conn| popups::all(conn, hydrus_core::TimestampMs::now().0 / 1000))
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
}
#[test]
fn clipboard_and_repeatable_current_callable_reach_real_native_controls_and_store_effects() {
    let (_directories, store) = crate::subscriptions::store();
    let windows = headless::init();
    let producer = Working::new(&store, "live actions", true);
    let fixture = hydrus_testkit::fixture_json("popup_actions.json");
    let payload = fixture["payload"].as_str().unwrap().to_owned();
    producer.set_clipboard(Some(("copy full payload".into(), payload.clone())));
    let called = Arc::new(AtomicUsize::new(0));
    producer.set_user_callable("repeat command", {
        let called = called.clone();
        let store = store.clone();
        move || {
            called.fetch_add(1, Ordering::SeqCst);
            store
                .write(|ctx| {
                    popups::add(
                        ctx.conn(),
                        &popups::Job::text("actual callable effect", 0.0),
                        0,
                    )
                })
                .unwrap();
        }
    });
    producer.show();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let row = ui.get_popups().row_data(0).unwrap();
    assert!(row.has_clipboard && row.has_callable);
    assert_eq!(row.clipboard, "copy full payload");
    let pixels = headless::render(&windows.get(0).unwrap(), 1100, 700);
    assert!(!pixels.is_empty());
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("popup_job_actions.png"),
        &pixels,
        1100,
        700,
    )
    .unwrap();
    ui.invoke_popup_copy_payload(
        row.key.clone(),
        row.action_owner.clone(),
        row.gui_owner.clone(),
    );
    assert_eq!(
        headless::clipboard_text().as_deref(),
        Some(payload.as_str())
    );
    ui.invoke_popup_call(
        row.key.clone(),
        row.action_owner.clone(),
        row.gui_owner.clone(),
    );
    ui.invoke_popup_call(
        row.key.clone(),
        row.action_owner.clone(),
        row.gui_owner.clone(),
    );
    producer.poll_actions();
    assert_eq!(called.load(Ordering::SeqCst), 2);
    assert_eq!(
        store
            .read(|conn| popups::all(conn, 0))
            .unwrap()
            .iter()
            .filter(|j| j.status_text_1.as_deref() == Some("actual callable effect"))
            .count(),
        2
    );
    producer.set_clipboard(Some((
        "replacement clipboard".into(),
        "replacement text".into(),
    )));
    // The row still displays the earlier label, but the handler reads current payload.
    ui.invoke_popup_copy_payload(
        row.key.clone(),
        row.action_owner.clone(),
        row.gui_owner.clone(),
    );
    assert_eq!(
        headless::clipboard_text().as_deref(),
        Some("replacement text")
    );
    producer.set_clipboard(None);
    headless::set_clipboard_text("removed payload");
    ui.invoke_popup_copy_payload(
        row.key.clone(),
        row.action_owner.clone(),
        row.gui_owner.clone(),
    );
    assert_eq!(
        headless::clipboard_text().as_deref(),
        Some("removed payload")
    );
    let replacement = Arc::new(AtomicUsize::new(0));
    producer.set_user_callable("replacement command", {
        let replacement = replacement.clone();
        move || {
            replacement.fetch_add(1, Ordering::SeqCst);
        }
    });
    ui.invoke_popup_cancel(0);
    ui.invoke_popup_call(
        row.key.clone(),
        row.action_owner.clone(),
        row.gui_owner.clone(),
    );
    producer.poll_actions();
    assert_eq!(
        replacement.load(Ordering::SeqCst),
        1,
        "Qt callable remains active after Cancel"
    );
    producer.finish();
    producer.clear_user_callable();
    ui.invoke_popup_call(
        row.key.clone(),
        row.action_owner.clone(),
        row.gui_owner.clone(),
    );
    producer.poll_actions();
    assert_eq!(replacement.load(Ordering::SeqCst), 1);
    // A retired GUI remains inert even if a retained window is shown again.
    headless::set_clipboard_text("retired");
    drop(bound);
    ui.show().unwrap();
    ui.invoke_popup_copy_payload(row.key, row.action_owner, row.gui_owner);
    assert_eq!(headless::clipboard_text().as_deref(), Some("retired"));
}
#[test]
fn yes_and_no_dismiss_the_popup_and_reach_the_retained_producer_once() {
    let (_directories, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    for answer in [true, false] {
        let producer = Working::new(&store, "question", true);
        producer.set_question(Some("Accept this?".into()));
        producer.show();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        let row = ui.get_popups().row_data(0).unwrap();
        assert!(row.has_question);
        assert_eq!(row.question, "Accept this?");
        // Capture real Slint yes/no controls once, alongside the existing answer
        // lifetime regression, then prove dismissal reaches the rendered stack.
        let question_pixels = answer.then(|| {
            let pixels = headless::render(&windows.get(0).unwrap(), 1100, 700);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("popup_job_question.png"),
                &pixels,
                1100,
                700,
            )
            .unwrap();
            pixels
        });
        ui.invoke_popup_answer(
            row.key.clone(),
            row.action_owner.clone(),
            row.question_token.clone(),
            row.gui_owner.clone(),
            answer,
        );
        assert_eq!(
            ui.get_popups().row_count(),
            0,
            "answer immediately finishes and dismisses"
        );
        assert!(store.read(|conn| popups::all(conn, 0)).unwrap().is_empty());
        if let Some(before) = question_pixels {
            let after = headless::render(&windows.get(0).unwrap(), 1100, 700);
            assert_ne!(
                before, after,
                "accepted answer removes the rendered question controls"
            );
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("popup_job_question_dismissed.png"),
                &after,
                1100,
                700,
            )
            .unwrap();
        }
        drop(bound); // An already accepted decision survives closing its GUI.
        assert_eq!(producer.question_answer(), Some(answer));
        ui.invoke_popup_answer(
            row.key,
            row.action_owner,
            row.question_token,
            row.gui_owner,
            !answer,
        );
        assert_eq!(producer.question_answer(), Some(answer));
    }
}
#[test]
fn removed_question_tokens_and_retired_producers_cannot_answer_successors() {
    let (_directories, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let old = Working::new(&store, "old question", true);
    old.set_question(Some("first?".into()));
    old.show();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let first = ui.get_popups().row_data(0).unwrap();
    old.set_question(Some("second?".into()));
    ui.invoke_popup_answer(
        first.key.clone(),
        first.action_owner.clone(),
        first.question_token.clone(),
        first.gui_owner.clone(),
        true,
    );
    assert_eq!(old.question_answer(), None);
    assert_eq!(current(&store).popup_yes_no_question.unwrap().1, "second?");
    drop(old);
    let successor = Working::new(&store, "successor", true);
    successor.set_question(Some("new owner?".into()));
    successor.show();
    ui.invoke_popup_answer(
        first.key,
        first.action_owner,
        first.question_token,
        first.gui_owner,
        false,
    );
    assert_eq!(successor.question_answer(), None);
    assert_eq!(current(&store).status_title.as_deref(), Some("successor"));
    drop(bound);
    let reopened = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let row = ui.get_popups().row_data(0).unwrap();
    ui.invoke_popup_answer(
        row.key,
        row.action_owner,
        row.question_token,
        row.gui_owner,
        true,
    );
    assert_eq!(successor.question_answer(), Some(true));
    drop(reopened);
}

#[test]
fn accepted_close_and_rebinding_retire_queued_calls_even_with_retained_windows_and_bindings() {
    let (_directories, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let producer = Working::new(&store, "owner boundary", true);
    let called = Arc::new(AtomicUsize::new(0));
    producer.set_user_callable("run", {
        let called = called.clone();
        move || {
            called.fetch_add(1, Ordering::SeqCst);
        }
    });
    producer.set_clipboard(Some(("copy".into(), "live".into())));
    producer.show();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let old_bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let old = ui.get_popups().row_data(0).unwrap();
    ui.invoke_popup_call(
        old.key.clone(),
        old.action_owner.clone(),
        old.gui_owner.clone(),
    );
    ui.invoke_popup_retire_owner(); // The accepted-close composition invokes this hook.
    ui.hide().unwrap();
    ui.show().unwrap();
    ui.invoke_popup_call(
        old.key.clone(),
        old.action_owner.clone(),
        old.gui_owner.clone(),
    );
    headless::set_clipboard_text("retired");
    ui.invoke_popup_copy_payload(
        old.key.clone(),
        old.action_owner.clone(),
        old.gui_owner.clone(),
    );
    producer.poll_actions();
    assert_eq!(called.load(Ordering::SeqCst), 0);
    assert_eq!(headless::clipboard_text().as_deref(), Some("retired"));
    let rebound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let current = ui.get_popups().row_data(0).unwrap();
    assert_ne!(old.gui_owner, current.gui_owner);
    ui.invoke_popup_call(
        current.key.clone(),
        current.action_owner.clone(),
        current.gui_owner.clone(),
    );
    // Another rebind retires its queued call without requiring Bound destruction.
    let latest = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    producer.poll_actions();
    assert_eq!(called.load(Ordering::SeqCst), 0);
    let row = ui.get_popups().row_data(0).unwrap();
    ui.invoke_popup_call(current.key, current.action_owner, current.gui_owner);
    ui.invoke_popup_call(row.key, row.action_owner, row.gui_owner);
    producer.poll_actions();
    assert_eq!(called.load(Ordering::SeqCst), 1);
    drop((old_bound, rebound, latest));
}
