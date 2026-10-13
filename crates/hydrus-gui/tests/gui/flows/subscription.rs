//! Flow 1 (`oracle/record_flow_subscription.py`): a subscription added
//! through network > subscriptions…, its downloader chooser and its edit
//! dialog, run by the daemon; then, once the site has a new post, checked
//! again from the dialog's "check queries now"; then the dialog reopened.
//! After each step what the user sees must be what the reference showed.

use serde_json::{Value as Json, json};
use slint::ComponentHandle as _;

use hydrus_gui::{MainWindow, Pages, SubscriptionsWindow, bind, headless};

use super::{Daemon, Known, Normaliser, Site, State, compare, install, wait_until_idle};
use crate::subscriptions::{open_dialog, rows};

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[];

/// Seconds a subscription may take to start after "apply": the reference
/// starts one three seconds after it is due; the daemon takes the client's
/// nudges every second. (Generous, for a loaded machine; without the
/// nudge it would wait up to 30 seconds, or two minutes after a run.)
const STARTS_WITHIN: u64 = 20;

fn shown(dialog: &SubscriptionsWindow, n: &Normaliser) -> Json {
    json!(
        rows(dialog)
            .into_iter()
            .map(|(cells, _)| cells.iter().map(|c| n.text(c)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    )
}

#[test]
fn add_a_subscription_run_it_and_run_it_again_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("flow_subscription.json");
    let (_dirs, store) = super::basic_store();
    install(&store, &recording);
    let site = Site::serve(&recording);
    let n = Normaliser::new(&site);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    // (popups change only on a window that is shown, as the reference's)
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let daemon = Daemon::start(&store);
    let state = State {
        store: &store,
        ui: &ui,
        site: &site,
        n: &n,
    };
    let mut steps = Vec::new();

    // 1. add: network > subscriptions…, add, the downloader, the editor
    // (named, a query added through its own dialog), apply, apply
    let list = open_dialog(&ui, &bound);
    let mut seen = json!({"rows when opened": shown(&list, &n)});
    list.invoke_add();
    let chooser = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .expect("the downloader chooser")
        .clone_strong();
    let choices = chooser.get_galleries();
    seen["downloaders"] = json!(
        (0..slint::Model::row_count(&choices))
            .map(|i| slint::Model::row_data(&choices, i).unwrap().to_string())
            .collect::<Vec<_>>()
    );
    chooser.set_selected(0);
    chooser.invoke_accept_clicked();
    let editor = bound
        .edit_subscription
        .borrow()
        .as_ref()
        .expect("the edit subscription dialog")
        .clone_strong();
    seen["name starts as"] = json!(editor.get_name().as_str());
    editor.set_name("blue eyes".into());
    editor.invoke_add_query();
    seen["query starts as"] = json!(editor.get_query_text().as_str());
    editor.set_query_text("blue_eyes".into());
    editor.invoke_query_apply();
    seen["initial file limit"] = json!(editor.get_initial_limit());
    seen["periodic file limit"] = json!(editor.get_periodic_limit());
    editor.invoke_apply();
    seen["rows at apply"] = shown(&list, &n);
    list.invoke_apply();
    // (the reference's "apply" wakes its subscriptions manager, which
    // starts what is due within seconds)
    site.wait_for_a_request(STARTS_WITHIN, "the new subscription");
    wait_until_idle(&daemon, || bound.subscriptions.borrow().is_none());
    steps.push(state.step("add", &seen));

    // 2. check again: a new post; the subscription selected, "check
    // queries now", apply
    site.set_phase(1);
    let list = open_dialog(&ui, &bound);
    let mut seen = json!({"rows when opened": shown(&list, &n)});
    list.invoke_row_clicked(0, false, false);
    list.invoke_check_now();
    assert!(!list.get_asking(), "nothing to ask of a working query");
    seen["rows at apply"] = shown(&list, &n);
    list.invoke_apply();
    // (even though it ran only a moment ago: "apply" sets the
    // subscriptions afresh)
    site.wait_for_a_request(STARTS_WITHIN, "checking the subscription now");
    wait_until_idle(&daemon, || bound.subscriptions.borrow().is_none());
    steps.push(state.step("check again", &seen));

    // 3. reopen, to see the list, and cancel
    let list = open_dialog(&ui, &bound);
    let seen = json!({"rows when opened": shown(&list, &n)});
    list.invoke_cancel();
    wait_until_idle(&daemon, || true);
    steps.push(state.step("reopen", &seen));

    compare(&steps, &recording, KNOWN);
}
