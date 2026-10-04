//! Favourite searches: the fixture's comes across from hydrus, shows in
//! the star button's menu, and loading it sets the page's search, sort and
//! collect; and saving and managing them, against the reference's
//! (`oracle/record_favourite_searches.py`, on the `basic` fixture): the
//! menu, the manage dialog's list, the names it gives searches it adds,
//! the edit dialog's question before overwriting another, and what it
//! gives back.

use slint::Model as _;

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

#[test]
fn a_favourite_search_loads_into_the_page() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let favourites: hydrus_store::settings::FavouriteSearches =
        store.read(hydrus_store::settings::get).unwrap();
    let favourite = favourites.0[0].clone();

    // loaded directly: the same files as searching for its predicates
    let mut page = SearchPage::new(store.clone());
    page.load_favourite(&favourite);
    let loaded = page.predicates();
    assert_eq!(loaded.len(), 3, "{loaded:?}");
    assert!(loaded.contains(&"system:inbox".to_owned()), "{loaded:?}");
    let found = page.results().to_vec();
    assert!(!found.is_empty());
    let mut by_hand = SearchPage::new(store.clone());
    for p in &loaded {
        by_hand.add_predicate(p);
    }
    by_hand.set_sort_by(page.file_sort().unwrap().by);
    by_hand.set_sort_order(page.file_sort().unwrap().order);
    assert_eq!(by_hand.results(), found);
    // its sort: largest file first
    assert_eq!(
        page.file_sort().unwrap().by,
        hydrus_search::SortBy::FileSize
    );
    assert_eq!(
        page.file_sort().unwrap().order,
        hydrus_search::SortOrder::Descending
    );

    // in the window: the star button's menu, its folder opened, and the
    // search loaded from it
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store)));
    assert!(ui.get_can_favourite());
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    assert_eq!(
        pane_lines(&ui, 0),
        [
            "manage favourite searches",
            "---",
            "save this search",
            "---",
            "example search >"
        ]
    );
    ui.invoke_menu_line_hovered(0, 4, 200.0, 100.0, 10.0);
    assert_eq!(pane_lines(&ui, 1), ["inbox filter"]);
    ui.invoke_menu_line_clicked(1, 0, 0.0, 0.0, 0.0);
    assert_eq!(ui.get_menu_panes().row_count(), 0, "closed");
    let shown: Vec<String> = (0..ui.get_predicates().row_count())
        .map(|i| ui.get_predicates().row_data(i).unwrap().text.to_string())
        .collect();
    assert_eq!(shown, loaded);
    assert!(
        ui.get_status()
            .starts_with(&format!("{} files - totalling ", found.len())),
        "{}",
        ui.get_status()
    );
}

/// The lines of open menu `pane`: a separator as "---", a submenu with " >".
fn pane_lines(ui: &MainWindow, pane: usize) -> Vec<String> {
    let lines = ui
        .get_menu_panes()
        .row_data(pane)
        .expect("a menu open")
        .lines;
    (0..lines.row_count())
        .map(|i| {
            let line = lines.row_data(i).unwrap();
            match line.kind {
                2 => "---".to_owned(),
                3 => format!("{} >", line.label),
                _ => line.label.to_string(),
            }
        })
        .collect()
}

#[test]
fn a_favourite_s_collect_collects_the_page() {
    use hydrus_core::pages::PageCollect;

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let favourites: hydrus_store::settings::FavouriteSearches =
        store.read(hydrus_store::settings::get).unwrap();
    // hydrus's example search has no collect: the page's stays
    assert_eq!(favourites.0[0].collect, None);
    let mut favourite = favourites.0[0].clone();
    let mut page = SearchPage::new(store.clone());
    page.load_favourite(&favourite);
    assert!(!page.collect().collects());
    let files = page.files().len();
    assert!(files > 1);

    // one collecting by creator collects what it finds
    let by_creator = PageCollect {
        namespaces: vec!["creator".into()],
        ratings: Vec::new(),
        collect_unmatched: true,
        tag_context: hydrus_core::search::context::TagContext::default(),
    };
    favourite.collect = Some(by_creator.clone());
    page.load_favourite(&favourite);
    assert_eq!(page.collect(), &by_creator);
    assert!(page.results().len() < files);
    assert_eq!(page.files().len(), files);
    // and one collecting nothing uncollects the page, though it doesn't
    // search: its files stay, single
    favourite.synchronised = false;
    favourite.collect = Some(PageCollect::default());
    page.load_favourite(&favourite);
    assert!(!page.collect().collects());
    assert_eq!(page.results().len(), files);
}

