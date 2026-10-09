//! The suggested tags' related panel in Manage tags: its three duration
//! buttons, the status line, and a search from the selected tags, replayed from
//! the reference's `RelatedTagsPanel`.
use crate::options_gui_support::basic_store;
use hydrus_gui::{MainWindow, Pages, bind};
use hydrus_store::related_tags::{Settings as Related, Weights};
use slint::{ComponentHandle as _, Model as _};

#[test]
fn related_buttons_search_from_the_selected_tags_within_the_time_as_the_reference() {
    let f = hydrus_testkit::fixture_json("related_tags_panel.json");
    let (_dirs, store) = basic_store();
    let files: Vec<hydrus_core::HashId> = store
        .read(|conn| {
            Ok(conn
                .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 6")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap();
    for (tag, files_with) in f["corpus"].as_array().unwrap().iter().map(|c| {
        (
            c[0].as_str().unwrap(),
            c[1].as_array()
                .unwrap()
                .iter()
                .map(|i| files[usize::try_from(i.as_u64().unwrap()).unwrap()])
                .collect::<Vec<_>>(),
        )
    }) {
        let mut model =
            hydrus_gui::manage_tags::ManageTags::new(store.clone(), files_with).unwrap();
        let i = model
            .service_names()
            .iter()
            .position(|n| n == f["service"].as_str().unwrap())
            .unwrap();
        model.choose_service(i).unwrap();
        model.add_side_suggestions(&[tag.into()]).unwrap();
        model.apply().unwrap();
    }
    let weights = Weights {
        search: serde_json::from_value(f["weights"][0].clone()).unwrap(),
        result: serde_json::from_value(f["weights"][1].clone()).unwrap(),
    };
    let set = |first_ms: u32| {
        let settings = Related {
            weights: weights.clone(),
            durations_ms: [first_ms, 2_000, 6_000],
            ..Related::default()
        };
        store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings))
            .unwrap();
    };
    set(250);
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::settings::TagSuggestionSettings {
                    default_page: "related".into(),
                    recent_limit: None,
                    ..hydrus_store::settings::TagSuggestionSettings::default()
                },
            )
        })
        .unwrap();
    let _windows = hydrus_gui::headless::init();
    let mut page = hydrus_gui::SearchPage::new(store.clone());
    page.choose_location(hydrus_core::search::context::LocationContext::default());
    page.enter();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(page));
    ui.show().unwrap();
    bound
        .current
        .borrow()
        .borrow_mut()
        .select_files(&files[..2]);
    ui.invoke_manage_tags_selected();
    let m = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let second = m
        .get_service_names()
        .iter()
        .position(|n| n == f["service"].as_str().unwrap())
        .unwrap();
    m.invoke_service_chosen(i32::try_from(second).unwrap());
    // the buttons' and toggles' tooltips
    assert_eq!(
        m.get_related_button_tooltip(),
        f["buttons"]["quick"].as_str().unwrap()
    );
    let unwrapped = |s: &str| s.replace('\n', " ");
    assert_eq!(
        m.get_related_local_tooltip(),
        unwrapped(f["toggles"]["local"].as_str().unwrap())
    );
    assert_eq!(
        m.get_related_display_tooltip(),
        unwrapped(f["toggles"]["display"].as_str().unwrap())
    );
    let listed = || {
        m.get_related_tag_rows()
            .iter()
            .map(|row| {
                let text = row.cells.row_data(0).unwrap().to_string();
                text.split(" (").next().unwrap().to_owned()
            })
            .collect::<Vec<_>>()
    };
    let status = || {
        let s = m.get_related_status().to_string();
        s.split(" in ").next().unwrap().to_owned()
    };
    let settle = |expected: &str| {
        for _ in 0..800 {
            slint::platform::update_timers_and_animations();
            if status() == expected {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!(
            "{expected}: {} {:?} tags {:?}",
            m.get_related_status(),
            listed(),
            m.get_tags()
                .iter()
                .map(|r| r.text.to_string())
                .collect::<Vec<_>>()
        );
    };
    let step = |name: &str| {
        f["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["action"] == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    let check = |name: &str| {
        let s = step(name);
        settle(s["status"].as_str().unwrap());
        assert_eq!(serde_json::json!(listed()), s["listed"], "{name}");
    };
    // opening searches at once, as quick
    check("quick");
    m.invoke_related_search(1);
    check("medium");
    m.invoke_related_search(2);
    check("thorough");
    // a tag selected: only it is searched from, and the others are not suggested
    let row = m
        .get_tags()
        .iter()
        .position(|r| r.text.starts_with("alpha:first"))
        .unwrap();
    m.invoke_tag_clicked(i32::try_from(row).unwrap(), false, false);
    m.invoke_related_search(0);
    check("selected alpha:first");
    m.invoke_tag_clicked(i32::try_from(row).unwrap(), true, false);
    m.invoke_related_search(0);
    check("selection cleared");
    // the toggles search again, with the same buttons' duration
    m.set_related_local(false);
    m.invoke_related_search(0);
    check("all known files");
    m.set_related_local(true);
    m.set_related_display(false);
    m.invoke_related_search(0);
    check("storage tags");
    m.set_related_display(true);
    // no time at all: no search tag is done
    set(0);
    m.invoke_related_search(0);
    check("no time at all");
}
