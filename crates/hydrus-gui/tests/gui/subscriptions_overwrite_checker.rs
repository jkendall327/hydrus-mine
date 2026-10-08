//! The manage subscriptions dialog's "overwrite checker options": the checker editor opens on
//! the first selected subscription's options, and what it applies replaces every selected
//! subscription's options (and only theirs) when the dialog is applied.

use slint::ComponentHandle as _;

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_gui::{MainWindow, Pages, bind, checker_options::PRESETS, headless};
use hydrus_store::subscriptions;

use crate::subscriptions::{now, open_dialog, rows, store};

// leaf: audit-network-subs-overwrite-checker
#[test]
fn overwrite_checker_options_reaches_only_the_selected_subscriptions_on_apply() {
    let (_dirs, store) = store();
    let now = now();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let settings = SubscriptionSettings {
                gug_name: "example tag search".into(),
                ..SubscriptionSettings::default()
            };
            for name in ["a", "b", "c"] {
                let id = subscriptions::create_subscription(conn, name, &settings)?.unwrap();
                subscriptions::add_query(conn, id, &QueryState::new(name), now)?;
            }
            Ok(())
        })
        .unwrap();
    let before = store.read(subscriptions::subscriptions).unwrap()[0]
        .settings
        .checker
        .clone();
    assert_ne!(before, PRESETS[1].1);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    let index = |name: &str| {
        i32::try_from(
            rows(&dialog)
                .iter()
                .position(|(cells, _)| cells[0] == name)
                .unwrap(),
        )
        .unwrap()
    };
    dialog.invoke_row_clicked(index("a"), false, false);
    dialog.invoke_row_clicked(index("b"), true, false);
    // Cancelling the editor changes nothing.
    dialog.invoke_overwrite_checker();
    let editor = bound
        .checker_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    editor.invoke_cancel();
    dialog.invoke_apply();
    let saved = store.read(subscriptions::subscriptions).unwrap();
    assert!(saved.iter().all(|s| s.settings.checker == before));
    // Choosing a reasonable default and "ok" replaces both selected subscriptions' options.
    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(index("a"), false, false);
    dialog.invoke_row_clicked(index("b"), true, false);
    dialog.invoke_overwrite_checker();
    let editor = bound
        .checker_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    editor.invoke_preset(1);
    editor.invoke_ok();
    assert!(bound.checker_options.borrow().is_none());
    // Staged only: nothing is stored until the list is applied.
    let saved = store.read(subscriptions::subscriptions).unwrap();
    assert!(saved.iter().all(|s| s.settings.checker == before));
    dialog.invoke_apply();
    let saved = store.read(subscriptions::subscriptions).unwrap();
    for s in &saved {
        let expected = if s.name == "c" {
            &before
        } else {
            &PRESETS[1].1
        };
        assert_eq!(&s.settings.checker, expected, "{}", s.name);
    }
}
