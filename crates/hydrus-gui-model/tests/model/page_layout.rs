//! Recorded global defaults, per-page geometry and concurrent staged Options.
use hydrus_gui_model::{
    options::{Editor, Settings},
    page_layout::Layout,
};
use hydrus_store::{
    page_layout::{self, PageLayout},
    settings,
};

#[test]
fn qt_live_sizes_saved_hidden_positions_and_signed_legacy_bounds() {
    let f = hydrus_testkit::fixture_json("sidebar_layout.json");
    let saved = PageLayout::default();
    assert_eq!(
        serde_json::json!({"hpos":saved.hpos,"vpos":saved.vpos,"hide_preview":saved.hide_preview,"save_on_exit":saved.save_on_exit}),
        f["defaults"]
    );
    let mut first = Layout::saved(&saved);
    assert_eq!(first.dimensions(1388, 922), (400, 240));
    first.resize(false, 470);
    first.resize(true, 295);
    let mut hidden_unsaved = first;
    hidden_unsaved.toggle(&saved);
    assert_eq!(
        hidden_unsaved.positions(hidden_unsaved.dimensions(1388, 922), &saved),
        (0, -295)
    );
    let other = Layout::saved(&saved);
    assert_eq!(
        first.positions(first.dimensions(1388, 922), &saved),
        (470, -295)
    );
    assert_eq!(other.dimensions(1388, 922), (400, 240));
    let step = |name: &str| {
        f["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["action"] == name)
            .unwrap()
    };
    assert_eq!(
        step("save current now")["saved"],
        serde_json::json!([470, -295])
    );
    let persisted = PageLayout {
        hpos: 470,
        vpos: -295,
        ..saved.clone()
    };
    first.toggle(&persisted);
    assert_eq!(
        first.positions(first.dimensions(1388, 922), &persisted),
        (0, -295)
    );
    assert_eq!(
        step("save while hidden")["saved"],
        serde_json::json!([0, -295])
    );
    first.toggle(&persisted);
    assert_eq!(first.dimensions(1388, 922), (470, 295));
    first.resize(true, 0);
    assert_eq!(
        first.positions(first.dimensions(1388, 922), &persisted),
        (470, -295)
    );
    let signed = Layout::saved(&PageLayout {
        hpos: -900,
        vpos: 600,
        ..saved.clone()
    });
    assert_eq!(signed.dimensions(1388, 922), (488, 788));
    assert_eq!(
        f["signed_legacy_probes"][0]["actual"]["sash"],
        serde_json::json!([488, -788])
    );
    let extreme = Layout::saved(&PageLayout {
        hpos: i64::MAX,
        vpos: i64::MIN,
        ..saved
    });
    assert_eq!(extreme.dimensions(1400, 1000), (1320, 920));
    assert_eq!(extreme.dimensions(0, 0), (0, 0));
}
#[test]
fn hide_draft_cancel_apply_reopen_preserves_concurrent_saved_sizes_and_exit_switch() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|p| *p == "gui pages")
        .unwrap();
    editor.show_page(page);
    let row=editor.rows().iter().position(|r|matches!(r,hydrus_gui_model::options::Row::Opt{option,..} if option.label=="Hide the bottom-left preview window: ")).unwrap();
    editor.check(row, true);
    assert!(!store.read(page_layout::load).unwrap().hide_preview);
    drop(editor);
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    editor.show_page(page);
    editor.check(row, true);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &PageLayout {
                    hpos: 515,
                    vpos: -315,
                    save_on_exit: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let (after, before, errors) = editor.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store.read(page_layout::load).unwrap(),
        PageLayout {
            hpos: 515,
            vpos: -315,
            save_on_exit: false,
            hide_preview: true
        }
    );
    let reopened = hydrus_store::Store::open(dir.path()).unwrap();
    assert!(
        reopened
            .read(Settings::load)
            .unwrap()
            .page_layout
            .hide_preview
    );
}
