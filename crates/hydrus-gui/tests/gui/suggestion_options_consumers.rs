//! The tag suggestions page's related durations, concurrence threshold and
//! recent count, set through the Options window's own rows, reaching the
//! Manage Tags panels: what the related buttons ask for and list, and what
//! the recent panel shows. Replays `suggestion_options_consumers.json`, where
//! the reference's `TagSuggestionsPanel` rows were set and its
//! `SuggestedTagsPanel` built after each change.
use crate::options_gui_support::{Client, basic_store, row, show_page};
use hydrus_core::{HashId, Sha256, Tag};
use hydrus_store::related_tags::{Settings as Related, Weights};
use hydrus_store::settings::TagSuggestionSettings;
use slint::{ComponentHandle as _, Model as _};

const QUICK: &str = "Initial/Quick search duration (ms): ";
const MEDIUM: &str = "Medium search duration (ms): ";
const THOROUGH: &str = "Thorough search duration (ms): ";
const CONCURRENCE: &str = "Tag concurrence threshold %: ";
const RECENT: &str = "number of recent tags to show: ";

/// The tags listed (the drawn rows less their parent rows and decorators).
fn texts(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<String> {
    rows.iter()
        .filter(|r| !r.cells.row_data(0).unwrap().starts_with("    "))
        .map(|r| {
            let text = r.cells.row_data(0).unwrap().to_string();
            text.split(" (").next().unwrap().to_owned()
        })
        .collect()
}

/// The basic store with the recording's corpus on "my tags", its recent tags
/// pushed in order, and the recording's weights; returns the files by hash.
fn seeded(
    f: &serde_json::Value,
) -> (
    Vec<tempfile::TempDir>,
    std::sync::Arc<hydrus_store::Store>,
    Vec<HashId>,
) {
    let (dirs, store) = basic_store();
    let hashes: Vec<Sha256> = f["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().parse::<Sha256>().unwrap())
        .collect();
    let ids = store
        .read(|conn| hydrus_store::master::hash_ids(conn, &hashes))
        .unwrap();
    let files: Vec<HashId> = hashes.iter().map(|h| ids[h]).collect();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    for row in f["corpus"].as_array().unwrap() {
        let tag = Tag::new(row[0].as_str().unwrap()).unwrap();
        let with: Vec<HashId> = row[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| files[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();
        store
            .write_content(move |w| {
                let id = hydrus_store::master::intern_tag(w.conn(), &tag)?;
                w.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    id,
                    &with,
                )
            })
            .unwrap();
    }
    let pushed: Vec<String> = serde_json::from_value(f["pushed"].clone()).unwrap();
    store
        .write(move |ctx| {
            ctx.conn().execute("DELETE FROM recent_tags", [])?;
            for (i, tag) in pushed.iter().enumerate() {
                let id = hydrus_store::master::intern_tag(ctx.conn(), &Tag::new(tag).unwrap())?;
                ctx.conn().execute(
                    "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?)",
                    rusqlite::params![service, id, 1_000_000 + i64::try_from(i).unwrap()],
                )?;
            }
            hydrus_store::settings::set(
                ctx.conn(),
                &Related {
                    weights: Weights {
                        search: vec![(String::new(), 100), (":".into(), 100)],
                        result: vec![(String::new(), 100), (":".into(), 100)],
                    },
                    ..Related::default()
                },
            )?;
            hydrus_store::settings::set(
                ctx.conn(),
                &TagSuggestionSettings {
                    columns: true,
                    ..TagSuggestionSettings::default()
                },
            )
        })
        .unwrap();
    (dirs, store, files)
}

/// Each scenario's rows are set in Options (applied), then Manage Tags is
/// opened on the two files: the recent panel shows (or hides) as many tags
/// as the count says, decaying the rest as the reference's read does, and
/// each related button asks for the tags on the files with its duration's
/// time and lists what passes the concurrence threshold.
// leaf: audit-options-nested-tag-suggestions-tabs
#[test]
fn related_durations_concurrence_and_the_recent_count_reach_the_panels_as_the_recording() {
    let f = hydrus_testkit::fixture_json("suggestion_options_consumers.json");
    let (dirs, store, files) = seeded(&f);
    let client = Client::with(dirs, store);
    {
        // (the second file is not in "my files"; the recording took both)
        let page = client.bound.current.borrow();
        let mut page = page.borrow_mut();
        page.choose_location(hydrus_core::search::context::LocationContext::default());
        page.enter();
    }
    for scenario in f["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        let set = &scenario["set"];
        let options = client.open_options();
        show_page(&options, "tag suggestions");
        for (label, ms) in [QUICK, MEDIUM, THOROUGH]
            .iter()
            .zip(set["durations"].as_array().unwrap())
        {
            options.invoke_number_edited(
                row(&options, label).0,
                i32::try_from(ms.as_i64().unwrap()).unwrap(),
            );
        }
        options.invoke_number_edited(
            row(&options, CONCURRENCE).0,
            i32::try_from(set["concurrence"].as_i64().unwrap()).unwrap(),
        );
        if let Some(count) = set["recent"].as_i64() {
            options.invoke_none_toggled(row(&options, RECENT).0, false);
            options.invoke_number_edited(row(&options, RECENT).0, i32::try_from(count).unwrap());
        } else {
            options.invoke_none_toggled(row(&options, RECENT).0, true);
        }
        options.invoke_apply();

        client
            .bound
            .current
            .borrow()
            .borrow_mut()
            .select_files(&files[..2]);
        client.ui.invoke_manage_tags_selected();
        let m = client
            .bound
            .manage_tags
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let mine = m
            .get_service_names()
            .iter()
            .position(|n| n == "my tags")
            .unwrap();
        m.invoke_service_chosen(i32::try_from(mine).unwrap());
        assert_eq!(
            m.get_recent_tags_enabled(),
            scenario["recent_shown"].as_bool().unwrap(),
            "{name}: the recent panel"
        );
        if let Some(recent) = scenario["recent"].as_array() {
            let recent: Vec<String> = recent
                .iter()
                .map(|t| t.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(texts(&m.get_recent_tag_rows()), recent, "{name}: recent");
        }
        for (level, press) in ["quick", "medium", "thorough"].iter().enumerate() {
            let expected = &scenario["related"][press];
            let listed: Vec<String> = expected["listed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_str().unwrap().to_owned())
                .collect();
            let level = i32::try_from(level).unwrap();
            m.set_related_level(level);
            m.invoke_related_search(level);
            let asked = hydrus_gui::related_tags_worker::last_request().expect("a related search");
            let wanted: Vec<String> = expected["read"]["search_tags"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_str().unwrap().to_owned())
                .collect();
            let mut searched = asked.searches.clone();
            searched.sort();
            assert_eq!(searched, wanted, "{name} {press}: the tags searched");
            let seconds = expected["read"]["max_time_to_take"].as_f64().unwrap();
            assert_eq!(
                asked.max_ms.map(f64::from),
                Some((seconds * 1000.0).round()),
                "{name} {press}: the time given"
            );
            assert_eq!(
                i64::from(asked.concurrence_percent),
                scenario["stored"]["concurrence"].as_i64().unwrap(),
                "{name} {press}"
            );
            // the worker answers through the window's timer, then says what it searched
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let answered = |m: &hydrus_gui::ManageTagsWindow| {
                let status = m.get_related_status();
                !status.is_empty() && status != "searching\u{2026}"
            };
            while !answered(&m) && std::time::Instant::now() < deadline {
                slint::platform::update_timers_and_animations();
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(answered(&m), "{name} {press}: the search finished");
            assert_eq!(
                texts(&m.get_related_tag_rows()),
                listed,
                "{name} {press}: listed"
            );
        }
        m.invoke_cancel();
    }
}