// saving and managing them, against the reference's

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value as Json, json};

use hydrus_core::ServiceKey;
use hydrus_core::pages::{FavouriteSearch, PageCollect, PageSort, PageSortBy, SortSettings};
use hydrus_gui::favourites::{self, Describe, Edit, Manager};
use hydrus_gui::main_menu::Entry;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
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

fn recorded() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/favourite_searches.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn fixture_favourites(store: &Store) -> Vec<FavouriteSearch> {
    let favourites: hydrus_store::settings::FavouriteSearches =
        store.read(hydrus_store::settings::get).unwrap();
    favourites.0
}

/// A menu as the recording writes it.
fn menu_json(entries: &[Entry]) -> Json {
    Json::Array(
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Item { label, .. } | Entry::Check { label, .. } => json!({ "text": label }),
                Entry::Menu { label, entries, .. } => {
                    json!({ "text": label, "submenu": menu_json(entries) })
                }
                Entry::Separator => json!("---"),
            })
            .collect(),
    )
}

fn service_key(store: &Store, name: &str) -> ServiceKey {
    store
        .snapshot()
        .services
        .all()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no {name}"))
        .key
        .clone()
}

/// The recording's favourites: each searching the fixture's favourite's
/// search, with its sort and collect.
fn recorded_rows(store: &Store, recorded: &Json) -> Vec<FavouriteSearch> {
    let search = fixture_favourites(store)[0].search.clone();
    recorded["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let sort = match &row[2] {
                Json::Null => None,
                spec => {
                    let by = match spec[0].as_str().unwrap() {
                        "system" => PageSortBy::System(match spec[1].as_str().unwrap() {
                            "SORT_FILES_BY_IMPORT_TIME" => 2,
                            "SORT_FILES_BY_WIDTH" => 5,
                            other => panic!("{other}"),
                        }),
                        "namespaces" => PageSortBy::Namespaces {
                            namespaces: strings(&spec[1]),
                            tag_display_type: 1,
                        },
                        _ => PageSortBy::Rating(service_key(store, spec[1].as_str().unwrap())),
                    };
                    Some(PageSort {
                        by,
                        ascending: spec[2].as_bool().unwrap(),
                    })
                }
            };
            let collect = match &row[3] {
                Json::Null => None,
                spec => Some(PageCollect {
                    namespaces: strings(&spec[0]),
                    ratings: strings(&spec[1])
                        .iter()
                        .map(|n| service_key(store, n))
                        .collect(),
                    ..PageCollect::default()
                }),
            };
            FavouriteSearch {
                folder: row[0].as_str().map(str::to_owned),
                name: row[1].as_str().unwrap().to_owned(),
                search: search.clone(),
                synchronised: true,
                sort,
                collect,
            }
        })
        .collect()
}

fn strings(json: &Json) -> Vec<String> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

/// How the list writes searches, from the store, as the window does.
struct Words {
    store: Arc<Store>,
    text: hydrus_search::TextContext,
}

impl Words {
    fn new(store: &Arc<Store>) -> Self {
        let viewing = store.read(hydrus_store::settings::get).unwrap_or_default();
        let text = hydrus_search::TextContext::from_store(&store.snapshot().services, &viewing);
        Self {
            store: store.clone(),
            text,
        }
    }

    fn predicates(&self, favourite: &FavouriteSearch) -> Vec<String> {
        favourite
            .search
            .predicates
            .iter()
            .map(|p| hydrus_search::predicate_text(p, &self.text))
            .collect()
    }

    fn collect(&self, collect: &PageCollect) -> String {
        let snapshot = self.store.snapshot();
        favourites::collect_text(collect, &|key| {
            snapshot.services.by_key(key).ok().map(|s| s.name.clone())
        })
    }

    fn rows(&self, rows: &[FavouriteSearch]) -> Vec<[String; 5]> {
        let describe = Describe {
            predicates: &|f| self.predicates(f),
            sort: &|s| hydrus_gui::sort::sort_text(&self.store, s),
            collect: &|c| self.collect(c),
        };
        rows.iter()
            .map(|f| favourites::display_row(f, &describe))
            .collect()
    }

    /// The list as shown: sorted on the folder, ascending.
    fn list(&self, rows: &[FavouriteSearch]) -> Vec<[String; 5]> {
        let rows = self.rows(rows);
        favourites::list_order(&rows, 0, true)
            .into_iter()
            .map(|i| rows[i].clone())
            .collect()
    }

