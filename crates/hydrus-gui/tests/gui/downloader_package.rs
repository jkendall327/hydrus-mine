//! The downloader sharing window as the reference's panels are used: the
//! export list filled by its "add" buttons' choosers and asked about before
//! it becomes a PNG, and the import's questions and notices, replayed from
//! `oracle/record_mixed_login_packages.py` (its real `DownloaderExportPanel`
//! and `ReviewDownloaderImport`, with scripted answers).

use hydrus_gui::{DownloaderExchangeWindow, downloader_interchange_window as windows, headless};
use hydrus_gui_model::downloader_interchange::{self as exchange, Draft, Native};
use hydrus_parse::Downloaders;
use hydrus_store::{Store, settings};
use serde_json::Value;
use slint::Model as _;
use std::sync::Arc;

fn setup() -> (tempfile::TempDir, Arc<Store>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

/// The open chooser: its title and labels.
pub(crate) fn chooser(w: &DownloaderExchangeWindow) -> (String, Vec<String>) {
    (
        w.get_chooser_title().into(),
        w.get_chooser_choices()
            .iter()
            .map(|c| c.label.to_string())
            .collect(),
    )
}

/// Answer the open chooser: cancel it (`None`), or check exactly `labels`
/// and apply.
pub(crate) fn choose(w: &DownloaderExchangeWindow, labels: Option<&[&str]>) {
    let Some(labels) = labels else {
        w.invoke_action("chooser-cancel".into());
        return;
    };
    for (row, choice) in w.get_chooser_choices().iter().enumerate() {
        let want = labels.contains(&choice.label.as_str());
        if choice.included != want {
            w.invoke_chooser_toggled(i32::try_from(row).unwrap(), want);
        }
    }
    w.invoke_action("chooser-ok".into());
}

/// The chooser as recorded (`{title, labels, answer}`), answered as it was.
pub(crate) fn replay_chooser(w: &DownloaderExchangeWindow, recorded: &Value) {
    let labels: Vec<String> = recorded["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        chooser(w),
        (recorded["title"].as_str().unwrap().to_owned(), labels)
    );
    let answer: Option<Vec<&str>> = recorded["answer"]
        .as_array()
        .map(|a| a.iter().map(|l| l.as_str().unwrap()).collect());
    choose(w, answer.as_deref());
    assert!(w.get_chooser_title().is_empty());
}

/// The export list as the recording lists it: each row's name and kind.
pub(crate) fn export_state(w: &DownloaderExchangeWindow) -> Vec<(String, i64)> {
    w.get_export_rows()
        .iter()
        .map(|row| {
            let name = row.cells.row_data(0).unwrap().to_string();
            let kind = match row.cells.row_data(1).unwrap().as_str() {
                "Gallery URL Generator" => 69,
                "Nested Gallery URL Generator" => 70,
                "URL Class" => 50,
                "Page Parser" => 58,
                "Domain Metadata" => 71,
                "Login Script - Domain" => 73,
                other => panic!("{other}"),
            };
            (name, kind)
        })
        .collect()
}

pub(crate) fn recorded_state(state: &Value) -> Vec<(String, i64)> {
    state
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_owned(),
                s["kind"].as_i64().unwrap(),
            )
        })
        .collect()
}

/// The question or notice shown, and whether it asks.
pub(crate) fn shown(w: &DownloaderExchangeWindow) -> (String, bool) {
    (w.get_question().into(), w.get_question_asks())
}

/// The recorder's client: the package's objects registered (as a user
/// would have made them), and a second login script the export doesn't
/// choose.
fn registered(store: &Store, fixture: &Value) {
    let mut draft = Draft::load(store).unwrap();
    draft
        .import(exchange::decode_text(&fixture["reference"].to_string()).unwrap())
        .unwrap();
    draft.save(store).unwrap();
    store
        .write(|ctx| {
            let mut logins = hydrus_store::logins::load(ctx.conn())?;
            let mut other = logins.scripts[0].clone();
            other.name = "unselected package login".into();
            other.key = "72".repeat(32);
            other.examples.truncate(1);
            other.examples[0].domain = "unselected.example".into();
            other.examples[0].description = "Unselected access.".into();
            logins.scripts.push(other);
            hydrus_store::logins::save(ctx.conn(), &logins)
        })
        .unwrap();
}

