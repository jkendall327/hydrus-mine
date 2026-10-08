//! The options' default checker options for watchers and subscriptions
//! (downloading page) reach their consumers: a watcher page made afterwards
//! gives its watchers the new default, and a subscription added afterwards
//! starts on the new default.
use crate::subscriptions::{open_dialog, rows, store};
use hydrus_core::subscriptions::{CheckerDefaults, CheckerOptions};
use hydrus_core::url::{AnyGug, Gug, Gugs};
use hydrus_gui::checker_options::PRESETS;
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{queues, subscriptions, watchers::watcher_state};
use slint::{ComponentHandle as _, Model as _};

fn open(ui: &MainWindow) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .expect("file > options");
    ui.invoke_menu_line_clicked(0, at as i32, 0.0, 0.0, 0.0);
}

fn row(options: &OptionsWindow, label: &str) -> i32 {
    let rows = options.get_rows();
    (0..rows.row_count())
        .find(|&i| rows.row_data(i).unwrap().label == label)
        .unwrap_or_else(|| panic!("{label:?}")) as i32
}

// leaf: audit-options-downloading-watchers-default-watcher-checker-options
// leaf: audit-options-downloading-subscriptions-default-subscription-checker-options
#[test]
fn default_checker_options_are_given_to_new_watchers_and_new_subscriptions() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before: CheckerDefaults = store.read(hydrus_store::settings::get).unwrap();
    let slow_thread = PRESETS[1].1.clone();
    assert_ne!(before.watchers, slow_thread);
    assert_ne!(before.subscriptions, slow_thread);

    // Both buttons edit their own default; one Apply stores both.
    open(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    for (search, label) in [
        ("watcher checker", "Default watcher checker options:"),
        (
            "subscription checker",
            "Default subscription checker options:",
        ),
    ] {
        window.invoke_search_edited(search.into());
        window.invoke_search_chosen(0);
        window.invoke_checker_clicked(row(&window, label));
        let editor = bound
            .checker_options
            .borrow()
            .as_ref()
            .expect("the editor opens")
            .clone_strong();
        editor.invoke_preset(1);
        editor.invoke_ok();
        assert!(bound.checker_options.borrow().is_none());
    }
    let unsaved: CheckerDefaults = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(unsaved, before, "kept for Apply");
    window.invoke_apply();
    let saved: CheckerDefaults = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(saved.watchers, slow_thread);
    assert_eq!(saved.subscriptions, slow_thread);

    // A watcher page made now starts its watchers on the new default.
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(4);
    let key = bound.pages.borrow().shown().key;
    ui.invoke_watcher_urls("https://boards.example/thread/1".into());
    let made = store
        .read(move |c| queues::queues_with_page_key(c, &key.0))
        .unwrap();
    let checker: CheckerOptions = watcher_state(
        &store
            .read({
                let id = made[0].id;
                move |c| queues::queue(c, id)
            })
            .unwrap()
            .unwrap(),
    )
    .unwrap()
    .checker;
    assert_eq!(checker, slow_thread);

    // A subscription added now starts on the new subscription default.
    let gug = AnyGug::Single(Gug {
        name: "alpha".into(),
        key: "01".into(),
        url_template: "https://booru.example/search/%tags%/1".into(),
        replacement_phrase: "%tags%".into(),
        separator: "+".into(),
        initial_search_text: "tag".into(),
        example_search_text: "blue_eyes".into(),
    });
    store
        .write(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_parse::Downloaders {
                    gugs: Gugs {
                        keys_to_display: vec![gug.key().to_owned()],
                        gugs: vec![gug],
                    },
                    ..hydrus_parse::Downloaders::default()
                },
            )
        })
        .unwrap();
    let list = open_dialog(&ui, &bound);
    list.invoke_add();
    let chooser = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    chooser.invoke_accept_clicked();
    let editor = bound
        .edit_subscription
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    editor.invoke_apply();
    assert_eq!(rows(&list).len(), 1);
    list.invoke_apply();
    let subs = store.read(subscriptions::subscriptions).unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].settings.checker, slow_thread);
}

// The subscriptions page's "Sync subscriptions in random order" check box
// writes the network setting the subscription runner reads (the runner's
// random or by-name choice is tested in `hydrus-download`,
// `the_random_order_option_picks_ready_subscriptions_at_random_or_by_name`).
// leaf: audit-options-downloading-subscriptions-sync-subscriptions-in-random-order
#[test]
fn the_random_order_check_box_is_kept_for_apply_and_cancel_forgets_it() {
    use hydrus_store::network::NetworkSettings;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let read = || -> bool {
        store
            .read(hydrus_store::settings::get::<NetworkSettings>)
            .unwrap()
            .process_subs_in_random_order
    };
    let before = read();
    let label = "Sync subscriptions in random order:";
    for apply in [false, true] {
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        window.invoke_search_edited("random order".into());
        window.invoke_search_chosen(0);
        let i = row(&window, label);
        let found = window.get_rows().row_data(i as usize).unwrap();
        assert_eq!(found.kind, 1, "a check box");
        assert_eq!(found.checked, before);
        window.invoke_check_toggled(i, !before);
        assert_eq!(read(), before, "kept until Apply");
        if apply {
            window.invoke_apply();
        } else {
            window.invoke_cancel();
        }
        assert!(bound.options.borrow().is_none());
        assert_eq!(read(), if apply { !before } else { before });
    }
}