    /// A search as the recording describes it.
    fn describe(&self, favourite: &FavouriteSearch) -> Json {
        json!({
            "folder": favourite.folder,
            "name": favourite.name,
            "location": hydrus_gui::domains::location_label(
                &self.store.snapshot().services,
                &favourite.search.location
            ),
            "predicates": self.predicates(favourite),
            "synchronised": favourite.synchronised,
            "sort": favourite.sort.as_ref().map(|s| hydrus_gui::sort::sort_text(&self.store, s)),
            "collect": favourite.collect.as_ref().map(|c| self.collect(c)),
        })
    }
}

#[test]
fn the_star_button_s_menu_is_the_reference_s() {
    let (_dirs, store) = store();
    let recorded = recorded();
    assert_eq!(
        menu_json(&favourites::menu(&fixture_favourites(&store))),
        recorded["menu"]
    );
    // nested folders ("/art/" is "art"), searches before folders, each by
    // name, capitals first
    assert_eq!(
        menu_json(&favourites::menu(&recorded_rows(&store, &recorded))),
        recorded["nested_menu"]
    );
    // with none, no separator after saving
    assert_eq!(
        menu_json(&favourites::menu(&[])),
        json!([{ "text": "manage favourite searches" }, "---", { "text": "save this search" }])
    );
}

#[test]
fn the_manage_dialog_lists_them_as_the_reference_s() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let words = Words::new(&store);
    let list = words.list(&recorded_rows(&store, &recorded));
    let theirs: Vec<Vec<String>> = recorded["list"]
        .as_array()
        .unwrap()
        .iter()
        .map(strings)
        .collect();
    let ours: Vec<Vec<String>> = list.iter().map(|r| r.to_vec()).collect();
    assert_eq!(ours, theirs);
}

#[test]
fn searches_are_saved_and_added_as_the_reference_saves_and_adds_them() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let words = Words::new(&store);

    // "save this search": the page's search, sort and collect (a page
    // opened on "my files" with the predicates, as the recording's)
    let predicates = strings(&recorded["page_predicates"])
        .iter()
        .map(|p| {
            hydrus_search::parse_api_search(&json!([p]))
                .unwrap()
                .remove(0)
        })
        .collect();
    let page = SearchPage::restored(
        store.clone(),
        hydrus_search::FileSearchContext {
            location: hydrus_search::LocationContext::single(ServiceKey::new(
                hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
            )),
            tags: hydrus_search::TagContext::default(),
            predicates,
        },
        true,
        None,
        Vec::new(),
    );
    assert_eq!(page.predicates(), strings(&recorded["page_predicates"]));
    let saved = page.favourite_to_save().unwrap();
    assert_eq!(words.describe(&saved), recorded["saved"]);

    // added in turn: each named so no other search has its name, starting
    // the edit dialog as the reference's does
    let sorts: SortSettings = store.read(hydrus_store::settings::get).unwrap();
    let defaults: hydrus_store::settings::SearchDefaults =
        store.read(hydrus_store::settings::get).unwrap();
    let mut manager = Manager::new(recorded_rows(&store, &recorded));
    for step in recorded["added"].as_array().unwrap() {
        let row = match step["what"].as_str().unwrap() {
            "saved" => saved.clone(),
            "add" => favourites::new_search(defaults.local_location.clone()),
            // (named as a search in another folder is)
            _ => FavouriteSearch {
                folder: None,
                name: "paintings".into(),
                sort: None,
                collect: None,
                ..fixture_favourites(&store)[0].clone()
            },
        };
        let row = manager.to_add(row);
        let edit = Edit::new(&row, &sorts.default_sort, &sorts.default_collect);
        let started = &step["started"];
        assert_eq!(edit.folder, started["folder"].as_str().unwrap());
        assert_eq!(edit.name, started["name"].as_str().unwrap());
        assert_eq!(edit.save_sort, started["save_sort"].as_bool().unwrap());
        assert_eq!(
            edit.save_collect,
            started["save_collect"].as_bool().unwrap()
        );
        assert_eq!(words.describe(&edit.value()), started["value"]);
        manager.added(edit.value());
        let ours: Vec<(String, String)> = words
            .list(&manager.rows)
            .into_iter()
            .map(|[folder, name, ..]| (folder, name))
            .collect();
        let theirs: Vec<(String, String)> = step["list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r[0].as_str().unwrap().to_owned(),
                    r[1].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(ours, theirs);
    }
}