/// The recorder's import target: an older script by the same name.
fn older_script(store: &Store, fixture: &Value) {
    let Native::Login(mut old) = exchange::decode_text(&fixture["reference"].to_string())
        .unwrap()
        .into_iter()
        .find(|d| matches!(d.native, Native::Login(_)))
        .unwrap()
        .native
    else {
        unreachable!()
    };
    let before = &fixture["before"]["domains"]["packages.example"];
    old.key = before[0].as_str().unwrap().into();
    old.examples.truncate(1);
    old.examples[0].access = hydrus_parse::login::Access::UserPreferences;
    old.examples[0].description = before[4].as_str().unwrap().into();
    store
        .write(move |ctx| {
            let mut logins = hydrus_store::logins::load(ctx.conn())?;
            logins.scripts = vec![old];
            hydrus_store::logins::save(ctx.conn(), &logins)
        })
        .unwrap();
}

fn names<T>(items: &[T], name: impl Fn(&T) -> String) -> Vec<String> {
    let mut names: Vec<String> = items.iter().map(name).collect();
    names.sort();
    names
}

/// The client's downloaders, URL classes, parsers and scripts, by name, as
/// the recording lists them.
fn manager_state(store: &Store) -> Value {
    let downloaders: Downloaders = store.read(settings::get).unwrap();
    let classes: hydrus_core::url::UrlClassSettings = store.read(settings::get).unwrap();
    let logins = store.read(hydrus_store::logins::load).unwrap();
    serde_json::json!({
        "scripts": logins.scripts.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        "gugs": names(&downloaders.gugs.gugs, |g| g.name().to_owned()),
        "classes": names(&classes.url_classes, |c| c.name.clone()),
        "parsers": names(&downloaders.parsers, |p| p.name.clone()),
    })
}

fn recorded_manager(state: &Value) -> Value {
    serde_json::json!({
        "scripts": state["scripts"].as_array().unwrap().iter().map(|s| s["name"].clone()).collect::<Vec<_>>(),
        "gugs": state["gugs"],
        "classes": state["classes"],
        "parsers": state["parsers"],
    })
}

