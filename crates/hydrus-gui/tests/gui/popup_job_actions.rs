//! Live producer → SQLite transport → actual native popup controls → effects.
use hydrus_download::popups::Working;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::popups;
use slint::{
    ComponentHandle as _, Model as _,
    platform::{PointerEventButton, WindowEvent},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};
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
    hydrus_gui::set_clipper(|clip| {
        if let hydrus_gui::Clip::Text(text) = clip {
            headless::set_clipboard_text(text);
        }
    });
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

#[test]
fn cloned_bound_keeps_popup_actions_live_until_its_last_owner_drops() {
    let (_directories, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let producer = Working::new(&store, "shared binding", true);
    let called = Arc::new(AtomicUsize::new(0));
    producer.set_user_callable("run", {
        let called = called.clone();
        move || {
            called.fetch_add(1, Ordering::SeqCst);
        }
    });
    producer.show();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let original = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let retained = original.clone();
    let row = ui.get_popups().row_data(0).unwrap();
    assert!(row.has_callable);
    let call = || {
        ui.invoke_popup_call(
            row.key.clone(),
            row.action_owner.clone(),
            row.gui_owner.clone(),
        );
    };

    drop(original);
    call();
    producer.poll_actions();
    assert_eq!(
        called.load(Ordering::SeqCst),
        1,
        "dropping one Bound clone preserves the shared live popup owner"
    );

    call(); // A queued call from this GUI must also retire with the final owner.
    drop(retained);
    assert!(ui.window().is_visible());
    assert_eq!(
        ui.get_popups().row_data(0).unwrap().gui_owner,
        row.gui_owner
    );
    call(); // The same retained visible window and displayed token are now inert.
    producer.poll_actions();
    assert_eq!(called.load(Ordering::SeqCst), 1);

    let job = current(&store);
    let key = job.key;
    let owner = job.action_owner.unwrap();
    assert_eq!(job.user_callable_label.as_deref(), Some("run"));
    assert!(
        store
            .write(move |ctx| hydrus_store::popup_actions::request(
                ctx.conn(),
                &key,
                &owner,
                hydrus_core::TimestampMs::now().0 / 1000,
                hydrus_store::popup_actions::Request::Call,
            ))
            .unwrap(),
        "retiring the GUI does not retire its independent producer"
    );
    producer.poll_actions();
    assert_eq!(called.load(Ordering::SeqCst), 2);
}

#[test]
fn long_question_and_action_controls_render_with_fixed_and_narrow_popup_caps() {
    // This integration regression also uses the separately implemented width
    // preferences and live geometry observers; those APIs are in the combined tree.
    let (_directories, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let question = "Continue this synthetic worker with its current settings? ".repeat(4);
    let measured = Rc::new(RefCell::new(BTreeMap::new()));
    ui.on_popup_control_measured({
        let weak = ui.as_weak();
        let measured = measured.clone();
        move |index, key, owner, control, button, card| {
            if let Some(ui) = weak.upgrade()
                && let Ok(index) = usize::try_from(index)
                && let Some(row) = ui.get_popups().row_data(index)
                && row.key == key
                && row.gui_owner == owner
            {
                measured
                    .borrow_mut()
                    .insert(control.to_string(), (button, card));
            }
        }
    });
    let reference = hydrus_testkit::fixture_json("popup_question_layout.json");
    let mut previous_cap = None;
    for (case, (characters, fixed, filename)) in [
        (32, true, "popup_job_question_fixed32.png"),
        (16, false, "popup_job_question_narrow16.png"),
    ]
    .into_iter()
    .enumerate()
    {
        measured.borrow_mut().clear();
        let expected = &reference["cases"][case];
        assert_eq!(expected["characters"], characters);
        assert_eq!(expected["fixed"], fixed);
        assert_eq!(expected["question"], question);
        assert_eq!(expected["cancelled"], true);
        for value in expected["controls"].as_object().unwrap().values() {
            assert!(
                value["y"].as_f64().unwrap() + value["height"].as_f64().unwrap()
                    <= expected["height"].as_f64().unwrap()
            );
        }
        store
            .write(move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_store::popup_width::PopupWidth { characters, fixed },
                )
            })
            .unwrap();
        let producer = Working::new(&store, "question and actions", true);
        producer.set_question(Some(question.clone()));
        producer.set_clipboard(Some((
            "copy payload".into(),
            "full synthetic payload".into(),
        )));
        producer.set_user_callable("run command", {
            let store = store.clone();
            move || hydrus_download::popups::show_text(&store, "synthetic command completed")
        });
        producer.show();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        let row = ui.get_popups().row_data(0).unwrap();
        assert!(row.has_question && row.has_clipboard && row.has_callable);
        assert_eq!(row.question, question);
        assert_eq!(i64::from(row.width_characters), characters);
        assert_eq!(row.fixed_width, fixed);
        let native = windows.get(0).unwrap();
        let _first = crate::popup_width::render_settled(&ui, &native, 1280, 1200);
        // A width notification is not a settled height. Pump actual observers
        // until all five real Button frames agree across subsequent renders;
        // containment is asserted afterwards, never used to hide a bad frame.
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        let mut previous = None;
        let mut unchanged_since = std::time::Instant::now();
        let pixels = loop {
            let pixels = headless::render(&native, 1280, 1200);
            let frames = measured.borrow().clone();
            if previous.as_ref() != Some(&frames) {
                unchanged_since = std::time::Instant::now();
            } else if frames.len() == 5 && unchanged_since.elapsed() >= Duration::from_millis(5) {
                // Each observer's 1 ms timer has had another opportunity to
                // publish any layout changed by the preceding render.
                break pixels;
            }
            previous = Some(frames);
            assert!(
                std::time::Instant::now() < deadline,
                "actual popup button frames must settle"
            );
            std::thread::yield_now();
        };
        for control in ["yes", "no", "clipboard", "callable", "cancel"] {
            let (button, card) = measured.borrow().get(control).unwrap().clone();
            assert!(button.width > 0.0 && button.height > 0.0);
            assert!(
                button.x >= card.x + 1.0 && button.y >= card.y + 1.0,
                "{control} starts outside its card: {button:?}, {card:?}"
            );
            assert!(
                button.x + button.width <= card.x + card.width - 1.0
                    && button.y + button.height <= card.y + card.height - 1.0,
                "{control} is clipped by its card: {button:?}, {card:?}"
            );
        }
        let width = ui.get_popup_card_widths().row_data(0).unwrap();
        let cap = ui.get_popup_card_caps().row_data(0).unwrap();
        assert!(
            width > 0.0 && cap > 0.0 && width <= cap + 1.0,
            "question/action card {width} exceeds cap {cap}"
        );
        if fixed {
            assert!(
                (width - cap).abs() <= 1.0,
                "fixed question card {width} differs from cap {cap}"
            );
        }
        if let Some(previous) = previous_cap {
            assert!(
                cap < previous,
                "narrow successor must consume its smaller saved cap"
            );
        }
        previous_cap = Some(cap);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(filename),
            &pixels,
            1280,
            1200,
        )
        .unwrap();
        let (stop, card) = measured.borrow().get("cancel").unwrap().clone();
        let x = stop.x + stop.width / 2.0;
        let bottom = stop.y + stop.height - 1.0;
        let pixel = |x: f32, y: f32| {
            let offset = (y.floor() as usize * 1280 + x.floor() as usize) * 4;
            &pixels[offset..offset + 4]
        };
        assert_ne!(
            pixel(x, bottom),
            pixel(card.x + 8.0, bottom),
            "the stop button's lower border must actually paint inside its card"
        );
        // Deliver a real pointer near the lower edge, which the old clipped
        // card could neither paint nor route to its Cancel button.
        let position = slint::LogicalPosition::new(x, stop.y + stop.height * 0.9);
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        producer.poll_actions();
        assert!(
            producer.is_cancelled(),
            "the lower stop-button area must reach the live producer"
        );
        producer.finish_and_dismiss();
        drop(bound);
    }
}