#[test]
fn the_edit_dialog_asks_before_overwriting_as_the_reference_s() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let sorts: SortSettings = store.read(hydrus_store::settings::get).unwrap();
    let search = fixture_favourites(&store)[0].search.clone();
    let existing: BTreeMap<Option<String>, Vec<String>> = BTreeMap::from([
        (None, vec!["zebra".to_owned(), "apple".to_owned()]),
        (Some("art".to_owned()), vec!["paintings".to_owned()]),
    ]);
    for case in recorded["edits"].as_array().unwrap() {
        let row = FavouriteSearch {
            folder: case["original"][0].as_str().map(str::to_owned),
            name: case["original"][1].as_str().unwrap().to_owned(),
            search: search.clone(),
            synchronised: true,
            sort: None,
            collect: None,
        };
        let mut edit = Edit::new(&row, &sorts.default_sort, &sorts.default_collect);
        case["renamed"][0]
            .as_str()
            .unwrap()
            .clone_into(&mut edit.folder);
        case["renamed"][1]
            .as_str()
            .unwrap()
            .clone_into(&mut edit.name);
        let question = edit.overwrite_question(&existing);
        assert_eq!(
            question.iter().cloned().collect::<Vec<_>>(),
            strings(&case["asked"]),
            "{case}"
        );
        let ok = question.is_none() || case["answer"].as_bool().unwrap();
        assert_eq!(ok, case["ok"].as_bool().unwrap(), "{case}");
    }
}

#[test]
fn the_edit_dialog_gives_back_what_the_reference_s_does() {
    let (_dirs, store) = store();
    let recorded = recorded();
    let words = Words::new(&store);
    let sorts: SortSettings = store.read(hydrus_store::settings::get).unwrap();
    let fixture = fixture_favourites(&store)[0].clone();
    for case in recorded["values"].as_array().unwrap() {
        let row = FavouriteSearch {
            folder: None,
            name: "name".into(),
            synchronised: false,
            ..fixture.clone()
        };
        let mut edit = Edit::new(&row, &sorts.default_sort, &sorts.default_collect);
        case["folder"]
            .as_str()
            .unwrap()
            .clone_into(&mut edit.folder);
        edit.save_sort = case["save_sort"].as_bool().unwrap();
        edit.save_collect = case["save_collect"].as_bool().unwrap();
        assert_eq!(words.describe(&edit.value()), case["value"], "{case}");
    }
}

