//! Actual sorted thumbnail/collection prefixes and rendered sidebar notices.
use super::tag_dialog_preferences::fixture;
use hydrus_core::{
    HashId, ServiceKey,
    pages::{PageCollect, PageSort, PageSortBy},
    search::context::{FileSearchContext, LocationContext, TagContext},
    tag_presentation::TagPresentation,
};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;

const LABEL: &str = "Max number of thumbnails to compute tags for when none are selected: ";
fn options(ui: &MainWindow) {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "file")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let row = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label.starts_with("options"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
}
fn edit(window: &OptionsWindow, limit: &Value) {
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "tag presentation")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = window
        .get_rows()
        .iter()
        .position(|row| row.label == LABEL)
        .unwrap();
    window.invoke_none_toggled(i32::try_from(row).unwrap(), limit.is_null());
    if let Some(limit) = limit.as_i64() {
        window.invoke_number_edited(i32::try_from(row).unwrap(), i32::try_from(limit).unwrap());
    }
}
fn profile(store: Arc<Store>, files: Vec<HashId>, case: &Value) -> SearchPage {
    let snapshot = store.snapshot();
    let tags = snapshot
        .services
        .by_name(case["service"].as_str().unwrap())
        .unwrap()
        .key
        .clone();
    let sort = PageSort {
        by: PageSortBy::System(20),
        ascending: case["ascending"].as_bool().unwrap(),
    };
    let mut page = SearchPage::restored(
        store,
        FileSearchContext {
            location: LocationContext::single(ServiceKey::new(
                hydrus_core::service::builtin_keys::MY_FILES,
            )),
            tags: TagContext::new(tags, true, true),
            predicates: Vec::new(),
        },
        false,
        Some(&sort),
        files,
    );
    page.set_collect(PageCollect {
        namespaces: if case["collected"] == true {
            vec!["cap_group".into()]
        } else {
            Vec::new()
        },
        collect_unmatched: false,
        ..PageCollect::default()
    });
    match case["selection"].as_str().unwrap() {
        "none" => {}
        "last" => page.select(page.results().len() - 1),
        "all" => page.select_all(),
        other => panic!("unknown selection {other}"),
    }
    page
}
fn hash_groups(page: &SearchPage) -> Value {
    let groups: Vec<Vec<HashId>> = page
        .results()
        .iter()
        .map(|item| page.files_of(*item))
        .collect();
    let hashes = page
        .store()
        .read(|conn| hydrus_store::master::hashes(conn, &page.files()))
        .unwrap();
    json!(
        groups
            .into_iter()
            .map(|group| {
                let mut hashes: Vec<_> =
                    group.iter().map(|file| hashes[file].to_string()).collect();
                hashes.sort();
                hashes
            })
            .collect::<Vec<_>>()
    )
}
fn rows(page: &SearchPage) -> Value {
    json!(
        page.tag_rows()
            .into_iter()
            .filter(|row| row.starts_with("parity:cap"))
            .collect::<Vec<_>>()
    )
}

