//! Manage tags' suggested tags (most used, related, recent) in every layout and
//! with every default page, and what activating the first tag of each adds,
//! replayed from the reference's `SuggestedTagsPanel` (file lookup scripts are
//! out of scope: a default page naming them falls back to the first page).
use crate::options_gui_support::basic_store;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind};
use hydrus_store::related_tags::{Settings as Related, Weights};
use hydrus_store::settings::{TagAutocompleteTabs, TagSuggestionSettings};
use slint::{ComponentHandle as _, Model as _};

// leaf: audit-media-tags-missing-suggestions
#[test]
fn most_used_related_and_recent_panels_follow_the_recording_in_every_layout_and_page() {
    let f = hydrus_testkit::fixture_json("suggested_tag_panels.json");
    let (_dirs, store) = basic_store();
    let files: Vec<hydrus_core::HashId> = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 6")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    // the corpus on "my tags"
    for row in f["corpus"].as_array().unwrap() {
        let tag = row[0].as_str().unwrap();
        let with: Vec<hydrus_core::HashId> = row[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| files[usize::try_from(i.as_u64().unwrap()).unwrap()])
            .collect();
        let mut model = hydrus_gui::manage_tags::ManageTags::new(store.clone(), with).unwrap();
        let i = model
            .service_names()
            .iter()
            .position(|n| n == "my tags")
            .unwrap();
        model.choose_service(i).unwrap();
        model.add_side_suggestions(&[tag.into()]).unwrap();
        model.apply().unwrap();
    }
    let most_used: Vec<String> = serde_json::from_value(f["most_used"].clone()).unwrap();
    let hex = key.to_hex();
    store
        .write(move |ctx| {
            let mut tabs: TagAutocompleteTabs = hydrus_store::settings::get(ctx.conn())?;
            tabs.most_used.insert(hex, most_used);
            hydrus_store::settings::set(ctx.conn(), &tabs)?;
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
            // (the corpus was entered through Manage tags, which remembers them as recent)
            ctx.conn().execute("DELETE FROM recent_tags", [])?;
            let recent = hydrus_core::Tag::new("parity:recent").unwrap();
            let tag = hydrus_store::master::intern_tag(ctx.conn(), &recent)?;
            ctx.conn().execute(
                "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?) ON CONFLICT(service_id,tag_id) DO UPDATE SET used_ms=excluded.used_ms",
                rusqlite::params![service, tag, hydrus_core::time::TimestampMs::now().0],
            )?;
            Ok(())
        })
        .unwrap();
    let _windows = hydrus_gui::headless::init();
    for case in f["cases"].as_array().unwrap() {
        let columns = case["layout"] == "columns";
        let page = case["default"].as_str().unwrap().to_owned();
        store
            .write(move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &TagSuggestionSettings {
                        width: 240,
                        columns,
                        default_page: page,
                        recent_limit: Some(20),
                    },
                )
            })
            .unwrap();
        let mut search = SearchPage::new(store.clone());
        search.choose_location(hydrus_core::search::context::LocationContext::default());
        search.enter();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(search));
        ui.show().unwrap();
        bound
            .current
            .borrow()
            .borrow_mut()
            .select_files(&files[..2]);
        ui.invoke_manage_tags_selected();
        let m = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        let mine = m
            .get_service_names()
            .iter()
            .position(|n| n == "my tags")
            .unwrap();
        m.invoke_service_chosen(i32::try_from(mine).unwrap());
        // the panels there are, and the page shown
        assert!(
            m.get_most_used_enabled()
                && m.get_related_tags_enabled()
                && m.get_recent_tags_enabled()
        );
        assert_eq!(
            case["labels"],
            serde_json::json!(["most used", "related", "recent"])
        );
        assert_eq!(m.get_suggested_columns(), columns, "{case}");
        if !columns {
            let shown = match m.get_suggested_page() {
                0 => "most used",
                2 => "related",
                _ => "recent",
            };
            assert_eq!(case["selected"], shown, "{case}");
        }
        let texts = |rows: slint::ModelRc<hydrus_gui::TableRow>| {
            rows.iter()
                .map(|r| {
                    let text = r.cells.row_data(0).unwrap().to_string();
                    text.split(" (").next().unwrap().to_owned()
                })
                .collect::<Vec<_>>()
        };
        // the related search settles through its worker
        for _ in 0..800 {
            slint::platform::update_timers_and_animations();
            if !texts(m.get_related_tag_rows()).is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let listed = |m: &hydrus_gui::ManageTagsWindow| {
            serde_json::json!({
                "favourites": texts(m.get_most_used_rows()),
                "related": texts(m.get_related_tag_rows()),
                "recent": texts(m.get_recent_tag_rows()),
            })
        };
        assert_eq!(listed(&m), case["lists"], "{case}");
        // activating the first tag of each panel adds it, and it leaves the list
        for (name, panel) in [("favourites", 0), ("related", 2), ("recent", 1)] {
            let tag = case["lists"][name][0].as_str().unwrap();
            m.invoke_side_activated(panel, 0);
            assert!(
                m.get_tags().iter().any(|r| r.text.starts_with(tag)),
                "{name}: {tag} is staged"
            );
            for _ in 0..40 {
                slint::platform::update_timers_and_animations();
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(listed(&m)[name], case["remaining"][name], "{name} {case}");
        }
        m.invoke_cancel();
    }
}

