//! The bandwidth review's confirmed actions and rules editor, through the real window and store.
use hydrus_core::{
    bandwidth::{BandwidthType, Rule, Rules},
    network::NetworkContext,
};
use hydrus_gui::{headless, network_data_window as windows};
use hydrus_store::{Store, bandwidth::BandwidthSettings, settings};
use slint::{ComponentHandle as _, Model as _};

fn until(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < std::time::Duration::from_secs(8));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn rules_of(store: &Store, context: &NetworkContext) -> Option<Vec<Rule>> {
    store
        .read(settings::get::<BandwidthSettings>)
        .unwrap()
        .rules
        .iter()
        .find(|(c, _)| c == context)
        .map(|(_, r)| r.rules().to_vec())
}
fn select(review: &hydrus_gui::BandwidthWindow, name: &str) {
    until(|| {
        review
            .get_rows()
            .iter()
            .any(|r| r.cells.row_data(0).unwrap().contains(name))
    });
    let row = review
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap().contains(name))
        .unwrap();
    review.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
}

// leaf: audit-network-bandwidth-inherit
#[test]
fn confirmed_revert_removes_only_that_override() {
    let _h = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let a = NetworkContext::domain("a.example.com");
    let b = NetworkContext::domain("b.example.com");
    store
        .write({
            let (a, b) = (a.clone(), b.clone());
            move |ctx| {
                let mut s = settings::get::<BandwidthSettings>(ctx.conn())?;
                s.watcher_page_wait = 21;
                for c in [a, b] {
                    s.rules.push((
                        c,
                        Rules::new([Rule::new(BandwidthType::Requests, Some(60), 5)]),
                    ));
                }
                settings::set(ctx.conn(), &s)
            }
        })
        .unwrap();
    let review = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| review.get_status().contains("offline"));
    review.set_show_all(true);
    review.invoke_refresh();
    select(&review, "a.example.com");
    until(|| review.get_can_revert());
    review.invoke_revert_clicked();
    assert!(review.get_question().contains("default rules"));
    review.invoke_answer(true);
    until(|| rules_of(&store, &a).is_none());
    assert!(rules_of(&store, &b).is_some());
    assert_eq!(
        store
            .read(settings::get::<BandwidthSettings>)
            .unwrap()
            .watcher_page_wait,
        21
    );
    until(|| !review.get_busy());
    review.invoke_close_clicked();
}

// leaf: bandwidth-reset-default
#[test]
fn confirmed_reset_defaults_keeps_specific_overrides() {
    let _h = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let a = NetworkContext::domain("a.example.com");
    let global = NetworkContext::global();
    store
        .write({
            let (a, global) = (a.clone(), global.clone());
            move |ctx| {
                let mut s = settings::get::<BandwidthSettings>(ctx.conn())?;
                s.rules.retain(|(c, _)| c != &global);
                s.rules.push((
                    global,
                    Rules::new([Rule::new(BandwidthType::Requests, Some(60), 1)]),
                ));
                s.rules.push((
                    a,
                    Rules::new([Rule::new(BandwidthType::Requests, Some(60), 5)]),
                ));
                settings::set(ctx.conn(), &s)
            }
        })
        .unwrap();
    let review = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| review.get_status().contains("offline"));
    review.invoke_reset_clicked();
    assert!(!review.get_question().is_empty());
    review.invoke_answer(false);
    assert_eq!(rules_of(&store, &global).unwrap().len(), 1);
    review.invoke_reset_clicked();
    review.invoke_answer(true);
    until(|| {
        rules_of(&store, &global) != Some(vec![Rule::new(BandwidthType::Requests, Some(60), 1)])
    });
    assert_eq!(
        rules_of(&store, &a),
        Some(vec![Rule::new(BandwidthType::Requests, Some(60), 5)])
    );
    review.invoke_close_clicked();
}

