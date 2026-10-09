//! Tag-service tabs of Manage tags and the siblings/parents dialogs, replayed
//! from `oracle/record_tag_service_tabs.py` on the `repositories` fixture.
//! hydrus-rs's Manage tags has no repository tabs (`DIFFERENCES.md`), so its
//! replay keeps to the local services' tabs.
use std::sync::Arc;

use crate::options_gui_support::Client;
use hydrus_core::{HashId, Sha256, Tag};
use hydrus_store::Store;
use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

fn repositories() -> Client {
    let legacy = hydrus_testkit::legacy_fixture("repositories");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    Client::with(vec![legacy, native], store)
}

fn files(store: &Arc<Store>) -> Vec<HashId> {
    let manifest = hydrus_testkit::fixture_json("legacy_db/repositories.manifest.json");
    let hashes: Vec<Sha256> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["hash"].as_str().unwrap().parse().unwrap())
        .collect();
    store
        .read(|conn| {
            hashes
                .iter()
                .map(|h| hydrus_store::master::hash_id(conn, h).map(Option::unwrap))
                .collect()
        })
        .unwrap()
}

fn set_default(client: &Client, name: &str, remember: bool) {
    let snapshot = client.store.snapshot();
    let key = snapshot.services.by_name(name).map_or_else(
        // a key that is no tab, as the reference's "all known tags" is
        || hydrus_core::ServiceKey::new(b"all known tags".to_vec()),
        |s| s.key.clone(),
    );
    client
        .store
        .write(move |ctx| {
            let mut o: hydrus_store::tag_editing::TagEditingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            o.default_service = key;
            o.remember_service = remember;
            hydrus_store::settings::set(ctx.conn(), &o)
        })
        .unwrap();
}

fn default_name(client: &Client) -> String {
    let o: hydrus_store::tag_editing::TagEditingSettings = client.setting();
    client
        .store
        .snapshot()
        .services
        .by_key(&o.default_service)
        .map_or_else(
            |_| String::from_utf8_lossy(o.default_service.as_bytes()).into_owned(),
            |s| s.name.clone(),
        )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, b| {
        out.push_str(&format!("{b:02x}"));
        out
    })
}

