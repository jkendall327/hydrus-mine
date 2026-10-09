//! The manage subscriptions dialog (network > subscriptions…) on the
//! store's subscriptions: its list (the rows themselves are tested against
//! the reference's in hydrus-gui-model's tests), its buttons and the
//! questions they ask, "apply" writing what changed, and "cancel"
//! writing nothing.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_gui::{Bound, MainWindow, Pages, SubscriptionsWindow, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};
use hydrus_store::subscriptions;

// leaf: subscriptions-copy-options
#[test]
fn favourites_load_selected_subscriptions_and_custom_overwrite_obeys_owner_lifetime() {
    use hydrus_core::import_options::ImportOptionsManager;
    use hydrus_downloader_exchange::import_options;
    use hydrus_store::settings;
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    let reference = &fixture["subscription_favourites"];
    let existing = import_options::decode_text(&fixture["existing"].to_string()).unwrap();
    let incoming = import_options::decode_text(&fixture["incoming"].to_string()).unwrap();
    let original = existing.clone();
    let favourite = incoming.clone();
    store
        .write(move |tx| {
            for name in ["alpha", "beta"] {
                subscriptions::create_subscription(
                    tx.conn(),
                    name,
                    &SubscriptionSettings {
                        import_options: original.clone(),
                        ..SubscriptionSettings::default()
                    },
                )?;
            }
            let mut manager: ImportOptionsManager = settings::get(tx.conn())?;
            manager.favourites.push(("profile".into(), favourite));
            settings::set(tx.conn(), &manager)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_favourite(0, "profile".into());
    assert_eq!(asked(&dialog).1, reference["information"][0]);
    dialog.invoke_chosen(0);
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_row_clicked(1, true, false);
    dialog.invoke_favourite(0, "profile".into());
    for (cells, _) in rows(&dialog) {
        assert_eq!(cells[8], reference["steps"][1]["rows"][0]["summary"]);
    }
    assert!(
        store
            .read(subscriptions::subscriptions)
            .unwrap()
            .iter()
            .all(|s| s.settings.import_options == existing)
    );
    dialog.invoke_cancel();
    dialog.invoke_favourite(0, "profile".into());
    dialog.invoke_apply();
    assert!(
        store
            .read(subscriptions::subscriptions)
            .unwrap()
            .iter()
            .all(|s| s.settings.import_options == existing)
    );

    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_row_clicked(1, true, false);
    dialog.invoke_favourite(1, "profile".into());
    assert_eq!(asked(&dialog).1, reference["information"][1]);
    dialog.invoke_chosen(0);
    let chooser = hydrus_gui::import_options_overwrite_window::last_opened().unwrap();
    assert!(dialog.get_import_child_open());
    chooser.invoke_preset(1);
    chooser.invoke_cancel();
    assert!(!dialog.get_import_child_open());
    chooser.invoke_apply();
    for (cells, _) in rows(&dialog) {
        assert_eq!(cells[8], reference["steps"][2]["rows"][0]["summary"]);
    }
    dialog.invoke_favourite(1, "profile".into());
    dialog.invoke_chosen(0);
    let chooser = hydrus_gui::import_options_overwrite_window::last_opened().unwrap();
    chooser.invoke_preset(1);
    let pixels = headless::render(&windows.get(1).unwrap(), 1180, 560);
    assert!(!pixels.is_empty());
    chooser.invoke_apply();
    for (cells, _) in rows(&dialog) {
        assert_eq!(cells[8], reference["steps"][3]["rows"][0]["summary"]);
    }
    dialog.invoke_apply();
    let expected =
        import_options::decode_text(&reference["dialogs"][1]["options"].to_string()).unwrap();
    assert!(
        store
            .read(subscriptions::subscriptions)
            .unwrap()
            .iter()
            .all(|s| s.settings.import_options == expected)
    );
    let reopened = open_dialog(&ui, &bound);
    assert_eq!(
        rows(&reopened)[0].0[8],
        reference["steps"][3]["rows"][0]["summary"]
    );
    reopened.invoke_row_clicked(0, false, false);
    reopened.invoke_favourite(1, "profile".into());
    let stale = hydrus_gui::import_options_overwrite_window::last_opened().unwrap();
    reopened.invoke_cancel();
    assert!(hydrus_gui::import_options_overwrite_window::last_opened().is_none());
    stale.invoke_preset(2);
    stale.invoke_apply();
    reopened.invoke_apply();
    assert!(
        store
            .read(subscriptions::subscriptions)
            .unwrap()
            .iter()
            .all(|s| s.settings.import_options == expected)
    );

    let direct = open_dialog(&ui, &bound);
    direct.invoke_row_clicked(0, false, false);
    direct.invoke_row_clicked(1, true, false);
    direct.invoke_favourite(0, "profile".into());
    direct.invoke_apply();
    assert!(
        store
            .read(subscriptions::subscriptions)
            .unwrap()
            .iter()
            .all(|s| s.settings.import_options == incoming)
    );
}

// leaf: subscriptions-copy-options
#[test]
fn subscription_option_clipboard_edits_stay_staged_and_closed_owners_cannot_apply() {
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    let existing =
        hydrus_downloader_exchange::import_options::decode_text(&fixture["existing"].to_string())
            .unwrap();
    let original = existing.clone();
    store
        .write(move |ctx| {
            for name in ["alpha", "beta"] {
                subscriptions::create_subscription(
                    ctx.conn(),
                    name,
                    &SubscriptionSettings {
                        import_options: original.clone(),
                        ..SubscriptionSettings::default()
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let copied = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().clone_from(text);
            }
        }
    });
    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_copy_import_options();
    assert_eq!(
        hydrus_downloader_exchange::import_options::decode_text(&copied.borrow()).unwrap(),
        existing
    );
    let incoming = fixture["incoming"].to_string();
    hydrus_gui::set_paster(move || incoming.clone());
    dialog.invoke_paste_import_options(0);
    assert_eq!(
        store.read(subscriptions::subscriptions).unwrap()[0]
            .settings
            .import_options,
        existing
    );
    let pixels = headless::render(&windows.get(1).unwrap(), 1180, 560);
    assert!(!pixels.is_empty());
    dialog.invoke_cancel();
    dialog.invoke_apply();
    assert_eq!(
        store.read(subscriptions::subscriptions).unwrap()[0]
            .settings
            .import_options,
        existing
    );

    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_row_clicked(1, true, false);
    hydrus_gui::set_paster(|| "not json".into());
    dialog.invoke_paste_import_options(0);
    assert!(
        asked(&dialog)
            .1
            .contains("JSON-serialised Import Options Container")
    );
    dialog.invoke_chosen(0);
    dialog.invoke_clear_import_options();
    assert_eq!(asked(&dialog).1, fixture["questions"][0]);
    dialog.invoke_chosen(1);
    dialog.invoke_clear_import_options();
    dialog.invoke_chosen(0);
    dialog.invoke_apply();
    assert!(
        store
            .read(subscriptions::subscriptions)
            .unwrap()
            .iter()
            .all(|s| s.settings.import_options.is_empty())
    );
    let reopened = open_dialog(&ui, &bound);
    assert_eq!(rows(&reopened).len(), 2);
    reopened.invoke_cancel();
}

pub(crate) fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

pub(crate) fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

/// network > subscriptions…, from the menu bar.
pub(crate) fn open_dialog(ui: &MainWindow, bound: &Bound) -> SubscriptionsWindow {
    let titles = ui.get_menu_titles();
    let network = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "network")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(network).unwrap(), 200.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let entry = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "subscriptions\u{2026}")
        .expect("network > subscriptions…");
    ui.invoke_menu_line_clicked(0, i32::try_from(entry).unwrap(), 0.0, 0.0, 0.0);
    bound
        .subscriptions
        .borrow()
        .as_ref()
        .expect("the dialog opens")
        .clone_strong()
}

/// The list's rows, as shown: each one's cells and whether it is
/// selected.
pub(crate) fn rows(dialog: &SubscriptionsWindow) -> Vec<(Vec<String>, bool)> {
    let rows = dialog.get_rows();
    (0..rows.row_count())
        .map(|r| {
            let row = rows.row_data(r).unwrap();
            let cells = (0..row.cells.row_count())
                .map(|c| row.cells.row_data(c).unwrap().to_string())
                .collect();
            (cells, row.selected)
        })
        .collect()
}

fn row<'a>(rows: &'a [(Vec<String>, bool)], name: &str) -> &'a [String] {
    &rows.iter().find(|r| r.0[0] == name).unwrap().0
}

