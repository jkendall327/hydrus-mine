//! A list's extended selection is Qt's (`oracle/record_list_selection.py`,
//! clicks on the reference's `BetterListCtrl`): ctrl and shift mixed, the
//! rows selected and the order the list hands them out in after each click.

use hydrus_gui_model::list_selection::ListSelection;

#[test]
fn ctrl_and_shift_clicks_select_and_order_rows_as_qt_does() {
    let fixture = hydrus_testkit::fixture_json("list_selection.json");
    let order: Vec<usize> = (0..fixture["files"].as_array().unwrap().len()).collect();
    let rows = |v: &serde_json::Value| -> Vec<usize> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|r| usize::try_from(r.as_u64().unwrap()).unwrap())
            .collect()
    };
    for sequence in fixture["sequences"].as_array().unwrap() {
        let mut selection = ListSelection::default();
        for (i, step) in sequence["steps"].as_array().unwrap().iter().enumerate() {
            selection.click(
                &order,
                usize::try_from(step["row"].as_u64().unwrap()).unwrap(),
                step["ctrl"].as_bool().unwrap(),
                step["shift"].as_bool().unwrap(),
            );
            let context = format!("{} step {i}", sequence["name"]);
            assert_eq!(
                selection.in_order(&order),
                rows(&step["selected"]),
                "{context}"
            );
            assert_eq!(
                selection.selected_order(),
                rows(&step["order"]),
                "{context}"
            );
        }
    }
}
