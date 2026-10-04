//! Close prompts, undo, staged controls, live history limits and real text focus.
use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::{
    Store, sessions,
    settings::{self, PageNavigationSettings},
};
use slint::{ComponentHandle as _, Model as _};

fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: hydrus_search::FileSearchContext::default(),
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}
fn notebook(name: &str, children: Vec<Page>) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Pages(children),
    }
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let i = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(i).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let i = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "gui pages")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(i).unwrap());
    window
}
fn row(window: &OptionsWindow, label: &str) -> i32 {
    let rows = window.get_rows();
    i32::try_from(
        (0..rows.row_count())
            .find(|&i| rows.row_data(i).unwrap().label == label)
            .unwrap(),
    )
    .unwrap()
}
const CONFIRM: &str = "Confirm when closing any page: ";
const FOCUS: &str = "When switching to pages, move keyboard focus to any text input field: ";
const HISTORY: &str = "Maximum entries to show in page navigation history: ";

#[test]
fn real_close_questions_cancel_and_undo_preserve_nested_trees_and_session_exemption() {
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("page_navigation_options.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![
            search("search"),
            notebook("empty", vec![]),
            notebook(
                "held",
                vec![
                    notebook("nested", vec![search("nested search")]),
                    search("second search"),
                ],
            ),
            search("two"),
        ],
    };
    let saved = original.clone();
    store
        .write(move |ctx| sessions::save(ctx.conn(), &saved, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    for step in fixture["close"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["for_session"] == false)
    {
        let enabled = step["enabled"].as_bool().unwrap();
        store
            .write(move |ctx| {
                settings::set(
                    ctx.conn(),
                    &PageNavigationSettings {
                        confirm_all_closes: enabled,
                        ..PageNavigationSettings::default()
                    },
                )
            })
            .unwrap();
        let index = match step["kind"].as_str().unwrap() {
            "search" => 0,
            "empty" => 1,
            "held" => 2,
            _ => unreachable!(),
        };
        ui.invoke_close_tab(0, index);
        let questions = step["questions"].as_array().unwrap();
        if let Some(question) = questions.first() {
            assert_eq!(ui.get_question(), question.as_str().unwrap());
            ui.invoke_answer(step["accepted"].as_bool().unwrap());
        } else {
            assert!(ui.get_question().is_empty());
        }
        if step["error"] == "VetoException" {
            assert_eq!(bound.pages.borrow().session(), &original);
        } else {
            assert_eq!(
                bound.pages.borrow().session().pages.len(),
                original.pages.len() - 1
            );
            ui.invoke_unclose_page();
            assert_eq!(bound.pages.borrow().session(), &original);
        }
        assert!(
            bound.pages.borrow_mut().session_close_vetoes().is_empty(),
            "all-close preference adds no session replacement veto"
        );
    }
}

#[test]
fn applied_controls_reopen_and_drive_history_and_search_focus() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![search("first"), search("second"), search("third")],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &original, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    windows
        .get(0)
        .unwrap()
        .dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    let cancelled = open(&ui, &bound);
    cancelled.invoke_check_toggled(row(&cancelled, CONFIRM), true);
    cancelled.invoke_check_toggled(row(&cancelled, FOCUS), true);
    cancelled.invoke_number_edited(row(&cancelled, HISTORY), 2);
    cancelled.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<PageNavigationSettings>).unwrap(),
        PageNavigationSettings::default()
    );
    let before = ui.get_search_focus_requests();
    ui.invoke_tab_chosen(0, 1);
    assert_eq!(ui.get_search_focus_requests(), before);
    let options = open(&ui, &bound);
    options.invoke_check_toggled(row(&options, CONFIRM), true);
    options.invoke_check_toggled(row(&options, FOCUS), true);
    options.invoke_number_edited(row(&options, HISTORY), 2);
    options.invoke_apply();
    assert_eq!(
        store.read(settings::get::<PageNavigationSettings>).unwrap(),
        PageNavigationSettings {
            confirm_all_closes: true,
            focus_search_on_change: true,
            history_entries: 2
        }
    );
    ui.invoke_tab_chosen(0, 2);
    headless::render(&windows.get(0).unwrap(), 1100, 700);
    assert_eq!(ui.get_search_focus_requests(), before + 1);
    assert!(
        ui.get_search_focused(),
        "actual Slint autocomplete text input receives focus"
    );
    ui.invoke_tab_chosen(0, 0);
    assert_eq!(ui.get_search_focus_requests(), before + 2);
    ui.invoke_tab_chosen(0, 0);
    assert_eq!(
        ui.get_search_focus_requests(),
        before + 2,
        "same page does not request a new focus"
    );
    ui.invoke_menu_title_pressed(2, 100.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let i = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "history")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(i).unwrap(), 100.0, 50.0, 12.0);
    let entries = ui.get_menu_panes().row_data(1).unwrap().lines;
    let labels = (0..entries.row_count())
        .map(|i| entries.row_data(i).unwrap().label.to_string())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["1: first", "2: third", "Clear History"]);
    assert_eq!(
        bound.pages.borrow().history().len(),
        3,
        "menu limit keeps older navigation history"
    );
    ui.invoke_menu_dismissed();
    let reopened = open(&ui, &bound);
    assert!(
        reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened, CONFIRM)).unwrap())
            .unwrap()
            .checked
    );
    assert!(
        reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened, FOCUS)).unwrap())
            .unwrap()
            .checked
    );
    assert_eq!(
        reopened
            .get_rows()
            .row_data(usize::try_from(row(&reopened, HISTORY)).unwrap())
            .unwrap()
            .number,
        2
    );
    reopened.invoke_check_toggled(row(&reopened, FOCUS), false);
    reopened.invoke_apply();
    ui.invoke_tab_chosen(0, 1);
    assert_eq!(ui.get_search_focus_requests(), before + 2);
}

#[test]
fn switching_to_each_importer_focuses_its_actual_query_or_url_text_input() {
    let windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("page_navigation_options.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    let main = windows.get(0).unwrap();
    main.dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    for (kind, button) in [("gallery", 6), ("watchers", 4), ("simple", 2), ("urls", 8)] {
        ui.invoke_tab_space_pressed(0, false);
        ui.invoke_tab_space_pressed(0, false);
        ui.invoke_chooser_pressed(4);
        ui.invoke_chooser_pressed(button);
        let index = i32::try_from(bound.pages.borrow().session().pages.len() - 1).unwrap();
        for step in fixture["importer_focus"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|step| step["kind"] == kind)
        {
            let enabled = step["enabled"].as_bool().unwrap();
            store
                .write(move |ctx| {
                    settings::set(
                        ctx.conn(),
                        &PageNavigationSettings {
                            focus_search_on_change: enabled,
                            ..PageNavigationSettings::default()
                        },
                    )
                })
                .unwrap();
            ui.invoke_tab_chosen(0, 0);
            headless::render(&main, 1100, 900);
            let before = ui.get_page_focus_requests();
            ui.invoke_tab_chosen(0, index);
            headless::render(&main, 1100, 900);
            assert_eq!(
                ui.get_page_focus_requests() - before,
                i32::try_from(step["calls"].as_array().unwrap().len()).unwrap(),
                "{step}"
            );
            if enabled {
                assert!(
                    ui.get_page_input_focused(),
                    "{kind} query/url widget has actual text focus"
                );
            }
        }
    }
}
