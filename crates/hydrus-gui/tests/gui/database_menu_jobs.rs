//! The Database menu's maintenance entries, driven from the real menu bar
//! against `oracle/record_database_maintenance.py`: the entry opens the
//! reference's question (its words and buttons), refusing it changes and
//! publishes nothing, and accepting it (choosing the first service when asked
//! which) runs the native job and sends the popups the reference sent.
//!
//! Entries whose reference cache the native store does not keep (ADR-1) still
//! ask the same question and report a clean run; they are driven here but not
//! tagged as leaves (the report names them).

use std::time::{Duration, Instant};

use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_gui_model::database_maintenance::{Asking, Job};
use hydrus_store::{Store, popups};
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

pub fn pump(for_: Duration) {
    let until = Instant::now() + for_;
    while Instant::now() < until {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let lines = ui
        .get_menu_panes()
        .row_data(usize::try_from(pane).unwrap())
        .unwrap()
        .lines;
    let index = lines
        .iter()
        .position(|row| row.label == label)
        .unwrap_or_else(|| panic!("the menu offers {label:?}"));
    assert!(lines.row_data(index).unwrap().usable, "{label:?} is usable");
    ui.invoke_menu_line_clicked(pane, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}

/// database > each of `submenus` in turn > `label`
pub fn from_menu_path(ui: &MainWindow, submenus: &[&str], label: &str) {
    let database = ui
        .get_menu_titles()
        .iter()
        .position(|title| title.label == "database")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(database).unwrap(), 20.0, 22.0);
    for (pane, submenu) in submenus.iter().enumerate() {
        choose(ui, i32::try_from(pane).unwrap(), submenu);
    }
    choose(ui, i32::try_from(submenus.len()).unwrap(), label);
}

/// database > `submenu` > `label…`
pub fn from_menu(ui: &MainWindow, submenu: &str, label: &str) {
    from_menu_path(ui, &[submenu], &format!("{label}\u{2026}"));
}

pub fn basic() -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

pub fn shown(store: &Store) -> Vec<(Option<String>, Option<String>)> {
    store
        .read(|conn| popups::all(conn, hydrus_core::TimestampMs::now().secs()))
        .unwrap()
        .into_iter()
        .map(|p| (p.status_title, p.status_text_1))
        .collect()
}

fn recorded_popups(event: &Value) -> Vec<(Option<String>, Option<String>)> {
    event["popups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| !p["dismissed"].as_bool().unwrap())
        .map(|p| {
            (
                p["title"].as_str().map(str::to_owned),
                p["text_1"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

/// Ask for `job` from the menu, refuse, ask again and accept.
fn drive(job: Job) {
    let fixture = hydrus_testkit::fixture_json("database_maintenance.json");
    let events: Vec<&Value> = fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["label"] == job.label())
        .collect();
    let (refused, accepted) = (events[0], events[1]);
    let submenu = refused["menu"].as_str().unwrap();

    let (_dir, store) = basic();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound: Bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let before = shown(&store);

    // the first question, in the reference's words and buttons
    let asked = &refused["asked"][0];
    let open = |ui: &MainWindow| from_menu(ui, submenu, job.label());
    let slot = &bound.database_maintenance;
    open(&ui);
    match job.asking() {
        Asking::YesNo { .. } => {
            let question = slot.question().expect("a yes/no question opened");
            assert!(question.window().is_visible());
            assert_eq!(question.get_message(), asked["message"].as_str().unwrap());
            assert_eq!(
                question.get_yes_label(),
                asked["yes_label"].as_str().unwrap()
            );
            assert_eq!(question.get_no_label(), asked["no_label"].as_str().unwrap());
            question.invoke_answered(false);
            assert!(slot.question().is_none());
        }
        Asking::YesYesNo { .. } | Asking::Buttons { .. } => {
            let chooser = slot.chooser().expect("a button question opened");
            assert!(chooser.window().is_visible());
            assert_eq!(chooser.get_message(), asked["message"].as_str().unwrap());
            let offered: Vec<String> = chooser
                .get_choices()
                .iter()
                .map(|c| c.to_string())
                .collect();
            let key = if asked["kind"] == "buttons" {
                "choices"
            } else {
                "yes"
            };
            let recorded: Vec<String> = asked[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c[0].as_str().unwrap().to_owned())
                .collect();
            assert_eq!(offered, recorded);
            if asked["kind"] == "yes_yes_no" {
                assert_eq!(chooser.get_no_label(), asked["no_label"].as_str().unwrap());
            } else {
                assert_eq!(chooser.get_window_title(), asked["title"].as_str().unwrap());
            }
            chooser.invoke_cancelled();
            assert!(slot.chooser().is_none());
        }
    }
    // refusing writes nothing and says nothing
    pump(Duration::from_millis(300));
    assert_eq!(shown(&store), before, "{job:?} refused");

    // accepting
    open(&ui);
    if let Some(question) = slot.question() {
        question.invoke_answered(true);
    }
    let wants_service = accepted["asked"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["title"] == "Which service?");
    assert_eq!(wants_service, job.chooses_service());
    if let Some(chooser) = slot.chooser() {
        chooser.invoke_chosen(0);
    }
    assert!(slot.question().is_none() && slot.chooser().is_none());

    let expected = recorded_popups(accepted);
    if job == Job::TablesUsingDefinitions {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !shown(&store).iter().any(|(_, text)| {
            text.as_deref()
                .is_some_and(|t| t.ends_with("table and column pairs sent to clipboard."))
        }) {
            assert!(Instant::now() < deadline, "the tables were never listed");
            pump(Duration::from_millis(20));
        }
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while shown(&store) != expected && !expected.is_empty() {
        assert!(
            Instant::now() < deadline,
            "{job:?}: popups {:?}, wanted {expected:?}",
            shown(&store)
        );
        pump(Duration::from_millis(20));
    }
    pump(Duration::from_millis(300));
    assert_eq!(shown(&store), expected, "{job:?}");
}

// leaf: audit-media-menu-database-analyze
#[test]
fn analyze_from_the_database_menu() {
    drive(Job::Analyze);
}

// leaf: audit-media-menu-database-clear-fix-orphan-file-records
#[test]
fn orphan_file_records_from_the_database_menu() {
    drive(Job::OrphanFileRecords);
}

// leaf: audit-media-menu-database-clear-orphan-url-mappings
#[test]
fn orphan_url_mappings_from_the_database_menu() {
    drive(Job::OrphanUrlMappings);
}

// leaf: audit-media-menu-database-clear-orphan-tables
#[test]
fn orphan_tables_from_the_database_menu() {
    drive(Job::OrphanTables);
}

// leaf: audit-media-menu-database-get-tables-using-definitions
#[test]
fn tables_using_definitions_from_the_database_menu() {
    drive(Job::TablesUsingDefinitions);
}

// leaf: audit-media-database-repair-invalid-tags
#[test]
fn fix_invalid_tags_from_the_database_menu() {
    drive(Job::FixInvalidTags);
}

// leaf: audit-media-menu-database-check-and-repair-fix-logically-inconsistent-mappings
#[test]
fn fix_inconsistent_mappings_from_the_database_menu() {
    drive(Job::FixInconsistentMappings);
}

// leaf: audit-media-menu-database-check-and-repair-resync-tag-mappings-cache-files
#[test]
fn resync_tag_counts_from_the_database_menu() {
    drive(Job::ResyncTagCounts);
}

// leaf: audit-media-menu-database-regenerate-tag-storage-mappings-cache-all-with-deferred-siblings-parents-calculation
#[test]
fn tag_storage_all_from_the_database_menu() {
    drive(Job::TagStorage);
}

// leaf: audit-media-menu-database-regenerate-tag-storage-mappings-cache-just-pending-tags-instant-calculation
#[test]
fn tag_storage_pending_from_the_database_menu() {
    drive(Job::TagStoragePending);
}

// leaf: audit-media-menu-database-regenerate-tag-text-search-cache
#[test]
fn tag_text_from_the_database_menu() {
    drive(Job::TagText);
}

// leaf: audit-media-menu-database-regenerate-tag-text-search-cache-subtags-repopulation
#[test]
fn tag_text_subtags_from_the_database_menu() {
    drive(Job::TagTextSubtags);
}

// leaf: audit-media-menu-database-regenerate-tag-text-search-cache-searchable-subtag-maps
#[test]
fn tag_text_searchable_from_the_database_menu() {
    drive(Job::TagTextSearchable);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn pending_count_from_the_database_menu() {
    drive(Job::PendingCount);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn tag_display_all_from_the_database_menu() {
    drive(Job::TagDisplay);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn tag_display_pending_from_the_database_menu() {
    drive(Job::TagDisplayPending);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn tag_display_repopulate_from_the_database_menu() {
    drive(Job::TagDisplayRepopulate);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn siblings_lookup_from_the_database_menu() {
    drive(Job::SiblingsLookup);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn parents_lookup_from_the_database_menu() {
    drive(Job::ParentsLookup);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn local_hashes_from_the_database_menu() {
    drive(Job::LocalHashes);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn local_tags_from_the_database_menu() {
    drive(Job::LocalTags);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn service_info_from_the_database_menu() {
    drive(Job::ServiceInfo);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn similar_tree_from_the_database_menu() {
    drive(Job::SimilarTree);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn repopulate_mappings_from_the_database_menu() {
    drive(Job::RepopulateMappings);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn resync_deleted_from_the_database_menu() {
    drive(Job::ResyncDeleted);
}

// (no native cache: asks the reference's question and reports a clean run)
#[test]
fn orphan_serialisables_from_the_database_menu() {
    drive(Job::OrphanSerialisables);
}

/// Accept `job` from the menu, choosing "all services" if asked which.
fn accept(ui: &MainWindow, bound: &Bound, job: Job) {
    from_menu(ui, submenu_of(job), job.label());
    if let Some(question) = bound.database_maintenance.question() {
        question.invoke_answered(true);
    }
    if let Some(chooser) = bound.database_maintenance.chooser() {
        chooser.invoke_chosen(0);
    }
}

fn submenu_of(job: Job) -> &'static str {
    match job {
        Job::Analyze
        | Job::OrphanFileRecords
        | Job::OrphanUrlMappings
        | Job::OrphanTables
        | Job::TablesUsingDefinitions
        | Job::OrphanSerialisables => "db maintenance",
        Job::FixInvalidTags
        | Job::FixInconsistentMappings
        | Job::RepopulateMappings
        | Job::ResyncDeleted
        | Job::ResyncTagCounts => "check and repair",
        _ => "regenerate",
    }
}

fn wait_for(store: &Store, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !shown(store).iter().any(|(_, t)| t.as_deref() == Some(text)) {
        assert!(Instant::now() < deadline, "never said {text:?}");
        pump(Duration::from_millis(20));
    }
    pump(Duration::from_millis(100));
}

fn count(store: &Store, sql: &str) -> i64 {
    let sql = sql.to_owned();
    store
        .read(move |conn| conn.query_row(&sql, [], |r| r.get(0)).map_err(Into::into))
        .unwrap()
}

// leaf: audit-media-menu-database-clear-orphan-url-mappings
#[test]
fn the_menu_deletes_orphan_url_mappings_and_keeps_good_ones() {
    let (dir, store) = basic();
    store
        .write(|ctx| {
            let hash: i64 =
                ctx.conn()
                    .query_row("SELECT hash_id FROM hashes LIMIT 1", [], |r| r.get(0))?;
            ctx.conn()
                .execute("INSERT INTO file_urls VALUES (?1, 999999)", [hash])?;
            Ok(())
        })
        .unwrap();
    let good = count(
        &store,
        "SELECT COUNT(*) FROM file_urls WHERE url_id != 999999",
    );
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    accept(&ui, &bound, Job::OrphanUrlMappings);
    wait_for(&store, "1 orphan url mappings deleted!");
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        count(
            &reopened,
            "SELECT COUNT(*) FROM file_urls WHERE url_id = 999999"
        ),
        0
    );
    assert_eq!(
        count(
            &reopened,
            "SELECT COUNT(*) FROM file_urls WHERE url_id != 999999"
        ),
        good
    );
}

// leaf: audit-media-menu-database-check-and-repair-fix-logically-inconsistent-mappings
#[test]
fn the_menu_removes_pending_mappings_that_are_also_current() {
    let (_dir, store) = basic();
    let (pending, tag, hash) = store
        .write(|ctx| {
            let registry = hydrus_store::services::ServiceRegistry::load(ctx.conn())?;
            let service = registry.tag_services().next().unwrap().id;
            let tables = hydrus_store::schema::MappingTables::new(service);
            let (tag, hash): (i64, i64) = ctx.conn().query_row(
                &format!("SELECT tag_id, hash_id FROM {} LIMIT 1", tables.current),
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            ctx.conn().execute(
                &format!("INSERT INTO {} VALUES (?1, ?2)", tables.pending),
                [tag, hash],
            )?;
            Ok((tables.pending, tag, hash))
        })
        .unwrap();
    let conflicts =
        format!("SELECT COUNT(*) FROM {pending} WHERE tag_id = {tag} AND hash_id = {hash}");
    assert_eq!(count(&store, &conflicts), 1);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    accept(&ui, &bound, Job::FixInconsistentMappings);
    wait_for(
        &store,
        "Found 1 bad mappings! They _should_ be deleted, and your pending counts should be updated.",
    );
    assert_eq!(count(&store, &conflicts), 0);
}

#[test]
fn set_a_password_is_in_the_menu_and_opens_its_dialog() {
    let (_dir, store) = basic();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let before = windows.count();
    from_menu_path(&ui, &[], "set a password\u{2026}");
    assert_eq!(windows.count(), before + 1, "the dialog opened");
}