// leaf: audit-network-rules-list
// leaf: bandwidth-rule-list
// leaf: audit-network-rules-kind
// leaf: audit-network-rules-interval
#[test]
fn rules_editor_adds_replaces_and_deletes_a_multiselection() {
    let _h = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let review = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| review.get_status().contains("offline"));
    review.set_domain("rules.example.com".into());
    review.invoke_domain_clicked();
    let edit = windows::last_rules().unwrap();
    let base = edit.get_rows().row_count();
    // Kind and amount validation.
    edit.set_requests(true);
    edit.set_amount("abc".into());
    edit.invoke_add_rule();
    assert!(!edit.get_error().is_empty());
    edit.set_amount("0".into());
    edit.invoke_add_rule();
    assert!(edit.get_error().contains("positive"));
    // Rolling interval needs positive seconds; monthly ignores them.
    edit.set_amount("7".into());
    edit.set_monthly(false);
    edit.set_seconds("0".into());
    edit.invoke_add_rule();
    assert!(!edit.get_error().is_empty());
    assert_eq!(edit.get_rows().row_count(), base);
    edit.set_seconds("90".into());
    edit.invoke_add_rule();
    assert_eq!(edit.get_rows().row_count(), base + 1);
    edit.set_requests(false);
    edit.set_monthly(true);
    edit.set_amount("2048".into());
    edit.invoke_add_rule();
    assert_eq!(edit.get_rows().row_count(), base + 2);
    // Replace the selected (last) row.
    edit.invoke_row_clicked(i32::try_from(base + 1).unwrap(), false, false);
    edit.set_amount("4096".into());
    edit.invoke_replace_rule();
    assert_eq!(edit.get_rows().row_count(), base + 2);
    // Multi-select the two new rows and delete them.
    edit.invoke_row_clicked(i32::try_from(base).unwrap(), false, false);
    edit.invoke_row_clicked(i32::try_from(base + 1).unwrap(), true, false);
    edit.invoke_delete_rule();
    assert_eq!(edit.get_rows().row_count(), base);
    // Nothing was persisted by editing alone; add again and apply.
    assert!(rules_of(&store, &NetworkContext::domain("rules.example.com")).is_none());
    edit.set_requests(true);
    edit.set_monthly(false);
    edit.set_amount("7".into());
    edit.set_seconds("90".into());
    edit.invoke_add_rule();
    edit.set_requests(false);
    edit.set_monthly(true);
    edit.set_amount("4096".into());
    edit.invoke_add_rule();
    edit.invoke_apply_clicked();
    until(|| windows::last_rules().is_none());
    let saved = rules_of(&store, &NetworkContext::domain("rules.example.com")).unwrap();
    assert!(saved.contains(&Rule::new(BandwidthType::Requests, Some(90), 7)));
    assert!(saved.contains(&Rule::new(BandwidthType::Data, None, 4096)));
    review.invoke_close_clicked();
}

// leaf: audit-network-cookies-expiry
#[test]
fn cookie_editor_expiry_modes_validate_and_persist() {
    use hydrus_gui::network_sessions_window as sessions;
    use hydrus_store::network;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _h = headless::init();
    let slots = sessions::Slots::default();
    let browser = sessions::open(&store, &slots, false).unwrap();
    browser.invoke_add_clicked();
    let edit = sessions::last_edit_opened().unwrap();
    edit.set_domain("example.com".into());
    edit.invoke_apply_clicked();
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    cookies.invoke_add_clicked();
    let edit = sessions::last_edit_opened().unwrap();
    edit.set_name("sid".into());
    edit.set_value("v".into());
    // Delta from now: below 1200 seconds is refused, otherwise it fills the absolute expiry.
    edit.set_expiry_delta("1199".into());
    edit.invoke_delta_clicked();
    assert!(edit.get_error().contains("1200 seconds"));
    edit.set_expiry_delta("3600".into());
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    edit.invoke_delta_clicked();
    assert!(edit.get_error().is_empty());
    assert!(!edit.get_session_cookie());
    let expires: i64 = edit.get_expires().parse().unwrap();
    assert!((before + 3600..=before + 3610).contains(&expires));
    // A non-numeric absolute expiry is refused and keeps the editor open.
    edit.set_expires("tomorrow".into());
    edit.invoke_apply_clicked();
    assert!(!edit.get_error().is_empty());
    assert!(edit.window().is_visible());
    edit.set_expires(expires.to_string().into());
    edit.invoke_apply_clicked();
    cookies.invoke_apply_clicked();
    let stored = store
        .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert_eq!(stored[0].expires, Some(expires));
    // The session checkbox clears the expiry again without dropping the other attributes.
    browser.invoke_refresh_clicked();
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    cookies.invoke_row_clicked(0, false, false);
    cookies.invoke_edit_clicked();
    let edit = sessions::last_edit_opened().unwrap();
    edit.set_session_cookie(true);
    edit.invoke_apply_clicked();
    cookies.invoke_apply_clicked();
    let stored = store
        .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert_eq!(stored[0].expires, None);
    assert_eq!(stored[0].name, "sid");
}
