//! The manage subscriptions dialog's "separate", taking only some queries
//! out ("only extract some of the subscription"): the queries ticked in
//! the window's question panel, made one new subscription (asking its
//! name) or one each, and "apply" writing them. The questions and the
//! names are tested against the reference's in hydrus-gui-model.

use slint::Model as _;

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_gui::{MainWindow, Pages, SubscriptionsWindow, bind, headless};
use hydrus_store::subscriptions;

use crate::subscriptions::{asked, now, open_dialog, rows, store};

fn answer(dialog: &SubscriptionsWindow, label: &str) {
    let (_, _, choices) = asked(dialog);
    let i = choices
        .iter()
        .position(|c| c.starts_with(label))
        .unwrap_or_else(|| panic!("{label} in {choices:?}"));
    dialog.invoke_chosen(i32::try_from(i).unwrap());
}

fn select(dialog: &SubscriptionsWindow, name: &str) {
    let r = rows(dialog)
        .iter()
        .position(|(cells, _)| cells[0] == name)
        .unwrap();
    dialog.invoke_row_clicked(i32::try_from(r).unwrap(), false, false);
}

#[test]
fn some_queries_are_separated_and_written_on_apply() {
    let (_dirs, store) = store();
    let now = now();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let settings = SubscriptionSettings {
                gug_name: "example tag search".into(),
                ..SubscriptionSettings::default()
            };
            let id = subscriptions::create_subscription(conn, "many", &settings)?.unwrap();
            for text in ["alpha", "beta", "gamma", "delta"] {
                subscriptions::add_query(conn, id, &QueryState::new(text), now)?;
            }
            Ok(())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);

    // two ticked, made one subscription
    select(&dialog, "many");
    dialog.invoke_separate();
    answer(&dialog, "only extract some");
    let (title, _, _) = asked(&dialog);
    assert_eq!(title, "select the queries to extract");
    let ticks = dialog.get_asking_ticks();
    let labels: Vec<String> = (0..ticks.row_count())
        .map(|i| ticks.row_data(i).unwrap().label.to_string())
        .collect();
    assert_eq!(labels, ["alpha", "beta", "gamma", "delta"]);
    assert!((0..ticks.row_count()).all(|i| !ticks.row_data(i).unwrap().on));
    dialog.invoke_ticked(1, true);
    dialog.invoke_ticked(3, true);
    answer(&dialog, "ok");
    answer(&dialog, "one new merged subscription");
    let (_, message, _) = asked(&dialog);
    assert_eq!(message, "Please enter the name for the new subscription.");
    assert_eq!(dialog.get_asked_text(), "many");
    dialog.set_asked_text("b and d".into());
    answer(&dialog, "ok");

    // one ticked: its own, "base: query"
    select(&dialog, "many");
    dialog.invoke_separate();
    // (two queries left: separated whole, asking only the name)
    let (_, message, _) = asked(&dialog);
    assert_eq!(
        message,
        "Please enter the base name for the new subscriptions. They will be named '[NAME]: query'."
    );
    dialog.invoke_cancelled();

    dialog.invoke_apply();
    let mut written: Vec<(String, Vec<String>)> = store
        .read(|c| {
            subscriptions::subscriptions(c)?
                .into_iter()
                .map(|s| {
                    let queries = subscriptions::queries(c, s.id)?
                        .into_iter()
                        .map(|q| q.state.query_text)
                        .collect();
                    Ok((s.name, queries))
                })
                .collect()
        })
        .unwrap();
    written.sort();
    assert_eq!(
        written,
        [
            (
                "b and d".to_owned(),
                vec!["delta".to_owned(), "beta".to_owned()]
            ),
            (
                "many".to_owned(),
                vec!["alpha".to_owned(), "gamma".to_owned()]
            ),
        ]
    );
}