/// The question asked: its title, message and choices.
pub(crate) fn asked(dialog: &SubscriptionsWindow) -> (String, String, Vec<String>) {
    assert!(dialog.get_asking(), "a question is asked");
    let choices = dialog.get_asking_choices();
    (
        dialog.get_asking_title().into(),
        dialog.get_asking_message().into(),
        (0..choices.row_count())
            .map(|i| choices.row_data(i).unwrap().to_string())
            .collect(),
    )
}

// leaf: audit-network-subs-duplicate-delete
#[test]
fn the_dialog_lists_the_subscriptions_and_changes_them_on_apply() {
    let (_dirs, store) = store();
    // a subscription, and one paused and delayed, with a live query and a
    // dead (and so paused) one that found a file
    let now = now();
    let (id, live, dead) = store
        .write(move |ctx| {
            let conn = ctx.conn();
            let settings = SubscriptionSettings {
                gug_name: "example tag search".into(),
                paused: true,
                no_work_until: now + 3600,
                no_work_until_reason: "network error".into(),
                ..SubscriptionSettings::default()
            };
            let other = SubscriptionSettings {
                gug_name: "example tag search".into(),
                ..SubscriptionSettings::default()
            };
            let alpha = subscriptions::create_subscription(conn, "Alpha", &other)?.unwrap();
            subscriptions::add_query(conn, alpha, &QueryState::new("red hair"), now)?;
            let id = subscriptions::create_subscription(conn, "zz artist", &settings)?.unwrap();
            let live = subscriptions::add_query(conn, id, &QueryState::new("blue eyes"), now)?;
            let mut state = QueryState::new("old artist");
            state.dead = true;
            state.paused = true;
            state.last_check_time = now - 86400;
            let dead = subscriptions::add_query(conn, id, &state, now)?;
            let seed = NewFileSeed {
                seed_type: SeedType::Url,
                data: "https://booru.example/post/1".into(),
                data_for_comparison: "https://booru.example/post/1".into(),
                source_time: None,
                referral_url: None,
                meta: FileSeedMeta::default(),
            };
            queues::add_file_seeds(conn, dead, &[seed], false, now - 86400)?;
            let mut seeds = queues::file_seeds(conn, dead)?;
            seeds[0].status = SeedStatus::SuccessfulAndNew;
            queues::update_file_seed(conn, &seeds[0])?;
            Ok((id, live, dead))
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // the list, by name, as the reference writes it
    let dialog = open_dialog(&ui, &bound);
    assert_eq!(dialog.get_window_title(), "manage subscriptions");
    assert!(!dialog.get_globally_paused());
    let shown = rows(&dialog);
    let names: Vec<&str> = shown.iter().map(|r| r.0[0].as_str()).collect();
    // (casefolded)
    assert_eq!(names, ["Alpha", "zz artist"]);
    assert_eq!(row(&shown, "Alpha")[2], "1 working");
    let zz = row(&shown, "zz artist");
    assert_eq!(zz[1], "example tag search");
    assert_eq!(zz[2], "1 working, 1 dead");
    assert!(zz[5].starts_with("delayed--retrying in "), "{}", zz[5]);
    assert!(zz[5].ends_with(" - because: network error"), "{}", zz[5]);
    assert_eq!((zz[6].as_str(), zz[7].as_str()), ("1", "yes"));
    assert!(!dialog.get_any_selected() && !dialog.get_can_check_now());

    // selected: its delay scrubbed
    dialog.invoke_row_clicked(1, false, false);
    assert!(rows(&dialog)[1].1);
    assert!(dialog.get_any_selected() && dialog.get_can_scrub_delays());
    dialog.invoke_scrub_delays();
    assert_eq!(row(&rows(&dialog), "zz artist")[5], "");
    assert!(!dialog.get_can_scrub_delays());

    // checking its queries now asks of the paused subscription, the dead
    // query and the paused one, as the reference does
    assert!(dialog.get_can_check_now());
    dialog.invoke_check_now();
    assert_eq!(
        asked(&dialog),
        (
            "Check which?".into(),
            "Of the 1 selected subscriptions, 1 are paused. Do you want to unpause these paused subs and check their queries?".into(),
            vec!["yes, unpause them and check their queries".into(), "no, leave them alone".into()],
        )
    );
    dialog.invoke_chosen(0);
    let (_, message, choices) = asked(&dialog);
    assert_eq!(
        message,
        "Of the 2 selected queries, 1 are DEAD. Do you want to check these?"
    );
    assert_eq!(choices.len(), 3);
    dialog.invoke_chosen(0);
    let (_, message, _) = asked(&dialog);
    assert_eq!(
        message,
        "Of the 2 selected queries, 1 are paused. Do you want to unpause and check them?"
    );
    dialog.invoke_chosen(0);
    assert!(!dialog.get_asking());
    let zz = row(&rows(&dialog), "zz artist").to_vec();
    assert_eq!((zz[2].as_str(), zz[7].as_str()), ("2 working", ""));
    assert!(!dialog.get_can_check_now(), "both are checking now");

    // "cancel": nothing written
    dialog.invoke_cancel();
    assert!(bound.subscriptions.borrow().is_none());
    let unchanged = store
        .read(move |c| subscriptions::subscription(c, id))
        .unwrap()
        .unwrap();
    assert!(unchanged.settings.paused && unchanged.settings.no_work_until == now + 3600);
    let dead_state = store
        .read(move |c| subscriptions::query(c, dead))
        .unwrap()
        .unwrap();
    assert!(dead_state.state.dead && !dead_state.state.check_now);

    // again, and "apply": what changed is written
    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(1, false, false);
    dialog.invoke_check_now();
    for _ in 0..3 {
        dialog.invoke_chosen(0);
    }
    dialog.invoke_apply();
    assert!(bound.subscriptions.borrow().is_none());
    let applied = store
        .read(move |c| subscriptions::subscription(c, id))
        .unwrap()
        .unwrap();
    assert!(!applied.settings.paused && applied.settings.no_work_until == 0);
    for queue in [live, dead] {
        let q = store
            .read(move |c| subscriptions::query(c, queue))
            .unwrap()
            .unwrap();
        assert!(
            q.state.check_now && !q.state.paused && !q.state.dead,
            "{queue}"
        );
    }

    // pausing, then selecting by query text (case and all) and deleting,
    // asking first
    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(1, false, false);
    dialog.invoke_pause_resume();
    assert_eq!(row(&rows(&dialog), "zz artist")[7], "yes");
    dialog.invoke_select_subscriptions();
    let (title, message, choices) = asked(&dialog);
    assert_eq!(title, "Enter text");
    assert!(message.starts_with("This selects subscriptions based on query text."));
    assert_eq!(choices, ["ok"]);
    dialog.set_asked_text("Blue".into());
    dialog.invoke_chosen(0);
    assert!(!dialog.get_any_selected(), "case and all");
    dialog.invoke_select_subscriptions();
    dialog.set_asked_text("blue".into());
    dialog.invoke_chosen(0);
    let shown = rows(&dialog);
    assert!(shown[1].1 && !shown[0].1);
    dialog.invoke_delete();
    let (title, message, choices) = asked(&dialog);
    assert_eq!(
        (title.as_str(), message.as_str()),
        ("Are you sure?", "Remove all selected?")
    );
    assert_eq!(choices, ["yes", "no"]);
    // (cancelled, then answered)
    dialog.invoke_cancelled();
    assert_eq!(rows(&dialog).len(), 2);
    dialog.invoke_delete();
    dialog.invoke_chosen(0);
    assert_eq!(rows(&dialog).len(), 1);
    dialog.invoke_apply();
    assert!(
        store
            .read(move |c| subscriptions::subscription(c, id))
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .read(move |c| subscriptions::query(c, live))
            .unwrap()
            .is_none()
    );
    assert_eq!(store.read(subscriptions::subscriptions).unwrap().len(), 1);

    // paused from the network menu: the dialog says so
    store
        .write(|ctx| {
            let mut pauses: hydrus_store::settings::Pauses =
                hydrus_store::settings::get(ctx.conn())?;
            pauses.subscriptions = true;
            hydrus_store::settings::set(ctx.conn(), &pauses)
        })
        .unwrap();
    let dialog = open_dialog(&ui, &bound);
    assert!(dialog.get_globally_paused());
}

/// The answer at `label` to the question asked.
fn answer(dialog: &SubscriptionsWindow, label: &str) {
    let (_, _, choices) = asked(dialog);
    let i = choices.iter().position(|c| c == label).unwrap();
    dialog.invoke_chosen(i32::try_from(i).unwrap());
}

fn select(dialog: &SubscriptionsWindow, names: &[&str]) {
    let mut first = true;
    for (r, (cells, _)) in rows(dialog).iter().enumerate() {
        if names.contains(&cells[0].as_str()) {
            dialog.invoke_row_clicked(i32::try_from(r).unwrap(), !first, false);
            first = false;
        }
    }
}

/// All persistent identities, states and histories, including seed metadata.
fn merge_store_state(store: &Store) -> serde_json::Value {
    store
        .read(|conn| {
            let mut result = Vec::new();
            for subscription in subscriptions::subscriptions(conn)? {
                let mut queries = Vec::new();
                for query in subscriptions::queries(conn, subscription.id)? {
                    queries.push(serde_json::json!({
                        "queue": query.queue_id,
                        "state": query.state,
                        "files": queues::file_seeds(conn, query.queue_id)?,
                        "gallery": queues::gallery_seeds(conn, query.queue_id)?,
                    }));
                }
                result.push(serde_json::json!({
                    "id": subscription.id, "name": subscription.name,
                    "settings": subscription.settings, "queries": queries,
                }));
            }
            Ok(serde_json::Value::Array(result))
        })
        .unwrap()
}

/// Load the recorded input with independent native queues for overlapping texts.
fn seed_merge_case(store: &Store, case: &serde_json::Value, now: i64) {
    let before = case["before"].as_array().unwrap().clone();
    store
        .write(move |tx| {
            let conn = tx.conn();
            for sub in &before {
                let settings = SubscriptionSettings {
                    gug_name: sub["source"].as_str().unwrap().into(),
                    gug_key: sub["source_key"].as_str().unwrap().into(),
                    paused: sub["paused"].as_bool().unwrap(),
                    initial_file_limit: sub["limits"][0].as_u64(),
                    periodic_file_limit: sub["limits"][1].as_u64(),
                    ..Default::default()
                };
                let id = subscriptions::create_subscription(
                    conn,
                    sub["name"].as_str().unwrap(),
                    &settings,
                )?
                .unwrap();
                for query in sub["queries"].as_array().unwrap() {
                    let header = &query["state"][2];
                    let mut state = QueryState::new(query["text"].as_str().unwrap());
                    state.display_name = header[2].as_str().map(Into::into);
                    state.check_now = header[3].as_bool().unwrap();
                    state.last_check_time = header[4].as_i64().unwrap();
                    state.next_check_time = header[5].as_i64().unwrap();
                    state.paused = header[6].as_bool().unwrap();
                    let queue = subscriptions::add_query(conn, id, &state, now)?;
                    let file = &query["files"][0];
                    let url = file[0].as_str().unwrap();
                    queues::add_file_seeds(
                        conn,
                        queue,
                        &[NewFileSeed {
                            seed_type: SeedType::Url,
                            data: url.into(),
                            data_for_comparison: url.into(),
                            source_time: Some(now - 100),
                            referral_url: Some("https://subscription.example/referral".into()),
                            meta: FileSeedMeta::default(),
                        }],
                        false,
                        now,
                    )?;
                    let mut seed = queues::file_seeds(conn, queue)?.remove(0);
                    seed.status = if file[1] == 4 {
                        SeedStatus::Error
                    } else {
                        SeedStatus::SuccessfulAndNew
                    };
                    seed.note = file[2].as_str().unwrap().into();
                    queues::update_file_seed(conn, &seed)?;
                    let gallery = &query["gallery"][0];
                    queues::add_gallery_seeds(
                        conn,
                        queue,
                        &[queues::NewGallerySeed {
                            url: gallery[0].as_str().unwrap().into(),
                            can_generate_more_pages: true,
                            referral_url: Some("https://subscription.example/referral".into()),
                            meta: queues::GallerySeedMeta::default(),
                        }],
                        None,
                        now,
                    )?;
                    let mut seed = queues::gallery_seeds(conn, queue)?.remove(0);
                    seed.status = if gallery[1] == 4 {
                        SeedStatus::Error
                    } else {
                        SeedStatus::SuccessfulAndNew
                    };
                    seed.note = gallery[2].as_str().unwrap().into();
                    queues::update_gallery_seed(conn, &seed)?;
                }
            }
            Ok(())
        })
        .unwrap();
}

// leaf: subscriptions-merge
#[test]
fn subscription_merge_stages_all_groups_and_preserves_query_histories() {
    let recording = hydrus_testkit::fixture_json("subscription_merge.json");
    let reference_now = recording["now"].as_i64().unwrap();
    let windows = headless::init();
    for case in recording["cases"].as_array().unwrap() {
        let (_dirs, store) = store();
        seed_merge_case(&store, case, reference_now);
        let saved = merge_store_state(&store);
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let dialog = open_dialog(&ui, &bound);
        let names: Vec<_> = case["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        select(&dialog, &names);
        let unchanged_rows = rows(&dialog);
        dialog.invoke_merge();
        for question in case["questions"].as_array().unwrap() {
            let at = format!("{}: {}", case["case"], question["kind"]);
            assert_eq!(
                rows(&dialog),
                unchanged_rows,
                "draft changed before final decision: {at}"
            );
            // Apply is inert while a question owns the workflow.
            dialog.invoke_apply();
            assert_eq!(merge_store_state(&store), saved, "{at}");
            match question["kind"].as_str().unwrap() {
                "confirm" => {
                    assert_eq!(asked(&dialog).1, question["message"].as_str().unwrap());
                    answer(
                        &dialog,
                        if question["answer"] == true {
                            "yes"
                        } else {
                            "no"
                        },
                    );
                }
                "primary" => {
                    let (title, _, choices) = asked(&dialog);
                    assert_eq!(title, question["title"].as_str().unwrap());
                    assert_eq!(serde_json::json!(choices), question["choices"]);
                    if case["case"] == "later primary cancel" && question["answer"].is_null() {
                        let window = windows.get(windows.count() - 1).unwrap();
                        headless::render(&window, 1180, 520);
                        let pixels = headless::render(&window, 1180, 520);
                        headless::save_png(
                            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                                .join("subscription_merge_later_primary.png"),
                            &pixels,
                            1180,
                            520,
                        )
                        .unwrap();
                    }
                    if let Some(name) = question["answer"].as_str() {
                        answer(&dialog, name);
                    } else {
                        dialog.invoke_cancelled();
                    }
                }
                "name" => {
                    assert_eq!(asked(&dialog).1, question["message"].as_str().unwrap());
                    assert_eq!(
                        dialog.get_asked_text(),
                        question["default"].as_str().unwrap()
                    );
                    if let Some(name) = question["answer"].as_str() {
                        dialog.set_asked_text(name.into());
                        dialog.invoke_chosen(0);
                    } else {
                        dialog.invoke_cancelled();
                    }
                }
                "information" => {
                    assert_eq!(asked(&dialog).1, question["message"].as_str().unwrap());
                    dialog.invoke_chosen(0);
                }
                other => panic!("unexpected reference question {other}"),
            }
        }
        assert!(!dialog.get_asking());
        let mut names: Vec<_> = rows(&dialog)
            .into_iter()
            .map(|row| row.0[0].clone())
            .collect();
        names.sort();
        let expected_names: Vec<_> = case["after"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, expected_names, "{}", case["case"]);
        if case["case"] == "both names cancel" {
            let window = windows.get(windows.count() - 1).unwrap();
            headless::render(&window, 1180, 520);
            let pixels = headless::render(&window, 1180, 520);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join("subscription_merge_completed.png"),
                &pixels,
                1180,
                520,
            )
            .unwrap();
        }
        assert_eq!(merge_store_state(&store), saved);
        dialog.invoke_apply();
        let after = merge_store_state(&store);
        assert_eq!(after.as_array().unwrap().len(), expected_names.len());
        for expected in case["after"].as_array().unwrap() {
            let original = saved
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["name"] == expected["source_key"])
                .unwrap();
            let actual = after
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["name"] == expected["name"])
                .unwrap();
            assert_eq!(actual["id"], original["id"]);
            assert_eq!(actual["settings"], original["settings"]);
            let expected_queries: Vec<_> = expected["queries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|q| {
                    let origin = case["before"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|s| {
                            s["queries"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|other| other["log"] == q["log"])
                        })
                        .unwrap();
                    let original = saved
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|s| s["name"] == origin["name"])
                        .unwrap();
                    original["queries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|other| other["state"]["query_text"] == q["text"])
                        .unwrap()
                        .clone()
                })
                .collect();
            assert_eq!(
                actual["queries"],
                serde_json::json!(expected_queries),
                "{}",
                case["case"]
            );
        }
        // Closed callbacks cannot commit a second time or alter a successor.
        let successor = open_dialog(&ui, &bound);
        dialog.invoke_cancelled();
        dialog.invoke_chosen(0);
        dialog.invoke_apply();
        assert!(bound.subscriptions.borrow().is_some());
        assert_eq!(merge_store_state(&store), after);
        successor.invoke_cancel();
    }
}

