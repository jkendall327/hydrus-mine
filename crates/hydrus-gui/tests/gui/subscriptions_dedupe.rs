//! The manage subscriptions dialog's "deduplicate", in the window: its
//! questions asked in turn in the window's panel (the texts ticked in a
//! list), the queries kept and dropped, and "apply" writing them. The
//! questions and what they do are tested against the reference's in
//! hydrus-gui-model.

use slint::Model as _;

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_gui::{MainWindow, Pages, SubscriptionsWindow, bind, headless};
use hydrus_store::subscriptions;

use crate::subscriptions::{asked, now, open_dialog, store};

fn answer(dialog: &SubscriptionsWindow, label: &str) {
    let (_, _, choices) = asked(dialog);
    let i = choices
        .iter()
        .position(|c| c.starts_with(label))
        .unwrap_or_else(|| panic!("{label} in {choices:?}"));
    dialog.invoke_chosen(i32::try_from(i).unwrap());
}

#[test]
fn duplicate_queries_are_deduplicated_and_written_on_apply() {
    let (_dirs, store) = store();
    let now = now();
    let (a, b) = store
        .write(move |ctx| {
            let conn = ctx.conn();
            let settings = SubscriptionSettings {
                gug_name: "example tag search".into(),
                ..SubscriptionSettings::default()
            };
            let a = subscriptions::create_subscription(conn, "sub a", &settings)?.unwrap();
            let b = subscriptions::create_subscription(conn, "sub b", &settings)?.unwrap();
            for text in ["blue", "Red", "green"] {
                subscriptions::add_query(conn, a, &QueryState::new(text), now)?;
            }
            for text in ["blue", "red", "green", "yellow"] {
                subscriptions::add_query(conn, b, &QueryState::new(text), now)?;
            }
            Ok((a, b))
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    assert!(dialog.get_can_dedupe());

    dialog.invoke_deduplicate();
    let (title, _, _) = asked(&dialog);
    assert_eq!(title, "Caseless or cased?");
    answer(&dialog, "do a normal");
    // one downloader: straight to the texts
    let (_, message, choices) = asked(&dialog);
    assert_eq!(
        message,
        "There are 3 duplicate query texts for the downloader \"example tag search\". Would you like to dedupe them all, or select which to do?"
    );
    assert_eq!(choices, ["do them all", "select which to do", "forget it"]);
    answer(&dialog, "select which");
    // the texts, all ticked; "green" unticked
    let ticks = dialog.get_asking_ticks();
    let labels: Vec<String> = (0..ticks.row_count())
        .map(|i| ticks.row_data(i).unwrap().label.to_string())
        .collect();
    assert_eq!(labels, ["blue", "green", "red"]);
    assert!((0..ticks.row_count()).all(|i| ticks.row_data(i).unwrap().on));
    dialog.invoke_ticked(1, false);
    assert!(!dialog.get_asking_ticks().row_data(1).unwrap().on);
    let pixels = headless::render(&windows.get(1).unwrap(), 900, 700);
    let shot = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("subscriptions_dedupe.png");
    headless::save_png(&shot, &pixels, 900, 700).unwrap();
    answer(&dialog, "ok");
    // both have both: "sub a" keeps them
    let (title, _, choices) = asked(&dialog);
    assert_eq!(title, "Which sub to retain the queries on?");
    assert_eq!(
        choices,
        [
            "sub a (has all duplicate queries)",
            "sub b (has all duplicate queries)"
        ]
    );
    answer(&dialog, "sub a");
    assert!(!dialog.get_asking());
    // "green" is still duplicated
    assert!(dialog.get_can_dedupe());

    dialog.invoke_apply();
    let texts = |id: i64| -> Vec<String> {
        let mut texts: Vec<String> = store
            .read(|c| subscriptions::queries(c, id))
            .unwrap()
            .into_iter()
            .map(|q| q.state.query_text)
            .collect();
        texts.sort();
        texts
    };
    assert_eq!(texts(a), ["Red", "blue", "green"]);
    assert_eq!(texts(b), ["green", "yellow"]);
}
