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