// leaf: subscriptions-merge
#[test]
fn closing_subscription_merge_owner_discards_pending_and_completed_drafts() {
    let recording = hydrus_testkit::fixture_json("subscription_merge.json");
    let case = recording["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case"] == "both names cancel")
        .unwrap();
    let (_dirs, store) = store();
    seed_merge_case(&store, case, recording["now"].as_i64().unwrap());
    let saved = merge_store_state(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for complete in [false, true] {
        let dialog = open_dialog(&ui, &bound);
        select(&dialog, &["a1", "a2", "b1", "b2"]);
        dialog.invoke_merge();
        answer(&dialog, "yes");
        answer(&dialog, "b1");
        dialog.invoke_cancelled();
        if complete {
            answer(&dialog, "a2");
            dialog.invoke_cancelled();
            assert_eq!(rows(&dialog).len(), 3);
        } else {
            assert_eq!(rows(&dialog).len(), 5);
        }
        dialog.invoke_cancel();
        assert_eq!(merge_store_state(&store), saved);
        let successor = open_dialog(&ui, &bound);
        select(&successor, &["outside"]);
        successor.invoke_pause_resume();
        dialog.invoke_chosen(0);
        dialog.invoke_cancelled();
        dialog.invoke_apply();
        assert_eq!(
            rows(&successor)
                .iter()
                .find(|r| r.0[0] == "outside")
                .unwrap()
                .0[7],
            "yes"
        );
        successor.invoke_cancel();
        assert_eq!(merge_store_state(&store), saved);
    }
}

