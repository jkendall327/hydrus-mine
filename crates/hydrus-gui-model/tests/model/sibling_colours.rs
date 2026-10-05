//! Actual Qt colour runs, staged preferences, and independent logical tag consumers.
use super::tag_dialog_preferences::fixture;
use hydrus_core::{
    ServiceKey,
    search::context::LocationContext,
    tag_presentation::{NamespaceColours, SiblingConnectorColours, TagText},
};
use hydrus_gui_model::{
    domains::Choice,
    manage_tags::ManageTags,
    options::{Editor, Row, Settings},
    write_autocomplete::WriteAutocomplete,
};
use hydrus_store::settings;
use serde_json::{Value, json};
const FADE: &str = "Fade the colour of the sibling connector string on Qt6: ";
const NAMESPACE: &str = "Namespace for the colour of the sibling connecting string: ";
fn editor(store: &hydrus_store::Store) -> Editor {
    let mut edit = Editor::new(store.read(Settings::load).unwrap());
    let page = edit
        .page_names()
        .iter()
        .position(|name| *name == "tag presentation")
        .unwrap();
    edit.show_page(page);
    edit
}
fn index(edit: &Editor, label: &str) -> usize {
    edit.rows()
        .iter()
        .position(|row| matches!(row,Row::Opt{option,..} if option.label==label))
        .unwrap()
}
fn edit(edit: &mut Editor, case: &Value) {
    edit.check(index(edit, FADE), false);
    let row = index(edit, NAMESPACE);
    edit.none(row, case["namespace"].is_null());
    if let Some(namespace) = case["namespace"].as_str() {
        edit.text(row, namespace);
    }
    edit.check(index(edit, FADE), case["fade"].as_bool().unwrap());
    assert!(
        matches!(edit.rows()[index(edit,NAMESPACE)],Row::Opt{enabled,..} if enabled==case["namespace_enabled"].as_bool().unwrap_or(!case["fade"].as_bool().unwrap()))
    );
}
fn compare(
    label: &str,
    parts: &[TagText],
    colour_tag: &str,
    colours: &NamespaceColours,
    expected: &Value,
    fade: bool,
) {
    let actual = if parts.is_empty() {
        json!([[label, colours.tag(colour_tag)]])
    } else {
        json!(
            parts
                .iter()
                .map(|run| json!([run.text, run.colour]))
                .collect::<Vec<_>>()
        )
    };
    assert_eq!(actual, expected["rows"][0]);
    let text: String = expected["rows"][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part[0].as_str().unwrap())
        .collect();
    assert_eq!(label, text);
    for (i, part) in parts.iter().enumerate() {
        assert_eq!(
            part.fade,
            fade && expected["can_fade"] == true && i > 0 && part.previous_colour != part.colour
        );
        if i > 0 {
            assert_eq!(part.previous_colour, parts[i - 1].colour);
        }
    }
}
#[test]
fn qt_sibling_colours_stage_cancel_apply_reopen_and_all_live_model_runs() {
    let recorded = hydrus_testkit::fixture_json("sibling_colours.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    fixture::set_preferences(
        &store,
        &json!({"listbook":false,"parents":true,"expanded":true,"siblings":true}),
    );
    let before: SiblingConnectorColours = store.read(settings::get).unwrap();
    assert_eq!(
        json!({"fade":before.fade,"namespace":before.namespace}),
        recorded["options"]["initial"]
    );
    let mut cancelled = editor(&store);
    edit(&mut cancelled, &recorded["options"]["applied"]);
    drop(cancelled);
    assert_eq!(
        store
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap(),
        before
    );
    let manage = ManageTags::new(store.clone(), files.clone()).unwrap();
    let key = ServiceKey::from_hex(recorded["tag_service"].as_str().unwrap()).unwrap();
    let location = LocationContext::single(
        ServiceKey::from_hex(recorded["file_context"][0].as_str().unwrap()).unwrap(),
    );
    let mut input = WriteAutocomplete::new(store.clone(), key.clone(), location.clone());
    input.choose_domain(Choice::Tags(key));
    input.choose_domain(Choice::Location(location));
    for case in recorded["cases"].as_array().unwrap() {
        let prior = store
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap();
        let mut options = editor(&store);
        edit(&mut options, case);
        assert_eq!(
            store
                .read::<SiblingConnectorColours>(settings::get)
                .unwrap(),
            prior
        );
        let (after, original, problems) = options.applied();
        assert!(problems.is_empty());
        let original = original.clone();
        store
            .write(move |ctx| after.save(ctx.conn(), &original))
            .unwrap();
        let saved = store
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap();
        assert_eq!(saved.fade, case["fade"]);
        assert_eq!(json!(saved.namespace), case["namespace"]);
        let reopened = editor(&store);
        let reopened_rows = reopened.rows();
        let Row::Opt { value, .. } = &reopened_rows[index(&reopened, NAMESPACE)] else {
            panic!("missing namespace control")
        };
        let hydrus_gui_model::options::Value::NoneableText { text, .. } = value else {
            panic!("wrong namespace control")
        };
        assert_eq!(text.as_str(), case["reopened_text"].as_str().unwrap());
        let colours: NamespaceColours = store.read(settings::get).unwrap();
        for expected in case["storage"].as_array().unwrap() {
            let rows = manage.display_rows();
            let row = rows.iter().find(|row| row.tag == expected["tag"]).unwrap();
            compare(
                &row.label,
                &row.parts,
                &row.colour_tag,
                &colours,
                expected,
                saved.fade,
            );
        }
        input.set_text(recorded["query"].as_str().unwrap());
        for expected in case["write"].as_array().unwrap() {
            let row = input
                .rows()
                .iter()
                .find(|row| row.tag == expected["tag"])
                .unwrap();
            compare(
                &row.label,
                &row.parts,
                &row.colour_tag,
                &colours,
                expected,
                saved.fade,
            );
        }
        assert!(!manage.has_changes());
        assert_eq!(
            ManageTags::new(store.clone(), files.clone())
                .unwrap()
                .display_rows(),
            manage.display_rows()
        );
        assert_eq!(
            hydrus_store::Store::open(store.dir())
                .unwrap()
                .read::<SiblingConnectorColours>(settings::get)
                .unwrap(),
            saved
        );
    }
}