#[test]
fn real_options_apply_cancel_reopen_counts_sorted_items_and_uncapped_selection() {
    let recorded = hydrus_testkit::fixture_json("unselected_tag_cap.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(profile(store.clone(), files.clone(), &recorded["cases"][0])),
    );
    let before: TagPresentation = store.read(settings::get).unwrap();
    options(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    edit(&window, &recorded["options"]["applied"]);
    assert_eq!(
        store.read::<TagPresentation>(settings::get).unwrap(),
        before
    );
    window.invoke_cancel();
    assert_eq!(
        store.read::<TagPresentation>(settings::get).unwrap(),
        before
    );
    for case in recorded["cases"].as_array().unwrap() {
        let current = bound.pages.borrow_mut().current();
        *current.borrow_mut() = profile(store.clone(), files.clone(), case);
        match case["selection"].as_str().unwrap() {
            "last" => {
                let last = current.borrow().results().len() - 1;
                ui.invoke_thumbnail_clicked(i32::try_from(last).unwrap(), false, false);
            }
            "all" => ui.invoke_select_all(),
            "none" => {}
            other => panic!("unknown selection {other}"),
        }
        options(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        let prior: TagPresentation = store.read(settings::get).unwrap();
        edit(&window, &case["limit"]);
        assert_eq!(store.read::<TagPresentation>(settings::get).unwrap(), prior);
        window.invoke_apply();
        let page = current.borrow();
        assert_eq!(hash_groups(&page), case["items"]);
        assert_eq!(rows(&page), case["rows"]);
        assert_eq!(
            page.tag_computation_limit().is_some(),
            case["capped"].as_bool().unwrap()
        );
        assert_eq!(page.tag_list_title(), case["title"]);
        assert_eq!(
            ui.get_selection_tags_title(),
            case["title"].as_str().unwrap()
        );
        let actual: Vec<String> = ui
            .get_tags()
            .iter()
            .filter(|row| row.text.starts_with("parity:cap"))
            .map(|row| row.text.to_string())
            .collect();
        assert_eq!(json!(actual), case["rows"]);
        // One capped thumbnail can contribute multiple files from a collection.
        if case["collected"] == true
            && case["limit"] == 1
            && case["ascending"] == true
            && case["selection"] == "none"
        {
            assert_eq!(actual.len(), 2);
            let pixels = headless::render(&windows.get(0).unwrap(), 1200, 900);
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
            assert_eq!(
                ui.get_selection_tags_title(),
                case["title"].as_str().unwrap()
            );
        }
        drop(page);
        options(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        edit(&window, &json!(999));
        window.invoke_cancel();
        let saved: TagPresentation = store.read(settings::get).unwrap();
        assert_eq!(
            serde_json::to_value(saved.unselected_tag_limit).unwrap(),
            case["limit"]
        );
        let mut reopened = profile(store.clone(), files.clone(), case);
        reopened.refresh_tags();
        assert_eq!(rows(&reopened), case["rows"]);
        assert_eq!(reopened.tag_list_title(), case["title"]);
    }
    // A background page's cached prefix is refreshed when it is activated again.
    let background = bound.pages.borrow_mut().current();
    *background.borrow_mut() = profile(store.clone(), files.clone(), &recorded["cases"][2]);
    assert_eq!(background.borrow().tag_computation_limit(), Some(3));
    bound
        .pages
        .borrow_mut()
        .new_page(&hydrus_gui::page_chooser::NewPage::Search {
            domain: ServiceKey::new(hydrus_core::service::builtin_keys::MY_FILES),
            name: "second cap page".into(),
        })
        .unwrap();
    let other = bound.pages.borrow_mut().current();
    *other.borrow_mut() = profile(store.clone(), files.clone(), &recorded["cases"][2]);
    ui.invoke_tab_chosen(0, 1);
    options(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    edit(&window, &json!(1));
    window.invoke_apply();
    assert_eq!(background.borrow().tag_computation_limit(), Some(3));
    ui.invoke_tab_chosen(0, 0);
    assert_eq!(rows(&background.borrow()), recorded["cases"][2]["rows"]);
    assert_eq!(
        ui.get_selection_tags_title(),
        recorded["cases"][2]["title"].as_str().unwrap()
    );
    let pixels = headless::render(&windows.get(0).unwrap(), 1200, 900);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
}

#[test]
fn changing_sort_recomputes_the_capped_prefix() {
    let recorded = hydrus_testkit::fixture_json("unselected_tag_cap.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    store
        .write(|ctx| {
            let mut presentation: TagPresentation = settings::get(ctx.conn())?;
            presentation.unselected_tag_limit = Some(1);
            settings::set(ctx.conn(), &presentation)
        })
        .unwrap();
    let mut page = profile(store, files, &recorded["cases"][2]);
    assert_eq!(rows(&page), recorded["cases"][2]["rows"]);
    page.set_sort_order(hydrus_search::SortOrder::Descending);
    assert_eq!(rows(&page), recorded["cases"][3]["rows"]);
    page.select_all();
    assert_eq!(page.tag_computation_limit(), None);
}