// leaf: audit-network-exchange-unsupported
#[test]
fn a_mixed_package_is_chosen_asked_about_and_imported_as_the_reference_does() {
    let _rendered = headless::init();
    let fixture = hydrus_testkit::fixture_json("mixed_login_packages.json");
    let choosers = fixture["chooser_states"].as_array().unwrap();
    let (dir, store) = setup();
    registered(&store, &fixture);

    // export: the list starts empty; each add button opens its chooser
    let slots = windows::Slots::default();
    let export = windows::package(&store, &slots, false).unwrap();
    assert!(export.get_export_list());
    assert!(export_state(&export).is_empty());
    assert!(export.get_text().is_empty());
    export.invoke_action("add-login-script".into());
    replay_chooser(&export, &choosers[0]);
    assert_eq!(export_state(&export), recorded_state(&fixture["cancelled"]));
    export.invoke_action("add-login-script".into());
    replay_chooser(&export, &choosers[1]);
    assert_eq!(
        export_state(&export),
        recorded_state(&fixture["login_only"])
    );
    // a nested downloader brings the one it runs, its URL classes and parser
    export.invoke_action("add-gug".into());
    replay_chooser(&export, &choosers[2]);
    assert_eq!(export_state(&export), recorded_state(&fixture["mixed"]));
    // what is listed isn't offered again
    export.invoke_action("add-login-script".into());
    replay_chooser(&export, &choosers[3]);
    assert_eq!(export_state(&export), recorded_state(&fixture["mixed"]));
    // the package is the list, in its order
    let listed = exchange::decode_text(export.get_text().as_str()).unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|d| d.name().to_owned())
            .collect::<Vec<_>>(),
        recorded_state(&fixture["mixed"])
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    );

    // "export to png" asks first, with the summary the end-user will see
    let png = dir.path().join("mixed.png");
    export.set_path(png.to_string_lossy().as_ref().into());
    export.invoke_action("save".into());
    assert_eq!(
        shown(&export),
        (
            fixture["export_questions"][0].as_str().unwrap().to_owned(),
            true
        )
    );
    export.invoke_action("no".into());
    assert!(!png.exists(), "no: nothing saved");
    assert!(export.get_question().is_empty());
    export.invoke_action("save".into());
    export.invoke_action("yes".into());
    assert!(export.get_error().is_empty(), "{}", export.get_error());
    assert_eq!(
        exchange::decode_png(&std::fs::read(&png).unwrap()).unwrap(),
        listed
    );
    // a row deleted leaves the list
    export.invoke_export_row_clicked(0, false, false);
    export.invoke_action("delete".into());
    assert_eq!(
        export_state(&export),
        recorded_state(&fixture["mixed"])[1..].to_vec()
    );
    export.invoke_action("cancel".into());

    // import, into a client with an older script by the same name: the
    // final question, declined, changes nothing
    let (_other_dir, other) = setup();
    older_script(&other, &fixture);
    assert_eq!(manager_state(&other), recorded_manager(&fixture["before"]));
    let import = windows::package(&other, &slots, true).unwrap();
    assert!(!import.get_select_from_list(), "on in advanced mode only");
    import.set_path(png.to_string_lossy().as_ref().into());
    import.invoke_action("open".into());
    assert!(import.get_ready(), "{}", import.get_error());
    import.invoke_action("accept".into());
    assert_eq!(
        shown(&import),
        (
            fixture["import_questions"][0].as_str().unwrap().to_owned(),
            true
        )
    );
    import.invoke_action("no".into());
    assert_eq!(
        manager_state(&other),
        recorded_manager(&fixture["declined"])
    );
    // accepted: the objects, and the notice saying so
    import.invoke_action("accept".into());
    assert_eq!(
        shown(&import).0,
        fixture["import_questions"][1].as_str().unwrap()
    );
    import.invoke_action("yes".into());
    assert_eq!(
        shown(&import),
        (fixture["notices"][0].as_str().unwrap().to_owned(), false)
    );
    import.invoke_action("ok".into());
    assert!(slots.0.borrow().is_none(), "done: the window closes");
    assert_eq!(
        manager_state(&other),
        recorded_manager(&fixture["accepted"])
    );
    // again: only the nested downloader is new
    let import = windows::package(&other, &slots, true).unwrap();
    import.set_path(png.to_string_lossy().as_ref().into());
    import.invoke_action("open".into());
    import.invoke_action("accept".into());
    assert_eq!(
        shown(&import).0,
        fixture["import_questions"][2].as_str().unwrap()
    );
    import.invoke_action("yes".into());
    assert_eq!(shown(&import).0, fixture["notices"][1].as_str().unwrap());
    import.invoke_action("ok".into());
    assert_eq!(
        manager_state(&other),
        recorded_manager(&fixture["reopened"])
    );
}

/// The client's downloaders, URL classes, parsers and default downloader,
/// as `downloader_import_selection.json` lists them.
fn selection_state(store: &Store) -> Value {
    let downloaders: Downloaders = store.read(settings::get).unwrap();
    let classes: hydrus_core::url::UrlClassSettings = store.read(settings::get).unwrap();
    let defaults: hydrus_core::subscriptions::GalleryDefaults = store.read(settings::get).unwrap();
    serde_json::json!({
        "gugs": names(&downloaders.gugs.gugs, |g| g.name().to_owned()),
        "classes": names(&classes.url_classes, |c| c.name.clone()),
        "parsers": names(&downloaders.parsers, |p| p.name.clone()),
        "default_gug": defaults.gug.map(|(_, name)| name),
    })
}