#[test]
fn qt_collapsed_parent_suffix_keeps_its_own_fade_and_ideal_trailing_colour() {
    let recorded = hydrus_testkit::fixture_json("sibling_colours.json");
    let case = &recorded["collapsed_case"];
    let mut input_fixture = recorded.clone();
    input_fixture["parents"] = case["parents"].clone();
    let (_directory, store, _files) = fixture::seed(&input_fixture);
    store
        .write(|ctx| {
            let mut preferences: hydrus_store::tag_editing::TagEditingSettings =
                settings::get(ctx.conn())?;
            preferences.autocomplete_show_parents = true;
            preferences.autocomplete_expand_parents = false;
            preferences.autocomplete_show_siblings = true;
            settings::set(ctx.conn(), &preferences)?;
            settings::set(
                ctx.conn(),
                &SiblingConnectorColours {
                    fade: true,
                    namespace: None,
                },
            )
        })
        .unwrap();
    let key = ServiceKey::from_hex(recorded["tag_service"].as_str().unwrap()).unwrap();
    let location = LocationContext::single(
        ServiceKey::from_hex(recorded["file_context"][0].as_str().unwrap()).unwrap(),
    );
    let mut input = WriteAutocomplete::new(store.clone(), key.clone(), location.clone());
    input.choose_domain(Choice::Tags(key));
    input.choose_domain(Choice::Location(location));
    input.set_text(recorded["query"].as_str().unwrap());
    let colours: NamespaceColours = store.read(settings::get).unwrap();
    for expected in case["write"].as_array().unwrap() {
        let row = input
            .rows()
            .iter()
            .find(|row| row.tag == expected["tag"])
            .unwrap();
        compare(
            &row.label,
            &row.parts,
            &row.colour_tag,
            &colours,
            expected,
            true,
        );
        assert!(row.parts.last().unwrap().fade);
    }
    let paints = case["selected_paints"].as_array().unwrap();
    assert_eq!(paints[0]["gradient_extent"], paints[1]["gradient_extent"]);
    assert_eq!(paints[0]["trailing_colour"], json!([0, 170, 0]));
    assert!(
        paints
            .iter()
            .all(|paint| paint["trailing_is_solid"] == true)
    );
}