// leaf: audit-network-subs-lowercase
// leaf: subscriptions-separate
// leaf: subscriptions-reset
#[test]
fn merging_separating_and_resetting_are_written_on_apply() {
    let (_dirs, store) = store();
    let now = now();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let settings = SubscriptionSettings {
                gug_name: "example tag search".into(),
                ..SubscriptionSettings::default()
            };
            let a = subscriptions::create_subscription(conn, "a", &settings)?.unwrap();
            let mut state = QueryState::new("Red_Hair");
            state.last_check_time = now - 3600;
            let red = subscriptions::add_query(conn, a, &state, now)?;
            let seed = NewFileSeed {
                seed_type: SeedType::Url,
                data: "https://booru.example/post/1".into(),
                data_for_comparison: "https://booru.example/post/1".into(),
                source_time: None,
                referral_url: None,
                meta: FileSeedMeta::default(),
            };
            queues::add_file_seeds(conn, red, &[seed], false, now)?;
            let b = subscriptions::create_subscription(conn, "b", &settings)?.unwrap();
            subscriptions::add_query(conn, b, &QueryState::new("blue"), now)?;
            subscriptions::add_query(conn, b, &QueryState::new("green"), now)?;
            Ok(())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);

    // merge "a" and "b" into "b", renamed "both"
    select(&dialog, &["a", "b"]);
    assert!(dialog.get_can_merge() && !dialog.get_can_separate());
    assert!(dialog.get_can_lowercase() && dialog.get_can_reset());
    dialog.invoke_merge();
    assert!(
        asked(&dialog)
            .1
            .starts_with("Are you sure you want to merge")
    );
    answer(&dialog, "yes");
    let (title, _, choices) = asked(&dialog);
    assert_eq!(
        title,
        "select the primary subscription--into which to merge the others"
    );
    assert_eq!(choices, ["a", "b"]);
    answer(&dialog, "b");
    let (_, message, _) = asked(&dialog);
    assert_eq!(
        message,
        "b was able to merge 1 other subscriptions. If you wish to change its name, do so here."
    );
    assert_eq!(dialog.get_asked_text(), "b");
    dialog.set_asked_text("both".into());
    dialog.invoke_chosen(0);
    assert!(!dialog.get_asking());
    let shown = rows(&dialog);
    assert_eq!(shown.len(), 1);
    assert_eq!(
        (shown[0].0[0].as_str(), shown[0].0[2].as_str()),
        ("both", "3 working")
    );

    // lowercase and reset it
    select(&dialog, &["both"]);
    dialog.invoke_lowercase();
    answer(&dialog, "yes");
    assert!(!dialog.get_can_lowercase());
    dialog.invoke_reset();
    assert!(
        asked(&dialog)
            .1
            .starts_with("Resetting these subscriptions")
    );
    answer(&dialog, "yes");
    assert!(!dialog.get_can_reset());

    // separate it whole, as "x"
    assert!(dialog.get_can_separate());
    dialog.invoke_separate();
    answer(&dialog, "break it all into single-query subscriptions");
    assert_eq!(dialog.get_asked_text(), "both");
    dialog.set_asked_text("x".into());
    dialog.invoke_chosen(0);
    let names: Vec<String> = rows(&dialog).into_iter().map(|r| r.0[0].clone()).collect();
    assert_eq!(names, ["x: blue", "x: green", "x: red_hair"]);

    // applied: the queries moved with their histories, emptied by reset
    dialog.invoke_apply();
    let written = store.read(subscriptions::subscriptions).unwrap();
    let names: Vec<&str> = written.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["x: blue", "x: green", "x: red_hair"]);
    let red = store
        .read(move |c| {
            let id = subscriptions::find_subscription(c, "x: red_hair")?
                .unwrap()
                .id;
            subscriptions::queries(c, id)
        })
        .unwrap();
    assert_eq!(red.len(), 1);
    assert_eq!(red[0].state.query_text, "red_hair");
    assert_eq!(red[0].state.last_check_time, 0);
    let queue = red[0].queue_id;
    assert!(
        store
            .read(move |c| queues::file_seeds(c, queue))
            .unwrap()
            .is_empty()
    );
}

