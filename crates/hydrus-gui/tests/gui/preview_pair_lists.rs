//! The rule editor's preview pair lists, against the reference's real preview panel.

use slint::{ComponentHandle as _, Model as _};

use crate::auto_resolution_review::{Opened, opened};
use crate::duplicates_lane_page::tested;

/// The rule editor's preview lists against the reference's real `PreviewPanel`
/// (`oracle/record_preview_pair_lists.py`): each list takes one row at a time (control and
/// shift clicks do not add rows), offers "show selected row in a new page" once a row is
/// selected, and that sends the row's two files to a new page.
// leaf: audit-media-preview-context
#[test]
fn preview_pair_lists_offer_show_selected_pairs_in_a_new_page() {
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("preview_pair_lists.json");
    for (rule_name, lists) in recorded.as_object().unwrap() {
        let Opened { ui, bound, hex, .. } = opened();
        ui.invoke_duplicates_action("edit rules".into(), 0, false, false);
        let list = bound
            .auto_resolution
            .list
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        let row = list
            .get_rows()
            .iter()
            .position(|r| {
                r.cells
                    .iter()
                    .next()
                    .is_some_and(|c| c == rule_name.as_str())
            })
            .unwrap_or_else(|| panic!("no rule {rule_name}"));
        list.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
        list.invoke_edit();
        let rule = bound
            .auto_resolution
            .rule
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        rule.set_tab(3);
        rule.invoke_preview_shown();
        tested(&rule);
        for passing in [true, false] {
            let name = if passing { "pass" } else { "fail" };
            let recorded_list = &lists[name];
            let rows = |rule: &hydrus_gui::AutoResolutionRuleWindow| {
                if passing {
                    rule.get_preview_pass_rows()
                } else {
                    rule.get_preview_fail_rows()
                }
            };
            let menu = |rule: &hydrus_gui::AutoResolutionRuleWindow| {
                if passing {
                    rule.get_preview_pass_menu()
                } else {
                    rule.get_preview_fail_menu()
                }
            };
            let recorded_rows = recorded_list["rows"].as_array().unwrap();
            assert_eq!(
                rows(&rule).row_count(),
                recorded_rows.len(),
                "{rule_name} {name}"
            );
            for step in recorded_list["steps"].as_array().unwrap() {
                let what = format!("{rule_name} {name}: {}", step["step"]);
                match step["step"].as_str().unwrap() {
                    "nothing selected" => {}
                    "first row clicked" => {
                        if passing {
                            rule.invoke_preview_pass_clicked(0, false, false);
                        } else {
                            rule.invoke_preview_fail_clicked(0, false, false);
                        }
                    }
                    "second row ctrl-clicked" => {
                        if passing {
                            rule.invoke_preview_pass_clicked(1, true, false);
                        } else {
                            rule.invoke_preview_fail_clicked(1, true, false);
                        }
                    }
                    "third row shift-clicked" => {
                        if passing {
                            rule.invoke_preview_pass_clicked(2, false, true);
                        } else {
                            rule.invoke_preview_fail_clicked(2, false, true);
                        }
                    }
                    other => panic!("{other}"),
                }
                let selected: Vec<usize> = rows(&rule)
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.selected)
                    .map(|(i, _)| i)
                    .collect();
                let wanted: Vec<usize> = step["selected"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| usize::try_from(r.as_u64().unwrap()).unwrap())
                    .collect();
                assert_eq!(selected, wanted, "{what}: selected rows");
                let labels: Vec<String> = step["menu"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|m| m.as_array().unwrap())
                    .map(|l| l.as_str().unwrap().to_owned())
                    .collect();
                let ours = menu(&rule).to_string();
                assert_eq!(
                    if ours.is_empty() { vec![] } else { vec![ours] },
                    labels,
                    "{what}: menu"
                );
                if let Some(shown) = step["shown"].as_array().and_then(|s| s.first()) {
                    let pages_before = bound.pages.borrow().open_pages().len();
                    if passing {
                        rule.invoke_preview_pass_show();
                    } else {
                        rule.invoke_preview_fail_show();
                    }
                    assert_eq!(
                        bound.pages.borrow().open_pages().len(),
                        pages_before + 1,
                        "{what}"
                    );
                    let files: Vec<String> = bound
                        .current
                        .borrow()
                        .borrow()
                        .files()
                        .iter()
                        .map(|f| hex[f].clone())
                        .collect();
                    // (pairs the sort cannot tell apart may come in another order, so the
                    // row's pair is one the reference listed, of the same smallest size)
                    let mut files = files;
                    files.sort();
                    let size_of = |h: &str| recorded_list["sizes"][h].as_u64().unwrap();
                    let smallest = |pair: &[String]| pair.iter().map(|h| size_of(h)).min().unwrap();
                    let row_index = usize::try_from(step["selected"][0].as_u64().unwrap()).unwrap();
                    let wanted_row: Vec<String> = recorded_rows[row_index]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|h| h.as_str().unwrap().to_owned())
                        .collect();
                    assert!(
                        recorded_rows.iter().any(|r| {
                            let mut pair: Vec<String> = r
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|h| h.as_str().unwrap().to_owned())
                                .collect();
                            pair.sort();
                            pair == files
                        }),
                        "{what}: {files:?} is not a pair the reference listed"
                    );
                    assert_eq!(
                        smallest(&files),
                        smallest(&wanted_row),
                        "{what}: the row's pair"
                    );
                    assert_eq!(
                        shown["hashes"].as_array().unwrap().len(),
                        files.len(),
                        "{what}"
                    );
                    // the page's location: the same service the reference chose
                    let location: Vec<String> = bound
                        .current
                        .borrow()
                        .borrow()
                        .location()
                        .current()
                        .iter()
                        .map(|k| {
                            k.as_bytes().iter().fold(String::new(), |mut s, b| {
                                use std::fmt::Write as _;
                                write!(s, "{b:02x}").unwrap();
                                s
                            })
                        })
                        .collect();
                    let wanted_location: Vec<String> = shown["location"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|k| k.as_str().unwrap().to_owned())
                        .collect();
                    assert_eq!(location, wanted_location, "{what}: the page's location");
                }
            }
            let mut sizes_by_row = vec![];
            let mut pairs_listed: Vec<Vec<String>> = vec![];
            for row in 0..recorded_rows.len() {
                let row = i32::try_from(row).unwrap();
                if passing {
                    rule.invoke_preview_pass_clicked(row, false, false);
                    rule.invoke_preview_pass_show();
                } else {
                    rule.invoke_preview_fail_clicked(row, false, false);
                    rule.invoke_preview_fail_show();
                }
                let mut pair: Vec<String> = bound
                    .current
                    .borrow()
                    .borrow()
                    .files()
                    .iter()
                    .map(|f| hex[f].clone())
                    .collect();
                pair.sort();
                sizes_by_row.push(
                    pair.iter()
                        .map(|h| recorded_list["sizes"][h].as_u64().unwrap())
                        .min()
                        .unwrap(),
                );
                pairs_listed.push(pair);
            }
            // the same pairs, sorted by their smaller file's size, largest first
            let mut wanted_pairs: Vec<Vec<String>> = recorded_rows
                .iter()
                .map(|r| {
                    let mut pair: Vec<String> = r
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|h| h.as_str().unwrap().to_owned())
                        .collect();
                    pair.sort();
                    pair
                })
                .collect();
            let wanted_sizes: Vec<u64> = recorded_rows
                .iter()
                .map(|r| {
                    r.as_array()
                        .unwrap()
                        .iter()
                        .map(|h| {
                            recorded_list["sizes"][h.as_str().unwrap()]
                                .as_u64()
                                .unwrap()
                        })
                        .min()
                        .unwrap()
                })
                .collect();
            assert_eq!(
                sizes_by_row, wanted_sizes,
                "{rule_name} {name}: the rows' order"
            );
            pairs_listed.sort();
            wanted_pairs.sort();
            assert_eq!(pairs_listed, wanted_pairs, "{rule_name} {name}: the pairs");
        }
    }
}
