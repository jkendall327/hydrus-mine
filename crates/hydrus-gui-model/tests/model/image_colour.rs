//! Real finite Qt ICC checkbox: staged persistence and independent fields.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{Store, image_colour, settings};
const LABEL: &str = "Apply image ICC Profile colour adjustments:";
fn editor(store: &Store) -> (Editor, usize) {
    let mut edit = Editor::new(store.read(Settings::load).unwrap());
    let page = edit
        .page_names()
        .iter()
        .position(|page| *page == "media playback")
        .unwrap();
    edit.show_page(page);
    let row = edit
        .rows()
        .iter()
        .position(|r| matches!(r, Row::Opt { option, .. } if option.label == LABEL))
        .unwrap();
    (edit, row)
}
#[test]
fn qt_icc_checkbox_cancel_saved_reopen_preserves_concurrent_viewer_rules() {
    let fixture = hydrus_testkit::fixture_json("image_decoder_policies.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.read(image_colour::load).unwrap().normalise_icc,
        fixture["factory"][0]
    );
    let (mut cancelled, row) = editor(&store);
    cancelled.check(row, false);
    drop(cancelled);
    assert_eq!(
        store.read(image_colour::load).unwrap().normalise_icc,
        fixture["cancel"][0]
    );
    for case in fixture["cases"].as_array().unwrap() {
        let (mut edit, row) = editor(&store);
        let before = store.read(image_colour::load).unwrap();
        let enabled = case["icc"].as_bool().unwrap();
        edit.check(row, enabled);
        assert_eq!(store.read(image_colour::load).unwrap(), before);
        store
            .write(|c| {
                let mut rules: hydrus_core::media_viewer::MediaViewerSettings =
                    settings::get(c.conn())?;
                rules.rating_icon_size = 29.0;
                settings::set(c.conn(), &rules)
            })
            .unwrap();
        let rules: hydrus_core::media_viewer::MediaViewerSettings =
            store.read(settings::get).unwrap();
        let (after, original, problems) = edit.applied();
        assert!(problems.is_empty());
        let original = original.clone();
        store
            .write(move |c| after.save(c.conn(), &original))
            .unwrap();
        assert_eq!(
            store.read(image_colour::load).unwrap().normalise_icc,
            case["saved"][0]
        );
        assert_eq!(
            store
                .read::<hydrus_core::media_viewer::MediaViewerSettings>(settings::get)
                .unwrap(),
            rules
        );
        assert_eq!(
            Store::open(store.dir())
                .unwrap()
                .read(image_colour::load)
                .unwrap()
                .normalise_icc,
            case["reopened"][0]
        );
    }
}