// leaf: subscription-add
// leaf: audit-network-subs-overwrite-downloader
#[test]
fn add_uses_a_separate_gallery_list_then_the_editor() {
    use hydrus_core::url::{AnyGug, Gug, Gugs};
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let list = open_dialog(&ui, &bound);
    let cases: serde_json::Value = hydrus_testkit::fixture_json("subscription_add.json");
    let cases = cases.as_array().unwrap();
    list.invoke_add();
    let warning = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        warning.get_message().as_str(),
        cases[0]["dialogs"][0]["message"].as_str().unwrap()
    );
    assert!(!list.get_asking());
    assert_eq!(
        warning.get_galleries().row_count(),
        0,
        "empty settings stay empty"
    );
    warning.invoke_accept_clicked();
    assert!(bound.edit_subscription.borrow().is_none());

    let gallery = |name: &str, key: &str| {
        AnyGug::Single(Gug {
            name: name.into(),
            key: key.into(),
            url_template: "https://booru.example/search/%tags%/1".into(),
            replacement_phrase: "%tags%".into(),
            separator: "+".into(),
            initial_search_text: "tag".into(),
            example_search_text: "blue_eyes".into(),
        })
    };
    for (names, key, fixture_index) in [
        (vec![gallery("alpha", "01")], None, 1),
        (
            vec![gallery("zed", "02"), gallery("alpha", "01")],
            Some(("02".to_owned(), "zed".to_owned())),
            5,
        ),
    ] {
        store
            .write(move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_parse::Downloaders {
                        gugs: Gugs {
                            keys_to_display: names.iter().map(|g| g.key().to_owned()).collect(),
                            gugs: names,
                        },
                        ..hydrus_parse::Downloaders::default()
                    },
                )?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &hydrus_core::subscriptions::GalleryDefaults {
                        gug: key,
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        let before = headless::render(&windows.get(1).unwrap(), 1180, 520);
        let window_count = windows.count();
        list.invoke_add();
        assert_eq!(
            windows.count(),
            window_count + 1,
            "a separate chooser window"
        );
        assert!(list.get_gallery_open());
        assert!(
            !list.get_asking(),
            "Add never inserts the inline question panel"
        );
        assert!(bound.edit_subscription.borrow().is_none());
        let chooser = bound
            .subscription_gallery
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let expected = &cases[fixture_index]["dialogs"][0];
        assert_eq!(
            chooser.get_window_title().as_str(),
            expected["title"].as_str().unwrap()
        );
        let choices = chooser.get_galleries();
        let choices: Vec<String> = (0..choices.row_count())
            .map(|i| choices.row_data(i).unwrap().to_string())
            .collect();
        assert_eq!(serde_json::to_value(&choices).unwrap(), expected["choices"]);
        assert_eq!(
            choices[usize::try_from(chooser.get_selected()).unwrap()],
            expected["selected"][0].as_str().unwrap()
        );
        let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
        let pixels = headless::render(&windows.get(1).unwrap(), 1180, 520);
        assert_eq!(
            &pixels[..1180 * 250 * 4],
            &before[..1180 * 250 * 4],
            "the subscriptions table keeps its rendered geometry"
        );
        headless::save_png(&shots.join("subscription_add_list.png"), &pixels, 1180, 520).unwrap();
        let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 420, 320);
        headless::save_png(
            &shots.join("subscription_add_chooser.png"),
            &pixels,
            420,
            320,
        )
        .unwrap();
        // Cancellation must leave the list and store empty, including with one gallery.
        chooser.invoke_cancel();
        chooser.invoke_accept_clicked();
        assert!(
            bound.edit_subscription.borrow().is_none(),
            "cancel invalidates the old chooser callback"
        );
        assert!(!list.get_gallery_open());
        assert!(bound.subscription_gallery.borrow().is_none());
        assert!(rows(&list).is_empty());
        assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
        list.invoke_add();
        let chooser = bound
            .subscription_gallery
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        chooser.invoke_accept_clicked();
        assert!(!list.get_gallery_open());
        let editor = bound
            .edit_subscription
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        assert_eq!(editor.get_name(), "new subscription");
        assert_eq!(
            editor.get_downloader(),
            choices[usize::try_from(chooser.get_selected()).unwrap()]
        );
        editor.invoke_cancel();
        assert!(rows(&list).is_empty());
    }
    // Overwrite uses the same chooser and preserves the old downloader on cancel.
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
    list.invoke_row_clicked(0, false, false);
    list.invoke_overwrite_downloader();
    let chooser = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    chooser.invoke_cancel();
    assert_eq!(rows(&list)[0].0[1], "zed");
    list.invoke_overwrite_downloader();
    let chooser = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    chooser.set_selected(0);
    chooser.invoke_accept_clicked();
    assert_eq!(rows(&list)[0].0[1], "alpha");
    list.invoke_apply();
    let written = store.read(subscriptions::subscriptions).unwrap();
    assert_eq!(written[0].settings.gug_name, "alpha");
    // Closing the parent also invalidates a retained chooser handle.
    let list = open_dialog(&ui, &bound);
    list.invoke_add();
    let chooser = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_cancel();
    assert!(bound.subscriptions.borrow().is_none());
    assert!(bound.subscription_gallery.borrow().is_none());
    chooser.invoke_accept_clicked();
    assert!(bound.edit_subscription.borrow().is_none());
    let unchanged = store.read(subscriptions::subscriptions).unwrap();
    assert_eq!(unchanged.len(), 1);
    assert_eq!(unchanged[0].settings.gug_name, "alpha");
    // Clearing configured galleries cannot resurrect either old choices or presets.
    store
        .write(|ctx| hydrus_store::settings::set(ctx.conn(), &hydrus_parse::Downloaders::default()))
        .unwrap();
    let list = open_dialog(&ui, &bound);
    list.invoke_add();
    let warning = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(warning.get_window_title(), "Warning");
    assert_eq!(warning.get_galleries().row_count(), 0);
    warning.invoke_accept_clicked();
    assert!(bound.edit_subscription.borrow().is_none());
}