/// The recent panel reads the newest tags the option keeps and forgets the rest
/// for good, and Clear empties it after its question, as the reference's does.
// leaf: audit-media-tags-missing-suggestions
#[test]
fn the_recent_panel_decays_on_read_and_clears_as_the_recording() {
    let f = hydrus_testkit::fixture_json("recent_tags_decay.json");
    let (_dirs, store) = basic_store();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    store
        .write(move |ctx| {
            ctx.conn().execute("DELETE FROM recent_tags", [])?;
            for (i, tag) in f["pushed"].as_array().unwrap().iter().enumerate() {
                let tag = hydrus_core::Tag::new(tag.as_str().unwrap()).unwrap();
                let id = hydrus_store::master::intern_tag(ctx.conn(), &tag)?;
                ctx.conn().execute(
                    "INSERT INTO recent_tags(service_id,tag_id,used_ms) VALUES(?,?,?)",
                    rusqlite::params![service, id, 1_000 + i64::try_from(i).unwrap() * 50],
                )?;
            }
            Ok(())
        })
        .unwrap();
    let _windows = hydrus_gui::headless::init();
    let f = hydrus_testkit::fixture_json("recent_tags_decay.json");
    for step in f["steps"].as_array().unwrap() {
        let Some(count) = step["count"].as_u64() else {
            continue; // (a panel that is off shows nothing: only the read keeps 20)
        };
        store
            .write(move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &TagSuggestionSettings {
                        recent_limit: Some(usize::try_from(count).unwrap()),
                        default_page: "recent".into(),
                        ..TagSuggestionSettings::default()
                    },
                )
            })
            .unwrap();
        let mut search = SearchPage::new(store.clone());
        search.choose_location(hydrus_core::search::context::LocationContext::default());
        search.enter();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(search));
        ui.show().unwrap();
        let file: hydrus_core::HashId = store
            .read(|conn| {
                Ok(conn.query_row(
                    "SELECT hash_id FROM files ORDER BY hash_id LIMIT 1",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        bound.current.borrow().borrow_mut().select_files(&[file]);
        ui.invoke_manage_tags_selected();
        let m = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        let mine = m
            .get_service_names()
            .iter()
            .position(|n| n == "my tags")
            .unwrap();
        m.invoke_service_chosen(i32::try_from(mine).unwrap());
        if step["step"] == "after clear" {
            m.invoke_clear_recent();
            assert_eq!(m.get_tag_menu_question().as_str(), "Clear recent tags?");
            m.invoke_tag_menu_answered(true);
        }
        for _ in 0..40 {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let shown: Vec<String> = m
            .get_recent_tag_rows()
            .iter()
            .map(|r| r.cells.row_data(0).unwrap().to_string())
            .collect();
        assert_eq!(serde_json::json!(shown), step["tags"], "{step}");
        m.invoke_cancel();
    }
}