// leaf: audit-network-exchange-unsupported
// leaf: audit-network-exchange-review
#[test]
fn objects_are_chosen_others_skipped_and_a_default_downloader_set_as_the_reference_does() {
    let _rendered = headless::init();
    let fixture = hydrus_testkit::fixture_json("downloader_import_selection.json");
    let (dir, store) = setup();
    assert_eq!(selection_state(&store), fixture["initial"]);
    let slots = windows::Slots::default();
    // "select objects from list" is on in advanced mode
    store
        .write(|ctx| settings::set(ctx.conn(), &settings::AdvancedMode(true)))
        .unwrap();
    let import = windows::package(&store, &slots, true).unwrap();
    assert!(import.get_select_from_list());
    import.invoke_action("cancel".into());
    store
        .write(|ctx| settings::set(ctx.conn(), &settings::AdvancedMode(false)))
        .unwrap();
    for stage in fixture["stages"].as_array().unwrap() {
        let name = stage["name"].as_str().unwrap();
        let source = dir.path().join(format!("{name}.json"));
        std::fs::write(&source, stage["source"].to_string()).unwrap();
        let import = windows::package(&store, &slots, true).unwrap();
        assert!(!import.get_select_from_list());
        import.set_select_from_list(stage["select"].as_bool().unwrap());
        import.set_path(source.to_string_lossy().as_ref().into());
        import.invoke_action("open".into());
        if let Some(warning) = stage["warnings"].as_array().unwrap().first() {
            // nothing downloader related: the warning is all it says
            assert_eq!(
                import.get_error().as_str(),
                warning.as_str().unwrap(),
                "{name}"
            );
            assert!(!import.get_ready());
            import.invoke_action("cancel".into());
            assert_eq!(selection_state(&store), stage["state"], "{name}");
            continue;
        }
        assert!(import.get_ready(), "{name}: {}", import.get_error());
        import.invoke_action("accept".into());
        for chooser in stage["choosers"].as_array().unwrap() {
            replay_chooser(&import, chooser);
        }
        let notices = stage["notices"].as_array().unwrap();
        if stage["questions"].as_array().unwrap().is_empty() {
            // (everything already there: the one notice says so; or the chooser was cancelled)
            match notices.first() {
                Some(notice) => {
                    assert_eq!(
                        shown(&import),
                        (notice.as_str().unwrap().to_owned(), false),
                        "{name}"
                    );
                    import.invoke_action("ok".into());
                }
                None => assert_eq!(shown(&import).0, "", "{name}: nothing asked"),
            }
            import.invoke_action("cancel".into());
            assert_eq!(selection_state(&store), stage["state"], "{name}");
            continue;
        }
        let (done, details) = notices.split_last().unwrap_or((&Value::Null, &[]));
        for detail in details {
            assert_eq!(
                shown(&import),
                (detail.as_str().unwrap().to_owned(), false),
                "{name}"
            );
            import.invoke_action("ok".into());
        }
        assert_eq!(
            shown(&import),
            (stage["questions"][0].as_str().unwrap().to_owned(), true),
            "{name}"
        );
        if !stage["yes"].as_bool().unwrap() {
            // "no": back to the review, nothing added
            import.invoke_action("no".into());
            assert_eq!(selection_state(&store), stage["state"], "{name}");
            import.invoke_action("cancel".into());
            continue;
        }
        import.invoke_action("yes".into());
        assert_eq!(
            shown(&import),
            (done.as_str().unwrap().to_owned(), false),
            "{name}"
        );
        import.invoke_action("ok".into());
        assert!(slots.0.borrow().is_none(), "{name}: closed");
        assert_eq!(selection_state(&store), stage["state"], "{name}");
    }
}
