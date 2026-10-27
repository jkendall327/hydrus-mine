#[path = "../support/notebook_tree.rs"]
mod support;
use hydrus_gui_model::page_tree::Tree;
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

/// Where a drop lands among the rows, with Qt's margin for 26-high rows
/// (five pixels at either edge), and where that puts the dragged page.
#[test]
fn drops_land_above_on_or_below_rows_as_qts_tree_view_places_them() {
    use hydrus_gui_model::page_tree::{Drop, drop_at};
    let session = support::source();
    let key = |name| support::key(&session, name);
    let mut tree = Tree::default();
    tree.sync(&session.pages, key("gamma"));
    tree.navigate(8);
    let visible = tree.visible();
    // rows: alpha, one, inner, beta, gamma, omega
    assert_eq!(drop_at(&visible, 0.0, 26.0), Drop::Above(key("alpha")));
    assert_eq!(drop_at(&visible, 4.9, 26.0), Drop::Above(key("alpha")));
    assert_eq!(drop_at(&visible, 5.0, 26.0), Drop::On(key("alpha")));
    assert_eq!(drop_at(&visible, 20.0, 26.0), Drop::On(key("alpha")));
    assert_eq!(drop_at(&visible, 21.0, 26.0), Drop::Below(key("alpha")));
    assert_eq!(
        drop_at(&visible, 26.0 * 2.0 + 13.0, 26.0),
        Drop::On(key("inner"))
    );
    assert_eq!(drop_at(&visible, 26.0 * 6.0, 26.0), Drop::Space);
    assert_eq!(drop_at(&visible, -1.0, 26.0), Drop::Space);
    let omega = key("omega");
    // above or below a row: among its siblings; on a notebook: its end; on a
    // page: its notebook's end; past the rows: the top notebook's end
    assert_eq!(
        tree.drop_destination(omega, Drop::Above(key("inner"))),
        Some((Some(key("alpha")), Some(1)))
    );
    assert_eq!(
        tree.drop_destination(omega, Drop::Below(key("gamma"))),
        Some((Some(key("inner")), Some(2)))
    );
    assert_eq!(
        tree.drop_destination(omega, Drop::On(key("inner"))),
        Some((Some(key("inner")), None))
    );
    assert_eq!(
        tree.drop_destination(omega, Drop::On(key("one"))),
        Some((Some(key("alpha")), None))
    );
    assert_eq!(
        tree.drop_destination(key("beta"), Drop::Space),
        Some((None, None))
    );
    // not on itself or inside itself
    assert_eq!(
        tree.drop_destination(key("alpha"), Drop::On(key("beta"))),
        None
    );
    assert_eq!(
        tree.drop_destination(key("inner"), Drop::Above(key("inner"))),
        None
    );
}
