//! Actual inert OR connector control: raw staging and isolated persistence.
use hydrus_core::{
    Tag,
    search::predicate::Predicate,
    tag_presentation::{NamespaceColours, TagPresentation},
};
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_search::{TextContext, predicate_text};
use hydrus_store::{Store, or_connector, settings};
fn predicate() -> Predicate {
    Predicate::Or(vec![
        Predicate::Tag {
            tag: Tag::new("character:alpha").unwrap(),
            inclusive: true,
        },
        Predicate::Tag {
            tag: Tag::new("series:beta").unwrap(),
            inclusive: false,
        },
    ])
}
fn editor(store: &Store) -> (Editor, usize) {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|p| *p == "tag presentation")
        .unwrap();
    editor.show_page(page);
    let row=editor.rows().iter().position(|r|matches!(r,Row::Opt{option,..}if option.label=="OR connecting string (on one line): ")).unwrap();
    (editor, row)
}
// leaf: audit-options-tag-presentation-other-rendering-or-connecting-string-on-one-line
#[test]
fn qt_raw_blank_newline_cancel_reopen_and_live_namespace_changes_remain_isolated() {
    let fixture = hydrus_testkit::fixture_json("or_connector.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(
        store.read(or_connector::load).unwrap().text,
        fixture["factory"]
    );
    let predicate = predicate();
    let syntax = serde_json::to_value(&predicate).unwrap();
    let (mut cancelled, row) = editor(&store);
    cancelled.text(row, "cancelled connector");
    drop(cancelled);
    assert_eq!(
        store.read(or_connector::load).unwrap().text,
        fixture["cancel"]["after"]
    );
    for case in fixture["cases"].as_array().unwrap() {
        let before = store.read(or_connector::load).unwrap();
        let (mut edit, row) = editor(&store);
        edit.text(row, case["shown"].as_str().unwrap());
        assert_eq!(store.read(or_connector::load).unwrap(), before);
        // Concurrent presentation changes belong to their own setting and must
        // survive saving only this text field from the older Options snapshot.
        store
            .write(|c| {
                let mut tags: TagPresentation = settings::get(c.conn())?;
                tags.namespace_connector = " = ".into();
                settings::set(c.conn(), &tags)?;
                let mut colours: NamespaceColours = settings::get(c.conn())?;
                colours.or_connector = Some("character".into());
                settings::set(c.conn(), &colours)
            })
            .unwrap();
        let tags: TagPresentation = store.read(settings::get).unwrap();
        let colours: NamespaceColours = store.read(settings::get).unwrap();
        let (after, original, problems) = edit.applied();
        assert!(problems.is_empty());
        let original = original.clone();
        store
            .write(move |c| after.save(c.conn(), &original))
            .unwrap();
        assert_eq!(store.read(or_connector::load).unwrap().text, case["saved"]);
        assert_eq!(store.read::<TagPresentation>(settings::get).unwrap(), tags);
        assert_eq!(
            store.read::<NamespaceColours>(settings::get).unwrap(),
            colours
        );
        let reopened = Store::open(store.dir()).unwrap();
        assert_eq!(
            reopened.read(or_connector::load).unwrap().text,
            case["reopened"]
        );
        assert_eq!(serde_json::to_value(&predicate).unwrap(), syntax);
        // Current active Qt ToString uses literal OR even with custom/empty
        // saved text; user namespace formatting remains independently live.
        assert_eq!(
            predicate_text(&predicate, &TextContext::default()),
            case["labels"]["canonical"]
        );
        let context = TextContext {
            presentation: Some(tags),
            ..Default::default()
        };
        assert_eq!(
            predicate_text(&predicate, &context),
            "-series = beta OR character = alpha"
        );
        assert_eq!(case["labels"]["header"][0][0], "OR:");
        assert_eq!(
            case["labels"]["copy"],
            serde_json::json!(["-series:beta", "character:alpha"])
        );
        assert_eq!(
            case["labels"]["collapsed_copy"],
            serde_json::json!([predicate_text(&predicate, &TextContext::default())])
        );
    }
}