/// The recorded default: a service's name, or the hex of a key that names
/// none (the reference's "all known tags" key, which is no tab).
fn recorded(name: &Value) -> String {
    let name = name.as_str().unwrap();
    if name == hex(b"all known tags") {
        "all known tags".to_owned()
    } else {
        name.to_owned()
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn labels(w: &hydrus_gui::ManageTagsWindow) -> Vec<String> {
    w.get_service_labels()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// The recorded tab labels hydrus-rs has: those of the local services.
fn local(recorded: &Value) -> Vec<String> {
    strings(recorded)
        .into_iter()
        .filter(|t| !t.starts_with("a tag repository"))
        .collect()
}

fn select(client: &Client, all: &[HashId], indices: &[usize]) {
    let results = client.bound.current.borrow().borrow().results().to_vec();
    for (n, i) in indices.iter().enumerate() {
        let at = results
            .iter()
            .position(|f| *f == all[*i])
            .unwrap_or_else(|| {
                panic!(
                    "file {i} of {} not among {} results",
                    all.len(),
                    results.len()
                )
            });
        client
            .ui
            .invoke_thumbnail_clicked(i32::try_from(at).unwrap(), n > 0, false);
    }
}

/// Every local file: the fixture's files are not all in "my files".
fn search_everything(client: &Client) {
    let key = client
        .store
        .snapshot()
        .services
        .by_name("hydrus local file storage")
        .unwrap()
        .key
        .clone();
    client
        .bound
        .current
        .borrow()
        .borrow_mut()
        .choose_location(hydrus_search::LocationContext::single(key));
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
}

fn manage(client: &Client) -> hydrus_gui::ManageTagsWindow {
    client.ui.invoke_manage_tags_selected();
    client
        .bound
        .manage_tags
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

fn relationships(client: &Client, kind: &str) -> hydrus_gui::TagRelationshipsWindow {
    let ui = &client.ui;
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let at = pane
        .lines
        .iter()
        .position(|l| l.label.starts_with(kind))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    client
        .bound
        .tag_relationships
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong()
}

fn seed(client: &Client, recording: &Value, all: &[HashId]) {
    let snapshot = client.store.snapshot();
    for m in recording["mappings"].as_array().unwrap() {
        let service = snapshot
            .services
            .by_name(m[0].as_str().unwrap())
            .unwrap()
            .id;
        let tag = Tag::new(m[1].as_str().unwrap()).unwrap();
        let files: Vec<HashId> = m[2]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| all[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();
        client
            .store
            .write_content(move |w| {
                let tag = hydrus_store::master::intern_tag(w.conn(), &tag)?;
                w.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &files,
                )
            })
            .unwrap();
    }
}

// leaf: audit-media-tags-service
#[test]
fn manage_tags_tabs_count_tags_mark_changes_keep_staging_and_follow_the_viewer() {
    let recording = hydrus_testkit::fixture_json("tag_service_tabs.json");
    let client = repositories();
    let all = files(&client.store);
    seed(&client, &recording, &all);
    let ui = &client.ui;
    search_everything(&client);
    set_default(&client, "my tags", false);
    for case in recording["manage_tags_opened"]
        .as_object()
        .unwrap()
        .values()
    {
        let indices: Vec<usize> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| usize::try_from(i.as_u64().unwrap()).unwrap())
            .collect();
        select(&client, &all, &indices);
        let w = manage(&client);
        assert_eq!(labels(&w), local(&case["tabs"]), "{case}");
        w.invoke_cancel();
    }
    // staging on one tab survives using another; the tabs count and mark it
    select(&client, &all, &[0]);
    let w = manage(&client);
    let names: Vec<String> = w
        .get_service_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let tab = |name: &str| i32::try_from(names.iter().position(|n| n == name).unwrap()).unwrap();
    let steps = recording["manage_tags_staging"].as_array().unwrap();
    let check = |step: &Value| {
        assert_eq!(labels(&w), local(&step["tabs"]), "{}", step["step"]);
        let current = strings(&step["tabs"])
            [usize::try_from(step["current"].as_u64().unwrap()).unwrap()]
        .clone();
        assert!(
            current.starts_with(&names[usize::try_from(w.get_service_index()).unwrap()]),
            "{}",
            step["step"]
        );
    };
    check(&steps[0]);
    w.invoke_text_edited("tabs:staged mine".into());
    w.invoke_entered();
    check(&steps[1]);
    w.invoke_service_chosen(tab("downloader tags"));
    w.invoke_text_edited("tabs:staged down".into());
    w.invoke_entered();
    check(&steps[2]);
    w.invoke_service_chosen(tab("my tags"));
    check(&steps[3]);
    // what each tab staged is still staged on it after the other was used
    let rows: Vec<String> = w.get_tags().iter().map(|r| r.text.to_string()).collect();
    assert!(
        rows.iter().any(|r| r.starts_with("tabs:staged mine")),
        "{rows:?}"
    );
    w.invoke_service_chosen(tab("downloader tags"));
    let rows: Vec<String> = w.get_tags().iter().map(|r| r.text.to_string()).collect();
    assert!(
        rows.iter().any(|r| r.starts_with("tabs:staged down")),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|r| r.starts_with("tabs:staged mine")),
        "{rows:?}"
    );
    w.invoke_cancel();
    // from the viewer (changes written at once), moving on to other files
    let viewer_case = &recording["manage_tags_viewer"];
    let results = client.bound.current.borrow().borrow().results().to_vec();
    let at = |i: usize| results.iter().position(|f| *f == all[i]).unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(at(0)).unwrap());
    let viewer = client
        .bound
        .viewer
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    viewer.invoke_manage_tags();
    let w = client
        .bound
        .manage_tags
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(labels(&w), local(&viewer_case["opened"]));
    let mut shown = at(0);
    for (file, key) in [
        (1, "next file"),
        (
            recording["untagged_file"].as_u64().unwrap() as usize,
            "untouched file",
        ),
    ] {
        while shown != at(file) {
            w.invoke_show_next();
            shown = (shown + 1) % results.len();
        }
        assert_eq!(labels(&w), local(&viewer_case[key]), "{key}");
    }
    w.invoke_cancel();
    viewer.invoke_close_requested();
}

/// A window with tag-service tabs.
trait Tabs {
    fn names(&self) -> Vec<String>;
    fn index(&self) -> i32;
    fn choose(&self, i: i32);
    fn close(&self);
}

impl Tabs for hydrus_gui::TagRelationshipsWindow {
    fn names(&self) -> Vec<String> {
        self.get_service_names()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
    fn index(&self) -> i32 {
        self.get_service_index()
    }
    fn choose(&self, i: i32) {
        self.invoke_service_chosen(i);
    }
    fn close(&self) {
        self.invoke_cancel();
    }
}

impl Tabs for hydrus_gui::ManageTagsWindow {
    fn names(&self) -> Vec<String> {
        self.get_service_names()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
    fn index(&self) -> i32 {
        self.get_service_index()
    }
    fn choose(&self, i: i32) {
        self.invoke_service_chosen(i);
    }
    fn close(&self) {
        self.invoke_cancel();
    }
}

fn replay_defaults<W: Tabs>(client: &Client, cases: &Value, tabs: &[String], open: impl Fn() -> W) {
    for case in cases.as_array().unwrap() {
        let default = case["default"].as_str().unwrap();
        set_default(client, default, case["save_on_change"].as_bool().unwrap());
        let w = open();
        let native = w.names();
        let opened = &tabs[usize::try_from(case["opened"].as_u64().unwrap()).unwrap()];
        // hydrus-rs opens on the first tab where the reference opens one it
        // lacks; the tab changes after that differ, so stop there
        let Some(expected) = native.iter().position(|n| n == opened) else {
            assert_eq!(w.index(), 0, "{case}");
            w.close();
            continue;
        };
        assert_eq!(w.index(), i32::try_from(expected).unwrap(), "{case}");
        assert_eq!(
            default_name(client),
            recorded(&case["default_after_open"]),
            "{case}"
        );
        for chosen in case["chosen"].as_array().unwrap() {
            let name = &tabs[usize::try_from(chosen["index"].as_u64().unwrap()).unwrap()];
            // (a tab hydrus-rs lacks: what follows differs, so stop there)
            let Some(at) = native.iter().position(|n| n == name) else {
                break;
            };
            w.choose(i32::try_from(at).unwrap());
            assert_eq!(w.index(), i32::try_from(at).unwrap(), "{case}");
            assert_eq!(default_name(client), recorded(&chosen["default"]), "{case}");
        }
        w.close();
    }
}

// leaf: audit-media-relationships-siblings-default-service
// leaf: audit-media-relationships-parents-default-service
#[test]
fn relationship_dialogs_open_on_the_default_tab_and_save_it_on_change_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("tag_service_tabs.json");
    let client = repositories();
    for kind in ["siblings", "parents"] {
        let tabs = strings(&recording[format!("{kind}_tabs")]);
        set_default(&client, "my tags", false);
        let w = relationships(&client, kind);
        let names: Vec<String> = w
            .get_service_names()
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(names, tabs);
        w.invoke_cancel();
        replay_defaults(
            &client,
            &recording[format!("{kind}_defaults")],
            &tabs,
            || relationships(&client, kind),
        );
    }
    // Manage tags shares the preference (its local tabs)
    search_everything(&client);
    let tabs: Vec<String> = strings(&recording["manage_tags_opened"]["one"]["tabs"])
        .into_iter()
        .map(|t| t.split(" (").next().unwrap().to_owned())
        .collect();
    let all = files(&client.store);
    replay_defaults(&client, &recording["manage_tags_defaults"], &tabs, || {
        select(&client, &all, &[0]);
        manage(&client)
    });
}
