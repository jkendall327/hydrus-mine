//! The edit subscription dialog, opened from the manage subscriptions
//! dialog on the store's subscriptions: its fields, its queries list's
//! buttons and questions (pasting, retrying, deleting, the query editor),
//! "apply" giving the subscription back to the list, and the list's
//! "apply" writing it all to the store. (Its workings are tested against
//! the reference's recording in hydrus-gui-model's tests.) And "add" with
//! no downloaders, which only says so.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_gui::{EditSubscriptionWindow, MainWindow, Pages, bind, headless, set_paster};
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};
use hydrus_store::subscriptions;

use crate::subscriptions::{now, open_dialog, rows, store};

/// The queries list's first cells (each query's name), in order.
fn names(dialog: &EditSubscriptionWindow) -> Vec<String> {
    let rows = dialog.get_rows();
    (0..rows.row_count())
        .map(|r| {
            rows.row_data(r)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}

fn click(dialog: &EditSubscriptionWindow, name: &str) {
    let row = names(dialog).iter().position(|n| n == name).unwrap();
    dialog.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
}

fn asked(dialog: &EditSubscriptionWindow) -> (String, String, Vec<String>) {
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
fn the_dialog_edits_a_subscription_and_the_list_writes_it() {
    let (_dirs, store) = store();
    let now = now();
    // a subscription with a query with a failed file and an ignored 404,
    // and another query
    let (id, blue) = store
        .write(move |ctx| {
            let conn = ctx.conn();
            let settings = SubscriptionSettings {
                gug_name: "example tag search".into(),
                ..SubscriptionSettings::default()
            };
            let id = subscriptions::create_subscription(conn, "zz artist", &settings)?.unwrap();
            let mut state = QueryState::new("blue eyes");
            state.last_check_time = now - 3600;
            let blue = subscriptions::add_query(conn, id, &state, now)?;
            subscriptions::add_query(conn, id, &QueryState::new("old artist"), now)?;
            let seed = |n: u32| NewFileSeed {
                seed_type: SeedType::Url,
                data: format!("https://booru.example/post/{n}"),
                data_for_comparison: format!("https://booru.example/post/{n}"),
                source_time: None,
                referral_url: None,
                meta: FileSeedMeta::default(),
            };
            queues::add_file_seeds(conn, blue, &[seed(1), seed(2), seed(3)], false, now)?;
            let mut seeds = queues::file_seeds(conn, blue)?;
            seeds[0].status = SeedStatus::Error;
            seeds[1].status = SeedStatus::Vetoed;
            seeds[1].note = "404 not found".into();
            seeds[2].status = SeedStatus::Vetoed;
            seeds[2].note = "creature:goblin is blacklisted!".into();
            for s in &seeds {
                queues::update_file_seed(conn, s)?;
            }
            Ok((id, blue))
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // "edit" on the one selected
    let list = open_dialog(&ui, &bound);
    assert!(!list.get_one_selected());
    list.invoke_row_clicked(0, false, false);
    assert!(list.get_one_selected());
    list.invoke_edit();
    let dialog = bound
        .edit_subscription
        .borrow()
        .as_ref()
        .expect("the edit dialog opens")
        .clone_strong();
    assert_eq!(dialog.get_window_title(), "edit subscription");
    assert_eq!(dialog.get_name(), "zz artist");
    assert_eq!(dialog.get_delay(), "no recent errors");
    // (the basic fixture has no downloaders)
    assert_eq!(dialog.get_downloader(), "not found: example tag search");
    assert_eq!(dialog.get_initial_limit(), 100);
    assert!(dialog.get_show_popup() && !dialog.get_publish_page());
    assert!(dialog.get_label_override_none());
    assert_eq!(dialog.get_import_options(), "import options (all default)");
    assert_eq!(names(&dialog), ["blue eyes", "old artist"]);

    // pasting a new query and one there already
    set_paster(|| "new one\nBlue Eyes\n".into());
    dialog.invoke_paste_queries();
    let (_, message, choices) = asked(&dialog);
    assert_eq!(
        message,
        "I pulled 2 texts from the clipboard.\n\nThis is new: \"new one\"\n\nThis is already working in the subscription: \"blue eyes\"\n\nWould you like to add these new queries?"
    );
    assert_eq!(choices, ["do it", "hold off"]);
    dialog.invoke_chosen(0);
    assert!(!dialog.get_asking());
    assert_eq!(names(&dialog), ["blue eyes", "new one", "old artist"]);
    // (a screenshot, kept in the target directory: the windows are the
    // main window, the list and this)
    let (width, height) = (1100, 720);
    let pixels = headless::render(&windows.get(2).unwrap(), width, height);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("edit_subscription.png"), &pixels, width, height).unwrap();

    // retrying the failed file and the ignored 404
    click(&dialog, "blue eyes");
    assert!(dialog.get_can_retry_failed() && dialog.get_can_retry_ignored());
    assert!(dialog.get_can_reset(), "it was checked");
    dialog.invoke_retry_failed();
    assert!(!dialog.get_can_retry_failed());
    dialog.invoke_retry_ignored();
    let (title, _, choices) = asked(&dialog);
    assert_eq!(title, "select what to retry");
    assert_eq!(
        choices,
        ["retry all", "retry 403s", "retry 404s", "retry blacklisted"]
    );
    dialog.invoke_chosen(2);
    assert!(
        dialog.get_can_retry_ignored(),
        "the blacklisted one is left"
    );

    // deleting one, asking first
    click(&dialog, "old artist");
    dialog.invoke_delete_queries();
    assert_eq!(asked(&dialog).1, "Remove all selected?");
    dialog.invoke_chosen(0);
    assert_eq!(names(&dialog), ["blue eyes", "new one"]);

    // the query editor, giving the new one a display name
    click(&dialog, "new one");
    dialog.invoke_edit_query();
    assert!(dialog.get_editing_query());
    assert_eq!(dialog.get_query_text(), "new one");
    assert!(dialog.get_display_name_none());
    assert_eq!(dialog.get_query_status(), "next check: imminent");
    dialog.set_query_paused(true);
    dialog.invoke_query_status_changed();
    assert_eq!(
        dialog.get_query_status(),
        "next check: paused, but would be imminent"
    );
    dialog.set_query_paused(false);
    dialog.set_display_name_none(false);
    dialog.set_display_name("Newbie".into());
    dialog.invoke_query_apply();
    assert!(!dialog.get_editing_query());
    assert_eq!(names(&dialog), ["blue eyes", "Newbie (new one)"]);

    // an edit to a text another query has is refused
    click(&dialog, "Newbie (new one)");
    dialog.invoke_edit_query();
    dialog.set_query_text("BLUE EYES".into());
    dialog.invoke_query_apply();
    assert_eq!(
        asked(&dialog).1,
        "You already have a query for \"blue eyes\"! The edit you just made will not be saved."
    );
    dialog.invoke_chosen(0);
    assert_eq!(names(&dialog), ["blue eyes", "Newbie (new one)"]);

    // a query's logs, from its editor (a new query has none yet)
    click(&dialog, "Newbie (new one)");
    dialog.invoke_edit_query();
    assert!(!dialog.get_query_has_logs());
    dialog.invoke_query_cancel();
    click(&dialog, "blue eyes");
    dialog.invoke_edit_query();
    assert!(dialog.get_query_has_logs());
    dialog.invoke_query_log(false);
    let log = bound.folders.log.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(log.get_window_title(), "file log");
    assert!(log.get_rows().row_count() > 0);
    dialog.invoke_query_log(true);
    let log = bound.folders.log.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(log.get_window_title(), "search log");
    log.invoke_close_window();
    dialog.invoke_query_cancel();

    // renamed, applied: back in the list
    dialog.set_name("renamed".into());
    dialog.set_publish_page(true);
    dialog.invoke_apply();
    assert!(bound.edit_subscription.borrow().is_none());
    assert_eq!(rows(&list)[0].0[0], "renamed");
    assert_eq!(rows(&list)[0].0[2], "2 working");

    // the list applied: all of it written
    list.invoke_apply();
    let written = store
        .read(move |c| subscriptions::subscription(c, id))
        .unwrap()
        .unwrap();
    assert_eq!(written.name, "renamed");
    assert!(written.settings.publish_files_to_page);
    let queries = store.read(move |c| subscriptions::queries(c, id)).unwrap();
    let texts: Vec<(&str, Option<&str>)> = queries
        .iter()
        .map(|q| (q.state.query_text.as_str(), q.state.display_name.as_deref()))
        .collect();
    assert_eq!(texts, [("blue eyes", None), ("new one", Some("Newbie"))]);
    let statuses: Vec<SeedStatus> = store
        .read(move |c| queues::file_seeds(c, blue))
        .unwrap()
        .iter()
        .map(|s| s.status)
        .collect();
    assert_eq!(
        statuses,
        [SeedStatus::Unknown, SeedStatus::Unknown, SeedStatus::Vetoed]
    );

    // "add", with no downloaders: it says so
    let list = open_dialog(&ui, &bound);
    list.invoke_add();
    assert!(!list.get_asking(), "no embedded Add panel");
    let warning = bound
        .subscription_gallery
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(warning.get_window_title(), "Warning");
    assert!(
        warning
            .get_message()
            .starts_with("Hey, you do not have any downloaders set up in this client")
    );
    warning.invoke_accept_clicked();
    assert!(!list.get_gallery_open());
    assert!(bound.edit_subscription.borrow().is_none());
}

fn quality_menu(dialog: &EditSubscriptionWindow, action: &str) {
    dialog.invoke_quality_menu(10.0, 10.0);
    let panes = dialog.get_quality_panes();
    assert_eq!(panes.row_count(), 1);
    let lines = panes.row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == action)
        .unwrap();
    dialog.invoke_quality_line_clicked(0, i32::try_from(index).unwrap(), 300.0, 100.0, 10.0);
}
fn quality_ready(dialog: &EditSubscriptionWindow) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while dialog.get_quality_working() {
        assert!(
            std::time::Instant::now() < deadline,
            "query quality worker finishes"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
}

#[test]
fn advanced_quality_menu_reads_saved_logs_and_current_media_without_applying_drafts() {
    use hydrus_core::Sha256;
    use hydrus_gui::{Clip, set_clipper};
    use std::{cell::RefCell, rc::Rc};
    let (_dirs, store) = store();
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let hashes: Vec<Sha256> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .take(3)
        .map(|f| f["hash"].as_str().unwrap().parse().unwrap())
        .collect();
    let ids = store
        .read(|c| hydrus_store::master::hash_ids(c, &hashes))
        .unwrap();
    let ordered: Vec<_> = hashes.iter().map(|h| ids[h]).collect();
    store
        .write_content({
            let ordered = ordered.clone();
            move |w| {
                w.inbox(&ordered)?;
                w.archive(&[ordered[1]])?;
                w.delete_files(
                    w.roles().combined_local_media,
                    &[ordered[2]],
                    Some("synthetic quality"),
                )
            }
        })
        .unwrap();
    let (sub, full, empty) = store
        .write(move |ctx| {
            let sub = subscriptions::create_subscription(
                ctx.conn(),
                "quality",
                &SubscriptionSettings::default(),
            )?
            .unwrap();
            let mut q = QueryState::new("synthetic query");
            q.display_name = Some("display, name".into());
            let full = subscriptions::add_query(ctx.conn(), sub, &q, 123)?;
            let empty = subscriptions::add_query(ctx.conn(), sub, &QueryState::new("empty"), 123)?;
            let hashes = [
                hashes[0],
                hashes[1],
                hashes[2],
                hashes[0],
                Sha256([170; 32]),
            ];
            let seeds: Vec<_> = hashes
                .iter()
                .enumerate()
                .map(|(i, h)| NewFileSeed {
                    seed_type: SeedType::Url,
                    data: format!("https://quality.example/{i}"),
                    data_for_comparison: format!("https://quality.example/{i}"),
                    source_time: None,
                    referral_url: None,
                    meta: FileSeedMeta {
                        hashes: vec![("sha256".into(), h.to_string())],
                        ..FileSeedMeta::default()
                    },
                })
                .collect();
            queues::add_file_seeds(ctx.conn(), full, &seeds, false, 123)?;
            Ok((sub, full, empty))
        })
        .unwrap();
    let copied = Rc::new(RefCell::new(Vec::new()));
    set_clipper({
        let copied = copied.clone();
        move |c| copied.borrow_mut().push(c.clone())
    });
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let list = open_dialog(&ui, &bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let ordinary = bound
        .edit_subscription
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(!ordinary.get_quality_visible());
    ordinary.invoke_quality_menu(10.0, 10.0);
    assert_eq!(ordinary.get_quality_panes().row_count(), 0);
    ordinary.invoke_cancel();
    store
        .write(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &hydrus_store::settings::AdvancedMode(true))
        })
        .unwrap();
    list.invoke_edit();
    let dialog = bound
        .edit_subscription
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(dialog.get_quality_visible());
    assert!(!dialog.get_can_quality());
    dialog.invoke_add_query();
    dialog.set_query_text("unsaved query".into());
    dialog.invoke_query_apply();
    click(&dialog, "unsaved query");
    assert!(!dialog.get_can_quality());
    dialog.invoke_quality_menu(10.0, 10.0);
    assert_eq!(dialog.get_quality_panes().row_count(), 0);
    let full_row = names(&dialog)
        .iter()
        .position(|n| n.starts_with("display, name"))
        .unwrap();
    let empty_row = names(&dialog).iter().position(|n| n == "empty").unwrap();
    dialog.invoke_row_clicked(i32::try_from(full_row).unwrap(), false, false);
    dialog.invoke_row_clicked(i32::try_from(empty_row).unwrap(), true, false);
    assert!(dialog.get_can_quality());
    quality_menu(&dialog, "show");
    assert!(dialog.get_quality_working());
    let original_names = names(&dialog);
    dialog.invoke_delete_queries();
    assert_eq!(
        names(&dialog),
        original_names,
        "disabled editor refuses mutation while worker runs"
    );
    quality_ready(&dialog);
    let recorded = hydrus_testkit::fixture_json("subscription_quality.json");
    assert_eq!(
        dialog.get_asking_message().as_str(),
        recorded["messages"][0].as_str().unwrap()
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 1100, 760);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("subscription_quality.png"),
        &pixels,
        1100,
        760,
    )
    .unwrap();
    dialog.invoke_chosen(0);
    quality_menu(&dialog, "copy csv data to clipboard");
    quality_ready(&dialog);
    assert_eq!(
        copied.borrow().last(),
        Some(&Clip::Text(
            recorded["clipboard"][0].as_str().unwrap().into()
        ))
    );
    assert_eq!(
        store
            .read(|c| subscriptions::queries(c, sub))
            .unwrap()
            .len(),
        2,
        "quality never saves a draft query"
    );
    store
        .write_content(move |w| w.archive(&[ordered[0]]))
        .unwrap();
    quality_menu(&dialog, "show");
    quality_ready(&dialog);
    assert!(
        dialog
            .get_asking_message()
            .contains("inbox 0 | archive 2 | deleted 2 | good 50%")
    );
    dialog.invoke_chosen(0);
    let copied_before = copied.borrow().clone();
    quality_menu(&dialog, "copy csv data to clipboard");
    dialog.invoke_cancel();
    std::thread::sleep(std::time::Duration::from_millis(30));
    slint::platform::update_timers_and_animations();
    assert_eq!(
        *copied.borrow(),
        copied_before,
        "closing editor cancels stale worker publication"
    );
    dialog.invoke_quality_menu(10.0, 10.0);
    assert_eq!(dialog.get_quality_panes().row_count(), 0);
    list.invoke_edit();
    let reopened = bound
        .edit_subscription
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(names(&reopened).len(), 2);
    click(&reopened, "empty");
    store
        .write(move |ctx| queues::delete_queue(ctx.conn(), empty))
        .unwrap();
    quality_menu(&reopened, "show");
    quality_ready(&reopened);
    assert!(
        reopened
            .get_asking_message()
            .starts_with("Could not read query quality:")
    );
    reopened.invoke_chosen(0);
    reopened.invoke_cancel();
    assert!(store.read(|c| queues::queue(c, full)).unwrap().is_some());
    list.invoke_cancel();
}