#[test]
fn full_subscription_exchange_is_staged_cancellable_and_reopens_with_complete_histories() {
    use hydrus_downloader_exchange::subscriptions as exchange;
    let reference = hydrus_testkit::fixture_json("subscription_exchange.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    let text = reference["single"].to_string();
    hydrus_gui::set_paster(move || text.clone());
    dialog.invoke_exchange_mode(3);
    assert!(!bound.subscription_exchange.has_open());
    assert_eq!(asked(&dialog).1, "1 objects added!");
    dialog.invoke_apply();
    assert!(
        bound.subscriptions.borrow().is_some(),
        "Apply is blocked while a question is open"
    );
    dialog.invoke_chosen(0);
    assert_eq!(rows(&dialog)[0].0[0], "Artist");
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    dialog.invoke_cancel();
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    let dialog = open_dialog(&ui, &bound);
    let path = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    std::fs::write(path.path(), reference["single"].to_string()).unwrap();
    let chosen = path.path().to_path_buf();
    hydrus_gui::set_picker(move |_, _| vec![chosen.clone()]);
    dialog.invoke_exchange_mode(4);
    assert_eq!(asked(&dialog).1, "1 objects added!");
    dialog.invoke_chosen(0);
    dialog.invoke_apply();
    let saved = store.read(subscriptions::subscriptions).unwrap();
    assert_eq!(saved.len(), 1);
    let id = saved[0].id;
    let queries = store
        .read(move |conn| subscriptions::queries(conn, id))
        .unwrap();
    let queue = queries[0].queue_id;
    let seeds = store
        .read(move |conn| queues::file_seeds(conn, queue))
        .unwrap();
    assert_eq!(seeds[0].note, "ignored\nrecorded reason");
    assert_eq!(
        seeds[0].meta.notes,
        [("note".into(), "first\n\nsecond".into())]
    );
    let galleries = store
        .read(move |conn| queues::gallery_seeds(conn, queue))
        .unwrap();
    assert_eq!(galleries[0].note, "gallery failure");
    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_exchange();
    let child = bound
        .subscription_exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let exported = exchange::decode_text(child.get_text().as_str()).unwrap();
    assert_eq!(exported[0].settings, saved[0].settings);
    assert_eq!(exported[0].queries[0].state, queries[0].state);
    assert_eq!(
        exported[0].queries[0]
            .log
            .as_ref()
            .unwrap()
            .file_seeds
            .len(),
        1
    );
    assert_eq!(
        exported[0].queries[0]
            .log
            .as_ref()
            .unwrap()
            .gallery_seeds
            .len(),
        1
    );
    assert_eq!(
        exported[0].queries[0].reference_header.as_ref().unwrap()[2][15],
        reference["single"][2][0][3][1][0][2][15]
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 880, 610);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("subscription-exchange-export.png"),
        &pixels,
        880,
        610,
    )
    .unwrap();
    let exports = tempfile::tempdir().unwrap();
    let path = exports.path().join("subscriptions.json");
    child.set_path(path.to_string_lossy().as_ref().into());
    child.invoke_action("save-json".into());
    assert!(child.get_error().is_empty(), "{}", child.get_error());
    assert_eq!(
        exchange::decode_text(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        exported
    );
    std::fs::write(&path, "previous file").unwrap();
    child.invoke_action("save-json".into());
    let question = reference["json_files"]["overwrite"][0]["question"]
        .as_str()
        .unwrap()
        .replace("{path}", path.to_string_lossy().as_ref());
    assert_eq!(child.get_overwrite_question().as_str(), question);
    child.invoke_action("no-json".into());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous file");
    child.invoke_action("save-json".into());
    child.invoke_action("yes-json".into());
    assert_eq!(
        exchange::decode_text(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        exported
    );
    std::fs::write(&path, "previous file").unwrap();
    child.invoke_action("save-json".into());
    dialog.invoke_cancel();
    child.invoke_action("yes-json".into());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "previous file");
}

#[test]
fn subscription_exchange_file_menus_load_selected_packages_and_cancel_invalid_batches() {
    let reference = hydrus_testkit::fixture_json("subscription_exchange.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let path = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
    std::fs::write(path.path(), reference["json_files"]["exported"].to_string()).unwrap();
    let dialog = open_dialog(&ui, &bound);
    let chosen = path.path().to_path_buf();
    let caption = reference["json_files"]["dialogs"][1]["title"]
        .as_str()
        .unwrap()
        .to_owned();
    hydrus_gui::set_picker(move |kind, title| {
        assert_eq!(kind, hydrus_gui::Pick::Files);
        assert_eq!(title, caption);
        vec![chosen.clone()]
    });
    dialog.invoke_exchange_mode(4);
    dialog.invoke_chosen(0);
    assert_eq!(
        serde_json::json!(
            rows(&dialog)
                .iter()
                .map(|(row, _)| row[0].clone())
                .collect::<Vec<_>>()
        ),
        reference["json_files"]["imported_names"]
    );
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    dialog.invoke_cancel();
    // A cancelled picker adds nothing.
    let dialog = open_dialog(&ui, &bound);
    hydrus_gui::set_picker(|_, _| Vec::new());
    dialog.invoke_exchange_mode(4);
    assert!(rows(&dialog).is_empty());
    assert!(!dialog.get_asking());
    // PNGs are read with the reference's picker title.
    let chosen = hydrus_testkit::fixtures_dir().join("subscription_exchange.png");
    hydrus_gui::set_picker(move |kind, title| {
        assert_eq!(kind, hydrus_gui::Pick::Files);
        assert_eq!(title, "select the png or pngs with the encoded data");
        vec![chosen.clone()]
    });
    dialog.invoke_exchange_mode(5);
    assert!(asked(&dialog).1.ends_with("objects added!"));
    dialog.invoke_chosen(0);
    assert!(!rows(&dialog).is_empty());
    dialog.invoke_cancel();
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    hydrus_gui::set_picker(|_, _| Vec::new());
}

#[test]
fn legacy_subscription_clipboard_import_reaches_saved_query_settings_and_full_histories() {
    use hydrus_downloader_exchange::subscriptions as exchange;
    let reference = hydrus_testkit::fixture_json("subscription_legacy_exchange.json");
    let _windows = headless::init();
    for case_index in [2, 5] {
        let case = &reference["cases"][case_index];
        let expected = exchange::decode_text(&case["normalised"].to_string())
            .unwrap()
            .remove(0);
        let (_dirs, store) = store();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let dialog = open_dialog(&ui, &bound);
        let text = case["source"].to_string();
        hydrus_gui::set_paster(move || text.clone());
        dialog.invoke_exchange_mode(3);
        assert!(!bound.subscription_exchange.has_open());
        assert_eq!(asked(&dialog).1, "1 objects added!");
        dialog.invoke_chosen(0);
        assert_eq!(rows(&dialog)[0].0[0], "Legacy artist");
        assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
        dialog.invoke_apply();
        let saved = store.read(subscriptions::subscriptions).unwrap();
        let mut saved_settings = saved[0].settings.clone();
        if case_index == 5 {
            assert_eq!(saved_settings.gug_name, "unknown downloader");
            assert!(saved_settings.paused);
            assert_eq!(saved_settings.initial_file_limit, Some(1000));
            assert_eq!(saved_settings.periodic_file_limit, Some(1000));
            assert_eq!(saved_settings.gug_key.len(), 64);
            assert!(!saved_settings.gug_key.is_empty());
            saved_settings
                .gug_key
                .clone_from(&expected.settings.gug_key);
        }
        assert_eq!(saved_settings, expected.settings);
        let id = saved[0].id;
        let queries = store
            .read(move |conn| subscriptions::queries(conn, id))
            .unwrap();
        assert_eq!(queries[0].state, expected.queries[0].state);
        let queue = queries[0].queue_id;
        let file = store
            .read(move |conn| queues::file_seeds(conn, queue))
            .unwrap();
        assert_eq!(file[0].meta.hashes, [("sha256".into(), "33".repeat(32))]);
        let gallery = store
            .read(move |conn| queues::gallery_seeds(conn, queue))
            .unwrap();
        if case_index == 5 {
            assert!(gallery.is_empty());
        } else {
            assert_eq!(gallery[0].note, "gallery failure");
        }
        let dialog = open_dialog(&ui, &bound);
        dialog.invoke_row_clicked(0, false, false);
        dialog.invoke_exchange();
        let child = bound
            .subscription_exchange
            .0
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let exported = exchange::decode_text(child.get_text().as_str()).unwrap();
        let mut exported_settings = exported[0].settings.clone();
        if case_index == 5 {
            exported_settings
                .gug_key
                .clone_from(&expected.settings.gug_key);
        }
        assert_eq!(exported_settings, expected.settings);
        assert_eq!(exported[0].queries[0].state, expected.queries[0].state);
        assert_eq!(
            exported[0].queries[0].log.as_ref().unwrap().file_seeds,
            expected.queries[0].log.as_ref().unwrap().file_seeds
        );
        assert_eq!(
            exported[0].queries[0].log.as_ref().unwrap().gallery_seeds,
            expected.queries[0].log.as_ref().unwrap().gallery_seeds
        );
        dialog.invoke_cancel();
    }
}

#[test]
fn actual_subscription_list_transport_choices_dispatch_frozen_packages_and_guard_owners() {
    use hydrus_downloader_exchange::subscriptions as exchange;
    use std::{cell::RefCell, rc::Rc};
    fn child(bound: &Bound) -> hydrus_gui::DownloaderExchangeWindow {
        bound
            .subscription_exchange
            .0
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    }
    let reference = hydrus_testkit::fixture_json("subscription_exchange.json");
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    for (actual, expected) in [
        (
            dialog.get_exchange_export_labels(),
            &reference["menus"]["export"],
        ),
        (
            dialog.get_exchange_import_labels(),
            &reference["menus"]["import"],
        ),
    ] {
        assert_eq!(
            actual.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            expected
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
    }
    let text = reference["single"].to_string();
    hydrus_gui::set_paster(move || text.clone());
    dialog.invoke_exchange_mode(3);
    assert!(!bound.subscription_exchange.has_open());
    assert_eq!(asked(&dialog).1, "1 objects added!");
    dialog.invoke_chosen(0);
    assert_eq!(rows(&dialog)[0].0[0], "Artist");
    let copies = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copies = copies.clone();
        move |clip| copies.borrow_mut().push(clip.clone())
    });
    dialog.invoke_exchange_mode(0);
    assert!(!bound.subscription_exchange.has_open());
    let exported = match &copies.borrow()[0] {
        hydrus_gui::Clip::Text(text) => exchange::decode_text(text).unwrap(),
        hydrus_gui::Clip::Files(_) => panic!("selected subscriptions must export serialized text"),
    };
    assert_eq!(exported[0].name, "Artist");
    assert_eq!(
        exported[0].queries[0].log.as_ref().unwrap().file_seeds[0].note,
        "ignored\nrecorded reason"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selected.json");
    let picked = path.clone();
    hydrus_gui::set_picker(move |_, title| {
        assert_eq!(title, "select where to save the json file");
        vec![picked.clone()]
    });
    dialog.invoke_exchange_mode(1);
    assert_eq!(
        exchange::decode_text(&std::fs::read_to_string(&path).unwrap()).unwrap(),
        exported
    );
    child(&bound).invoke_action("cancel".into());
    let picked = path.clone();
    hydrus_gui::set_picker(move |_, title| {
        assert_eq!(title, "select the json or jsons with the serialised data");
        vec![picked.clone()]
    });
    dialog.invoke_exchange_mode(4);
    assert!(!bound.subscription_exchange.has_open());
    assert_eq!(asked(&dialog).1, "1 objects added!");
    dialog.invoke_chosen(0);
    assert_eq!(rows(&dialog).len(), 2);
    dialog.invoke_exchange_mode(2);
    let png = hydrus_gui::png_export_window::last().unwrap();
    assert!(child(&bound).get_png_child());
    let stale_path = dir.path().join("closed.png");
    png.set_path(stale_path.to_string_lossy().as_ref().into());
    png.set_png_title("closed subscription package".into());
    dialog.invoke_cancel();
    assert!(!bound.subscription_exchange.has_open());
    assert!(!bound.subscription_exchange.1.has_open());
    let before = copies.borrow().len();
    dialog.invoke_exchange_mode(0);
    dialog.invoke_exchange_mode(3);
    png.invoke_action("export".into());
    assert!(!stale_path.exists());
    assert_eq!(copies.borrow().len(), before);
    assert!(!bound.subscription_exchange.has_open());
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    hydrus_gui::set_picker(|_, _| Vec::new());
    hydrus_gui::set_clipper(|_| {});
    drop(windows);
}

// leaf: subscriptions-reset
// leaf: subscriptions-retry-failed
// leaf: subscriptions-retry-ignored
#[test]
fn subscription_reset_and_retries_refresh_persisted_export_caches_and_forget_file_hashes() {
    use hydrus_downloader_exchange::subscriptions as exchange;
    use hydrus_gui_model::subscription_exchange::Headers;
    fn child(bound: &Bound) -> hydrus_gui::DownloaderExchangeWindow {
        bound
            .subscription_exchange
            .0
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    }
    let reference = hydrus_testkit::fixture_json("subscription_exchange.json");
    let _windows = headless::init();
    for case in reference["log_changes"].as_array().unwrap() {
        let (_dirs, store) = store();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let dialog = open_dialog(&ui, &bound);
        let input = case["input"].to_string();
        hydrus_gui::set_paster(move || input.clone());
        dialog.invoke_exchange_mode(3);
        assert!(!bound.subscription_exchange.has_open());
        assert_eq!(asked(&dialog).1, "1 objects added!");
        dialog.invoke_chosen(0);
        dialog.invoke_apply();
        let saved = store.read(subscriptions::subscriptions).unwrap();
        let id = saved[0].id;
        let query = store
            .read(move |conn| subscriptions::queries(conn, id))
            .unwrap()
            .remove(0);
        let queue = query.queue_id;
        let before = store
            .read(move |conn| Ok(hydrus_store::settings::get::<Headers>(conn)?.0[&queue].clone()))
            .unwrap();
        let dialog = open_dialog(&ui, &bound);
        dialog.invoke_row_clicked(0, false, false);
        match case["action"].as_str().unwrap() {
            "reset" => {
                dialog.invoke_reset();
                dialog.invoke_chosen(0);
            }
            "retry_failed" => {
                dialog.invoke_retry_failed();
            }
            "retry_ignored" => {
                dialog.invoke_retry_ignored();
                dialog.invoke_chosen(0);
            }
            other => panic!("unknown recorded log action {other}"),
        }
        dialog.invoke_apply();
        let seeds = store
            .read(move |conn| queues::file_seeds(conn, queue))
            .unwrap();
        let header = store
            .read(move |conn| Ok(hydrus_store::settings::get::<Headers>(conn)?.0[&queue].clone()))
            .unwrap();
        let expected = &case["output"][2][0][3][1][0];
        assert_eq!(header[2][9][2][1], expected[2][9][2][1]);
        assert_eq!(header[2][9][2][2], expected[2][9][2][2]);
        assert_eq!(header[2][13], before[2][13]);
        assert_eq!(header[2][14], before[2][14]);
        assert_eq!(header[2][16], before[2][16]);
        if case["action"] == "reset" {
            assert!(seeds.is_empty());
            assert!(header[2][15].is_null());
        } else {
            assert_eq!(seeds[0].status, SeedStatus::Unknown);
            assert!(seeds[0].note.is_empty());
            assert!(seeds[0].meta.hashes.is_empty());
            assert_eq!(header[2][15][2][4].as_i64().unwrap(), seeds[0].modified);
            assert_eq!(header[2][15][2][6], 0);
            assert_eq!(header[2][15][2][7], "");
            assert_eq!(header[2][15][2][16], serde_json::json!([]));
        }
        let dialog = open_dialog(&ui, &bound);
        dialog.invoke_row_clicked(0, false, false);
        dialog.invoke_exchange();
        let exported = exchange::decode_text(child(&bound).get_text().as_str()).unwrap();
        assert_eq!(
            exchange::query_header_tuple(&exported[0].queries[0]).unwrap()[2][9],
            header[2][9]
        );
        assert_eq!(
            exported[0].queries[0]
                .log
                .as_ref()
                .unwrap()
                .file_seeds
                .len(),
            seeds.len()
        );
        dialog.invoke_cancel();
    }
}

/// A query made natively is exported, through the real window, as the
/// reference's add-query header: unsynced, default velocity, no examples, so
/// the reference's own Sync recalculates it
/// (`oracle/fixtures/subscription_header_resync.json`).
#[test]
fn a_fresh_native_query_exports_through_the_window_as_the_references_unsynced_header() {
    use hydrus_core::subscriptions::QueryState;
    let fixture = hydrus_testkit::fixture_json("subscription_header_resync.json");
    let (_dirs, store) = store();
    store
        .write(|tx| {
            let id = subscriptions::create_subscription(
                tx.conn(),
                "Fresh",
                &SubscriptionSettings::default(),
            )?
            .unwrap();
            subscriptions::add_query(tx.conn(), id, &QueryState::new("fresh query"), now())?;
            Ok(())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    dialog.invoke_row_clicked(0, false, false);
    dialog.invoke_exchange();
    let child = bound
        .subscription_exchange
        .0
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let exported: serde_json::Value = serde_json::from_str(child.get_text().as_str()).unwrap();
    let header = &exported[2][0][3][1][0];
    let recorded = &fixture["fresh"];
    for index in [8, 13, 14, 15, 16] {
        assert_eq!(header[2][index], recorded[2][index], "header field {index}");
    }
    // (an empty history: no files by status, none found yet)
    assert_eq!(header[2][9][2][1], recorded[2][9][2][1]);
    assert_eq!(header[2][9][2][2], recorded[2][9][2][2]);
    dialog.invoke_cancel();
}

#[test]
fn direct_import_menus_replay_qt_type_warnings_file_prefixes_and_missing_rejection() {
    use hydrus_downloader_exchange::subscriptions as exchange;
    let fixture = hydrus_testkit::fixture_json("subscription_import_flow.json");
    let recorded_now = fixture["now"].as_i64().unwrap();
    // Slint's platform is initialized once per thread. Each independent case
    // owns its windows/context until the joined thread releases them.
    for case in fixture["cases"].as_array().unwrap().iter().cloned() {
        std::thread::spawn(move || {
            let (_dirs, store) = store();
            let _windows = headless::init();
            let ui = MainWindow::new().unwrap();
            let bound = bind(&ui, Pages::open(store.clone()).unwrap());
            let dialog = open_dialog(&ui, &bound);
            let files = tempfile::tempdir().unwrap();
            let mode = case["mode"].as_str().unwrap();
            if mode == "clipboard" {
                let text = case["sources"][case["inputs"][0].as_str().unwrap()]
                    .as_str()
                    .unwrap()
                    .to_owned();
                hydrus_gui::set_paster(move || text.clone());
            } else {
                let mut paths = Vec::new();
                for (index, key) in case["inputs"].as_array().unwrap().iter().enumerate() {
                    let path = files.path().join(format!("{index}.{mode}"));
                    let Some(source) = case["sources"][key.as_str().unwrap()].as_str() else {
                        paths.push(path);
                        continue;
                    };
                    let bytes = if mode == "png" && source != "not json" {
                        hydrus_downloader_exchange::text_png::encode(source, 512, &vec![255; 512])
                            .unwrap()
                    } else {
                        source.as_bytes().to_vec()
                    };
                    std::fs::write(&path, bytes).unwrap();
                    paths.push(path);
                }
                let expected_title = if mode == "png" {
                    "select the png or pngs with the encoded data"
                } else {
                    "select the json or jsons with the serialised data"
                };
                hydrus_gui::set_picker(move |_, title| {
                    assert_eq!(title, expected_title);
                    paths.clone()
                });
            }
            dialog.invoke_exchange_mode(match mode {
                "clipboard" => 3,
                "json" => 4,
                "png" => 5,
                _ => unreachable!(),
            });
            assert!(
                !bound.subscription_exchange.has_open(),
                "actual menu imports add to the list directly"
            );
            for notice in case["messages"].as_array().unwrap() {
                let (title, text, _) = asked(&dialog);
                let shown = rows(&dialog);
                assert_eq!(
                    serde_json::json!(shown.iter().map(|r| &r.0[0]).collect::<Vec<_>>()),
                    notice["during"],
                    "{}",
                    case["case"]
                );
                assert_eq!(
                    serde_json::json!(
                        shown
                            .iter()
                            .filter(|r| r.1)
                            .map(|r| &r.0[0])
                            .collect::<Vec<_>>()
                    ),
                    notice["selected"]
                );
                match notice["kind"].as_str().unwrap() {
                    "warning" => {
                        assert_eq!(title, "Warning");
                        let expected = notice["text"].as_str().unwrap();
                        let mut actual_types = text
                            .split("\n\n")
                            .nth(1)
                            .unwrap()
                            .lines()
                            .collect::<Vec<_>>();
                        let mut expected_types = expected
                            .split("\n\n")
                            .nth(1)
                            .unwrap()
                            .lines()
                            .collect::<Vec<_>>();
                        actual_types.sort_unstable();
                        expected_types.sort_unstable();
                        assert_eq!(actual_types, expected_types);
                        assert_eq!(
                            text.split("\n\n").skip(2).collect::<Vec<_>>(),
                            expected.split("\n\n").skip(2).collect::<Vec<_>>()
                        );
                    }
                    "information" => {
                        assert_eq!(title, "Information");
                        assert_eq!(text, notice["text"]);
                    }
                    "question" => {
                        assert_eq!(title, notice["title"]);
                        assert_eq!(text, notice["text"]);
                    }
                    "critical" => {
                        assert_eq!(title, notice["title"]);
                        assert!(!text.is_empty());
                    }
                    _ => unreachable!(),
                }
                dialog.invoke_apply();
                assert!(
                    store.read(subscriptions::subscriptions).unwrap().is_empty(),
                    "notices and missing-history decisions retain the draft owner"
                );
                if notice["kind"] == "question" {
                    dialog.invoke_chosen(1);
                } else {
                    dialog.invoke_chosen(0);
                }
            }
            assert!(!dialog.get_asking());
            let shown = rows(&dialog);
            assert_eq!(
                serde_json::json!(shown.iter().map(|r| &r.0[0]).collect::<Vec<_>>()),
                case["names"]
            );
            assert_eq!(
                serde_json::json!(
                    shown
                        .iter()
                        .filter(|r| r.1)
                        .map(|r| &r.0[0])
                        .collect::<Vec<_>>()
                ),
                case["selected"]
            );
            dialog.invoke_apply();
            let saved = store.read(subscriptions::subscriptions).unwrap();
            assert_eq!(
                serde_json::json!(saved.iter().map(|s| &s.name).collect::<Vec<_>>()),
                case["names"]
            );
            let mut queue_ids = std::collections::BTreeSet::new();
            let mut history_names = std::collections::BTreeSet::new();
            let headers = store
                .read(
                    hydrus_store::settings::get::<hydrus_gui_model::subscription_exchange::Headers>,
                )
                .unwrap();
            for saved in saved {
                let object = case["exported"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|object| object[2][0][1] == saved.name)
                    .unwrap();
                let expected = exchange::decode_text_at(&object.to_string(), recorded_now)
                    .unwrap()
                    .remove(0);
                assert_eq!(saved.settings, expected.settings);
                let queries = store
                    .read(|conn| subscriptions::queries(conn, saved.id))
                    .unwrap();
                assert_eq!(queries.len(), expected.queries.len());
                for (query, expected) in queries.iter().zip(&expected.queries) {
                    assert!(
                        queue_ids.insert(query.queue_id),
                        "each import owns a different queue even for identical query text"
                    );
                    let name = headers.0[&query.queue_id][2][0].as_str().unwrap();
                    assert_eq!(name.len(), 64);
                    assert!(history_names.insert(name.to_owned()));
                    assert_ne!(name, expected.log_name);
                    assert_eq!(query.state, expected.state);
                    let mut actual = store
                        .read(|conn| {
                            hydrus_gui_model::subscription_exchange::history(
                                conn,
                                query.queue_id,
                                "native history",
                            )
                            .map_err(hydrus_store::StoreError::Invalid)
                        })
                        .unwrap();
                    let expected = expected.log.as_ref().unwrap();
                    actual.name.clone_from(&expected.name);
                    assert_eq!(
                        &actual, expected,
                        "saved ordered file and gallery histories"
                    );
                }
            }
            let reopened = open_dialog(&ui, &bound);
            assert_eq!(
                serde_json::json!(rows(&reopened).iter().map(|r| &r.0[0]).collect::<Vec<_>>()),
                case["names"]
            );
            reopened.invoke_cancel();
            hydrus_gui::set_picker(|_, _| Vec::new());
        })
        .join()
        .unwrap();
    }
}

#[test]
fn direct_import_picker_and_pending_notices_are_invalid_after_owner_cancel() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_flow.json");
    let text = fixture["cases"][4]["sources"]["a"]
        .as_str()
        .unwrap()
        .to_owned();
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("subscription.json");
    std::fs::write(&path, &text).unwrap();
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    hydrus_gui::set_picker({
        let owner = dialog.as_weak();
        let path = path.clone();
        move |_, _| {
            owner.upgrade().unwrap().invoke_cancel();
            vec![path.clone()]
        }
    });
    // The modal picker can close its owner before returning. No mutable owner
    // borrow survives the picker and its returned paths cannot stage anything.
    dialog.invoke_exchange_mode(4);
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    let successor = open_dialog(&ui, &bound);
    dialog.invoke_chosen(0);
    dialog.invoke_apply();
    dialog.invoke_exchange_mode(3);
    assert!(rows(&successor).is_empty());
    hydrus_gui::set_picker(move |_, _| vec![path.clone(), path.clone()]);
    successor.invoke_exchange_mode(4);
    assert_eq!(rows(&successor).len(), 1);
    assert_eq!(asked(&successor).1, "1 objects added!");
    successor.invoke_cancel();
    let fresh = open_dialog(&ui, &bound);
    successor.invoke_chosen(0);
    successor.invoke_cancelled();
    successor.invoke_apply();
    assert!(rows(&fresh).is_empty());
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    fresh.invoke_cancel();
    hydrus_gui::set_picker(|_, _| Vec::new());
}

#[test]
fn direct_missing_history_acceptance_uses_empty_owned_queues_and_dismissal_continues() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_flow.json");
    let reference = &fixture["cases"][2];
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let dialog = open_dialog(&ui, &bound);
    let source = reference["sources"]["missing"].as_str().unwrap().to_owned();
    hydrus_gui::set_paster(move || source.clone());
    dialog.invoke_exchange_mode(3);
    assert_eq!(asked(&dialog).0, reference["messages"][0]["title"]);
    assert_eq!(rows(&dialog).len(), 1);
    dialog.invoke_apply();
    assert!(store.read(subscriptions::subscriptions).unwrap().is_empty());
    dialog.invoke_chosen(0);
    assert_eq!(rows(&dialog).len(), 3);
    assert_eq!(asked(&dialog).1, "3 objects added!");
    // Closing an informational notice resumes its owned continuation.
    dialog.invoke_cancelled();
    assert!(!dialog.get_asking());
    dialog.invoke_apply();
    let saved = store.read(subscriptions::subscriptions).unwrap();
    let incomplete = saved.iter().find(|s| s.name == "Incomplete").unwrap();
    let queries = store
        .read(|conn| subscriptions::queries(conn, incomplete.id))
        .unwrap();
    assert!(!queries.is_empty());
    for query in queries {
        assert!(
            store
                .read(|conn| queues::file_seeds(conn, query.queue_id))
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .read(|conn| queues::gallery_seeds(conn, query.queue_id))
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(saved.len(), 3);
    let reopened = open_dialog(&ui, &bound);
    assert_eq!(rows(&reopened).len(), 3);
    reopened.invoke_cancel();
}