#[test]
fn the_dialogs_save_edit_and_delete_searches() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let page = || bound.current.borrow().clone();
    page().borrow_mut().add_predicate("blue eyes");
    let list = || {
        bound
            .favourites
            .list
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the list open")
    };
    let edit = || {
        bound
            .favourites
            .edit
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the edit dialog open")
    };
    let rows = |window: &hydrus_gui::FavouritesWindow| -> Vec<(String, String, bool)> {
        window
            .get_rows()
            .iter()
            .map(|r| {
                let cell = |i| r.cells.row_data(i).unwrap().to_string();
                (cell(0), cell(1), r.selected)
            })
            .collect()
    };
    let kept = || {
        let favourites: hydrus_store::settings::FavouriteSearches =
            store.read(hydrus_store::settings::get).unwrap();
        favourites
            .0
            .into_iter()
            .map(|f| (f.folder, f.name))
            .collect::<Vec<_>>()
    };
    let example = (Some("example search".to_owned()), "inbox filter".to_owned());

    // "save this search": the list, and the edit dialog on the page's
    // search
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 2, 0.0, 0.0, 0.0);
    let window = edit();
    assert_eq!(window.get_name(), "new favourite search");
    assert_eq!(window.get_folder(), "");
    let predicates = |window: &hydrus_gui::FavouriteEditWindow| -> Vec<String> {
        window
            .get_predicates()
            .iter()
            .map(|p| p.to_string())
            .collect()
    };
    assert_eq!(predicates(&window), page().borrow().predicates());
    assert!(window.get_save_sort() && window.get_save_collect());
    assert_eq!(
        window.get_sort_label(),
        "sort by file: filesize, smallest first"
    );
    assert_eq!(window.get_collect_label(), "no collections");
    // typing adds to its search, a double-click removes from it; what
    // doesn't parse says why
    window.set_typed("series:metroid".into());
    window.invoke_typed_accepted();
    assert_eq!(predicates(&window).last().unwrap(), "series:metroid");
    assert_eq!(window.get_typed(), "");
    window.set_typed("system:nonsense".into());
    window.invoke_typed_accepted();
    assert!(!window.get_error().is_empty());
    window.invoke_predicate_removed(0);
    assert!(!predicates(&window).contains(&"blue eyes".to_owned()));
    window.invoke_save_collect_ticked(false);
    window.set_folder("mine".into());
    window.invoke_apply();
    assert!(
        bound.favourites.edit.borrow().is_none(),
        "the edit dialog closed"
    );
    let manage = list();
    assert_eq!(
        rows(&manage),
        [
            (
                "example search".to_owned(),
                "inbox filter".to_owned(),
                false
            ),
            ("mine".to_owned(), "new favourite search".to_owned(), true),
        ]
    );
    assert!(manage.get_can_edit() && manage.get_can_delete());
    // nothing kept until "apply"
    assert_eq!(kept(), std::slice::from_ref(&example));
    manage.invoke_apply();
    assert!(bound.favourites.list.borrow().is_none(), "the list closed");
    assert_eq!(
        kept(),
        [
            example.clone(),
            (Some("mine".to_owned()), "new favourite search".to_owned())
        ]
    );
    let favourites: hydrus_store::settings::FavouriteSearches =
        store.read(hydrus_store::settings::get).unwrap();
    let mine = &favourites.0[1];
    assert_eq!(mine.search.predicates.len(), 1);
    assert!(mine.sort.is_some() && mine.collect.is_none());
    // and the menu has it
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    assert_eq!(pane_lines(&ui, 0)[4..], ["example search >", "mine >"]);
    ui.invoke_menu_line_hovered(0, 5, 200.0, 100.0, 10.0);
    ui.invoke_menu_line_clicked(1, 0, 0.0, 0.0, 0.0);
    assert_eq!(page().borrow().predicates(), ["series:metroid"]);

    // "manage favourite searches": sorted on a header clicked, and again
    // the other way
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let manage = list();
    assert!(!manage.get_can_edit() && !manage.get_can_delete());
    manage.invoke_header_clicked(1);
    let names = |window| rows(window).into_iter().map(|r| r.1).collect::<Vec<_>>();
    assert_eq!(names(&manage), ["inbox filter", "new favourite search"]);
    manage.invoke_header_clicked(1);
    assert!(!manage.get_ascending());
    assert_eq!(names(&manage), ["new favourite search", "inbox filter"]);

    // deleting asks first; "no" keeps it, "yes" doesn't
    manage.invoke_row_clicked(1, false, false);
    manage.invoke_delete();
    assert_eq!(manage.get_question(), "Remove all selected?");
    manage.invoke_answered(false);
    assert_eq!(manage.get_question(), "");
    assert_eq!(rows(&manage).len(), 2);
    manage.invoke_delete();
    manage.invoke_answered(true);
    assert_eq!(names(&manage), ["new favourite search"]);
    // cancelled, nothing changes
    manage.invoke_cancel();
    assert_eq!(kept().len(), 2);

    // renamed onto another, it asks first; told yes, it replaces it
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let manage = list();
    manage.invoke_row_activated(1);
    let window = edit();
    assert_eq!(window.get_name(), "new favourite search");
    window.set_folder("example search".into());
    window.set_name("inbox filter".into());
    window.invoke_apply();
    assert_eq!(
        window.get_question(),
        "The search \"inbox filter\" under folder \"example search\" already exists! Do you want to overwrite it?"
    );
    window.invoke_answered(false);
    assert!(bound.favourites.edit.borrow().is_some(), "still open");
    window.invoke_apply();
    window.invoke_answered(true);
    assert!(bound.favourites.edit.borrow().is_none());
    assert_eq!(
        rows(&manage),
        [("example search".to_owned(), "inbox filter".to_owned(), true)]
    );
    manage.invoke_apply();
    let favourites: hydrus_store::settings::FavouriteSearches =
        store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(favourites.0.len(), 1);
    assert_eq!(
        favourites.0[0].search.predicates.len(),
        1,
        "the search replaced"
    );

    // a new one saved over another replaces it too
    ui.invoke_favourites_menu_requested(10.0, 100.0);
    ui.invoke_menu_line_clicked(0, 2, 0.0, 0.0, 0.0);
    let window = edit();
    window.set_folder("example search".into());
    window.set_name("inbox filter".into());
    window.invoke_apply();
    assert!(window.get_question().contains("already exists"));
    window.invoke_answered(true);
    assert_eq!(
        rows(&list()),
        [("example search".to_owned(), "inbox filter".to_owned(), true)]
    );
}
