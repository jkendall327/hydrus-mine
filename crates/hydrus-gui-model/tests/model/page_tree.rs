#[path = "../support/notebook_tree.rs"]
mod support;
use hydrus_gui_model::page_tree::Tree;
// leaf: audit-options-gui-pages-navigation-and-drag-and-drop-experimental-show-tab-tree-view
#[test]
fn cursor_expansion_and_activation_replay_actual_frame_tree() {
    let fixture = hydrus_testkit::fixture_json("notebook_tree.json");
    let session = support::source();
    let mut shown = support::key(&session, "gamma");
    let mut tree = Tree::default();
    tree.sync(&session.pages, shown);
    for step in fixture["steps"].as_array().unwrap() {
        let label = step["action"].as_str().unwrap();
        if let Some(action) = support::action(label) {
            if let Some(key) = tree.navigate(action) {
                shown = key;
            }
        } else {
            match label {
                "click beta" => tree.select(support::key(&session, "beta")),
                "select inner" => tree.select(support::key(&session, "inner")),
                "collapse alpha" => {
                    tree.select(support::key(&session, "alpha"));
                    tree.navigate(2);
                }
                "expand inner" | "click inner disclosure" => {
                    tree.toggle(support::key(&session, "inner"));
                }
                "collapse parent retains inner expansion" | "expand parent" => {
                    tree.toggle(support::key(&session, "alpha"));
                }
                "double click beta" => {
                    tree.select(support::key(&session, "beta"));
                    shown = tree.navigate(6).unwrap();
                }
                "show gamma reveals ancestors" => shown = support::key(&session, "gamma"),
                "double click inner" => {
                    tree.select(support::key(&session, "inner"));
                    tree.toggle(support::key(&session, "inner"));
                }
                "initial" => {}
                _ => panic!("unknown action {label}"),
            }
        }
        tree.sync(&session.pages, shown);
        let entries = support::entries(&session.pages);
        let name = |key| entries.iter().find(|(k, _)| *k == key).unwrap().1.clone();
        assert_eq!(
            serde_json::json!(
                tree.visible()
                    .iter()
                    .map(|n| n.name.clone())
                    .collect::<Vec<_>>()
            ),
            step["visible"],
            "{label}"
        );
        assert_eq!(
            serde_json::json!(
                entries
                    .iter()
                    .filter(|(k, _)| tree.expanded(*k))
                    .map(|(_, n)| n.clone())
                    .collect::<Vec<_>>()
            ),
            step["expanded"],
            "{label}"
        );
        assert_eq!(
            name(tree.current().unwrap()),
            step["current"].as_str().unwrap(),
            "{label}"
        );
        assert_eq!(name(shown), step["shown"].as_str().unwrap(), "{label}");
    }
}
#[test]
fn stable_keys_survive_reorder_rename_and_closed_cursor_without_stale_activation() {
    let mut session = support::source();
    let shown = support::key(&session, "gamma");
    let cursor = support::key(&session, "omega");
    let mut tree = Tree::default();
    tree.sync(&session.pages, shown);
    tree.select(cursor);
    session.pages.reverse();
    session.pages[0].name = "renamed".into();
    tree.sync(&session.pages, shown);
    assert_eq!(tree.current(), Some(cursor));
    assert_eq!(tree.navigate(6), Some(cursor));
    tree.toggle(support::key(&session, "alpha"));
    tree.select(PageKey::random());
    assert_eq!(tree.current(), Some(cursor));
    session.pages.remove(0);
    tree.sync(&session.pages, shown);
    assert_eq!(tree.current(), Some(shown));
    assert!(tree.visible().iter().any(|n| n.key == shown));
    tree.navigate(4);
    tree.navigate(0);
    assert_eq!(tree.current(), Some(support::key(&session, "alpha")));
}
use hydrus_core::pages::PageKey;
