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
    warning.invoke_accept();
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
        chooser.invoke_accept();
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
    chooser.invoke_accept();
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
    chooser.invoke_accept();
    assert_eq!(rows(&list)[0].0[1], "alpha");
    list.invoke_apply();
    let written = store.read(subscriptions::subscriptions).unwrap();
    assert_eq!(written[0].settings.gug_name, "alpha");
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
    warning.invoke_accept();
    assert!(bound.edit_subscription.borrow().is_none());
}
