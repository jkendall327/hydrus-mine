//! Options > gui pages > "When closing the current tab, move focus": chosen in the options
//! window, then every tab of four closed in every selection, compared with where the
//! reference left focus (`oracle/record_close_focus.py`, edge tabs included).
use hydrus_core::service::builtin_keys;
use hydrus_gui::page_chooser::NewPage;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use slint::{ComponentHandle as _, Model as _};

use crate::options_window::{open, row, show_page, store};

// leaf: audit-options-gui-pages-opening-and-closing-when-closing-the-current-tab-move-focus
#[test]
fn closing_the_current_tab_moves_focus_where_the_reference_does_for_each_choice() {
    let recorded = hydrus_testkit::fixture_json("close_focus.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    // the option, as the reference lists it
    let label = "When closing the current tab, move focus: ";
    {
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "gui pages");
        let (_, shown) = row(&window, label);
        // (the default is "right of the closed page tab": the second choice)
        assert_eq!(shown.index, recorded["default"].as_i64().unwrap() as i32);
        window.invoke_cancel();
    }
    for option in [0_i64, 1] {
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        open(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        show_page(&window, "gui pages");
        let (index, _) = row(&window, label);
        window.invoke_choice_chosen(index, i32::try_from(option).unwrap());
        window.invoke_apply();
        for case in recorded["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["option"] == option)
        {
            let names = ["a", "b", "c", "d"];
            let at = |name: &str| names.iter().position(|n| *n == name).unwrap();
            let mut pages = Pages::single(hydrus_gui::SearchPage::new(store.clone()));
            for name in &names[1..] {
                pages
                    .new_page(&NewPage::Search {
                        domain: hydrus_core::ServiceKey::new(
                            builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
                        ),
                        name: (*name).into(),
                    })
                    .unwrap();
            }
            let ui = MainWindow::new().unwrap();
            let _bound = bind(&ui, pages);
            let selected = at(case["selected"].as_str().unwrap());
            let closed = at(case["closed"].as_str().unwrap());
            ui.invoke_tab_chosen(0, i32::try_from(selected).unwrap());
            ui.invoke_close_tab(0, i32::try_from(closed).unwrap());
            let row = ui.get_tab_rows().row_data(0).unwrap();
            assert_eq!(
                (row.names.row_count(), i64::from(row.selected)),
                (3, case["current_index"].as_i64().unwrap()),
                "{case}"
            );
        }
    }
}
