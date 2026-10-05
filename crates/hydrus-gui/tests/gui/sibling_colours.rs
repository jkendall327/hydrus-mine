//! Real Options staging and segmented colours in all existing sibling-display consumers.
use super::tag_dialog_preferences::fixture;
use hydrus_core::{ServiceKey, Tag, tag_presentation::SiblingConnectorColours};
use hydrus_gui::{
    ListText, MainWindow, OptionsWindow, Pages, SearchPage, WriteTagsWindow, bind, headless,
};
use hydrus_store::settings;
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _, ModelRc};
use std::rc::Rc;
const FADE: &str = "Fade the colour of the sibling connector string on Qt6: ";
const NAMESPACE: &str = "Namespace for the colour of the sibling connecting string: ";
fn menu(ui: &MainWindow, title: &str, item: &str) {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == title)
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let row = lines
        .iter()
        .position(|row| row.label.starts_with(item))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
}
fn index(window: &OptionsWindow, label: &str) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap(),
    )
    .unwrap()
}
fn edit(window: &OptionsWindow, case: &Value) {
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "tag presentation")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    window.invoke_check_toggled(index(window, FADE), false);
    window.invoke_none_toggled(index(window, NAMESPACE), case["namespace"].is_null());
    if let Some(namespace) = case["namespace"].as_str() {
        window.invoke_text_edited(index(window, NAMESPACE), namespace.into());
    }
    window.invoke_check_toggled(index(window, FADE), case["fade"].as_bool().unwrap());
    assert_eq!(
        window
            .get_rows()
            .row_data(usize::try_from(index(window, NAMESPACE)).unwrap())
            .unwrap()
            .enabled,
        !case["fade"].as_bool().unwrap()
    );
}
fn source(window: &WriteTagsWindow, tags: bool, label: &str) {
    window.invoke_domain_menu(tags, 0.0, 0.0);
    let lines = window.get_tag_menu_panes().row_data(0).unwrap().lines;
    let row = lines.iter().position(|row| row.label == label).unwrap();
    window.invoke_tag_menu_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
}
fn rgb(colour: slint::Color) -> [u8; 3] {
    [colour.red(), colour.green(), colour.blue()]
}
fn compare(rows: &ModelRc<ListText>, expected: &Value, fade: bool) {
    for expected in expected.as_array().unwrap() {
        let first = expected["rows"][0][0][0].as_str().unwrap();
        let row = rows.iter().find(|row| row.text.starts_with(first)).unwrap();
        let actual = if row.parts.row_count() == 0 {
            json!([[row.text.as_str(), rgb(row.colour)]])
        } else {
            json!(
                row.parts
                    .iter()
                    .map(|part| json!([part.text.as_str(), rgb(part.colour)]))
                    .collect::<Vec<_>>()
            )
        };
        assert_eq!(actual, expected["rows"][0]);
        for (i, part) in row.parts.iter().enumerate() {
            assert_eq!(
                part.fade,
                fade && expected["can_fade"] == true
                    && i > 0
                    && part.previous_colour != part.colour
            );
        }
    }
}
#[test]
fn options_cancel_retired_apply_reopen_and_all_segmented_native_consumers() {
    let recorded = hydrus_testkit::fixture_json("sibling_colours.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    fixture::set_preferences(
        &store,
        &json!({"listbook":false,"parents":true,"expanded":true,"siblings":true}),
    );
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "sibling colours",
            None,
            files,
        )),
    );
    ui.invoke_select_all();
    let before: SiblingConnectorColours = store.read(settings::get).unwrap();
    menu(&ui, "file", "options");
    let cancelled = bound.options.borrow().as_ref().unwrap().clone_strong();
    edit(&cancelled, &recorded["options"]["applied"]);
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert_eq!(
        store
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap(),
        before
    );
    let key = ServiceKey::from_hex(recorded["tag_service"].as_str().unwrap()).unwrap();
    let file_key = ServiceKey::from_hex(recorded["file_context"][0].as_str().unwrap()).unwrap();
    let tag_label = store.snapshot().services.by_key(&key).unwrap().name.clone();
    let file_label = store
        .snapshot()
        .services
        .by_key(&file_key)
        .unwrap()
        .name
        .clone();
    for (case_index, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        menu(&ui, "file", "options");
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        let prior = store
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap();
        edit(&options, case);
        assert_eq!(
            store
                .read::<SiblingConnectorColours>(settings::get)
                .unwrap(),
            prior
        );
        options.invoke_apply();
        let saved = store
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap();
        assert_eq!(saved.fade, case["fade"]);
        assert_eq!(json!(saved.namespace), case["namespace"]);
        ui.invoke_manage_tags_selected();
        let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        compare(&manage.get_tags(), &case["storage"], saved.fade);
        manage.invoke_cancel();
        let slot = hydrus_gui::write_tag_window::Slot::default();
        let native_index = windows.count();
        let child = hydrus_gui::write_tag_window::open(
            &store,
            key.clone(),
            &[],
            "sibling colours",
            &slot,
            Rc::new(|_| {}),
            Rc::new(|| {}),
        )
        .unwrap();
        source(&child, true, &tag_label);
        source(&child, false, &file_label);
        child.invoke_edited(recorded["query"].as_str().unwrap().into());
        child.invoke_fetch();
        compare(&child.get_suggestions(), &case["write"], saved.fade);
        if case_index == 0 || case_index == 2 {
            let native = windows.get(native_index).unwrap();
            let pixels = headless::render(&native, 760, 650);
            assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join(format!("sibling-colours-{case_index}-unselected.png")),
                &pixels,
                760,
                650,
            )
            .expect("save the sibling colour evidence");
            let alias = child
                .get_suggestions()
                .iter()
                .position(|row| row.text.starts_with("creator:parity alias"))
                .unwrap();
            child.invoke_selection_clicked(i32::try_from(alias).unwrap(), false, false);
            assert_eq!(
                child.get_selected().iter().collect::<Vec<_>>(),
                (0..child.get_suggestions().row_count())
                    .map(|row| row == alias)
                    .collect::<Vec<_>>(),
                "the actual selected alias owns its segmented paint"
            );
            let pixels = headless::render(&native, 760, 650);
            headless::save_png(
                &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join(format!("sibling-colours-{case_index}-selected.png")),
                &pixels,
                760,
                650,
            )
            .expect("save the sibling colour evidence before checking its pixels");
            let gradient_colours: std::collections::BTreeSet<_> = pixels
                .chunks_exact(4)
                .filter(|pixel| {
                    pixel[2] == 0
                        && pixel[0] > 5
                        && pixel[1] > 5
                        && (169..=171).contains(&(u16::from(pixel[0]) + u16::from(pixel[1])))
                })
                .map(|pixel| (pixel[0], pixel[1]))
                .collect();
            if case_index == 0 {
                assert!(
                    gradient_colours.len() >= 4,
                    "selected real connector paints a red-to-green gradient"
                );
            } else {
                assert!(
                    gradient_colours.is_empty(),
                    "custom namespace is solid when fade is off"
                );
            }
        }
        child.invoke_cancel();
        child.invoke_chosen(0);
        child.invoke_apply();
        assert!(slot.borrow().is_none());
        for kind in ["siblings", "parents"] {
            menu(&ui, "tags", kind);
            let relationship = bound
                .tag_relationships
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            for right in [false, true] {
                for (tags, label) in [(true, &tag_label), (false, &file_label)] {
                    relationship.invoke_domain_menu(right, tags, 0.0, 0.0);
                    let lines = relationship.get_tag_menu_panes().row_data(0).unwrap().lines;
                    let row = lines.iter().position(|row| row.label == label).unwrap();
                    relationship.invoke_tag_menu_clicked(
                        0,
                        i32::try_from(row).unwrap(),
                        0.0,
                        0.0,
                        0.0,
                    );
                }
                relationship
                    .invoke_autocomplete_edited(right, recorded["query"].as_str().unwrap().into());
                relationship.invoke_autocomplete_fetch(right);
                compare(
                    &if right {
                        relationship.get_right_suggestions()
                    } else {
                        relationship.get_left_suggestions()
                    },
                    &case["write"],
                    saved.fade,
                );
            }
            relationship.invoke_cancel();
        }
        menu(&ui, "file", "options");
        let reopened = bound.options.borrow().as_ref().unwrap().clone_strong();
        let page = reopened
            .get_pages()
            .iter()
            .position(|page| page.text == "tag presentation")
            .unwrap();
        reopened.invoke_page_chosen(i32::try_from(page).unwrap());
        let row = reopened
            .get_rows()
            .row_data(usize::try_from(index(&reopened, NAMESPACE)).unwrap())
            .unwrap();
        assert_eq!(row.is_none, case["namespace"].is_null());
        assert_eq!(row.text, case["reopened_text"].as_str().unwrap());
        reopened.invoke_cancel();
        assert_eq!(
            store
                .read::<SiblingConnectorColours>(settings::get)
                .unwrap(),
            saved
        );
    }
    // A visible editor observes another owner's preference change without retyping.
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let child = hydrus_gui::write_tag_window::open(
        &store,
        key.clone(),
        &[],
        "live sibling colours",
        &slot,
        Rc::new(|_| {}),
        Rc::new(|| {}),
    )
    .unwrap();
    source(&child, true, &tag_label);
    source(&child, false, &file_label);
    child.invoke_edited(recorded["query"].as_str().unwrap().into());
    child.invoke_fetch();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let draft = child.get_text();
    let selected: Vec<_> = child.get_selected().iter().collect();
    let snapshot_revision = store.snapshot().revision;
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &SiblingConnectorColours {
                    fade: false,
                    namespace: Some("system".into()),
                },
            )
        })
        .unwrap();
    assert_eq!(
        store.snapshot().revision,
        snapshot_revision,
        "plain colour preference commits do not republish the Store snapshot"
    );
    assert_eq!(
        store
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap(),
        SiblingConnectorColours {
            fade: false,
            namespace: Some("system".into())
        },
        "the observer must notice the durable policy independently of snapshot revision"
    );
    let has_custom_connector = |rows: &ModelRc<ListText>| {
        rows.iter().any(|row| {
            row.parts
                .iter()
                .any(|part| part.text == " → " && rgb(part.colour) == [153, 101, 21])
        })
    };
    for _ in 0..100 {
        slint::platform::update_timers_and_animations();
        // Each visible owner starts its own timer. The earlier Write Tags tick
        // does not imply the later Manage Tags observer has repainted yet.
        if has_custom_connector(&child.get_suggestions())
            && has_custom_connector(&manage.get_tags())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    compare(
        &child.get_suggestions(),
        &recorded["cases"][2]["write"],
        false,
    );
    compare(&manage.get_tags(), &recorded["cases"][2]["storage"], false);
    assert_eq!(child.get_text(), draft);
    assert_eq!(child.get_selected().iter().collect::<Vec<_>>(), selected);
    child.invoke_cancel();
    manage.invoke_cancel();
    // A selected fading final parent suffix stays fixed as the viewport widens.
    let collapsed = &recorded["collapsed_case"];
    let service = store.snapshot().services.by_key(&key).unwrap().id;
    hydrus_store::content::tag_relations::apply(
        &store,
        hydrus_store::display::RelationKind::Parents,
        collapsed["parents"]
            .as_array()
            .unwrap()
            .iter()
            .map(
                |pair| hydrus_store::content::tag_relations::RelationUpdate {
                    service,
                    left: Tag::new(pair[0].as_str().unwrap()).unwrap(),
                    right: Tag::new(pair[1].as_str().unwrap()).unwrap(),
                    action: hydrus_store::content::tag_relations::RelationAction::Add,
                },
            )
            .collect(),
    )
    .unwrap();
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
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let native_index = windows.count();
    let child = hydrus_gui::write_tag_window::open(
        &store,
        key.clone(),
        &[],
        "collapsed parent colours",
        &slot,
        Rc::new(|_| {}),
        Rc::new(|| {}),
    )
    .unwrap();
    source(&child, true, &tag_label);
    source(&child, false, &file_label);
    child.invoke_edited(recorded["query"].as_str().unwrap().into());
    child.invoke_fetch();
    compare(&child.get_suggestions(), &collapsed["write"], true);
    let alias = child
        .get_suggestions()
        .iter()
        .position(|row| row.text.starts_with("creator:parity alias"))
        .unwrap();
    child.invoke_selection_clicked(i32::try_from(alias).unwrap(), false, false);
    let native = windows.get(native_index).unwrap();
    let mut extents = Vec::new();
    let expected_trailing: [u8; 3] =
        serde_json::from_value(collapsed["selected_paints"][0]["trailing_colour"].clone()).unwrap();
    for width in [760_u32, 1100] {
        let pixels = headless::render(&native, width, 650);
        let y = child.get_results_y().floor() as usize + 2 + alias * 22 + 2;
        let row = &pixels[y * width as usize * 4..(y + 1) * width as usize * 4];
        // The selected row has a 4px palette-coloured strip outside each
        // PaintedTagText edge. That blue is not a sibling gradient. Sample
        // the actual text consumer: 2px row padding plus its 4px inset.
        let left = child.get_results_x() + 6.0;
        let right = child.get_results_x() + child.get_results_width() - 6.0;
        let gradient: Vec<_> = row
            .chunks_exact(4)
            .enumerate()
            .filter(|(x, pixel)| {
                (*x as f32) >= left
                    && (*x as f32) < right
                    && pixel[0] == 0
                    && pixel[2] > 0
                    && pixel[2] < 250
                    && pixel[1] < 170
            })
            .map(|(x, _)| x)
            .collect();
        assert!(
            !gradient.is_empty(),
            "collapsed parent suffix paints its own green-to-blue fade"
        );
        extents.push((*gradient.first().unwrap(), *gradient.last().unwrap()));
        for x in width as usize - 100..width as usize - 30 {
            assert_eq!(
                &row[x * 4..x * 4 + 3],
                expected_trailing.as_slice(),
                "Qt keeps the preceding ideal solid beyond the fading suffix"
            );
        }
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("sibling-colours-collapsed-{width}.png")),
            &pixels,
            width,
            650,
        )
        .expect("save the sibling colour evidence");
    }
    assert_eq!(
        extents[0], extents[1],
        "a wider viewport cannot stretch the final text/background fade"
    );
    child.invoke_cancel();
    assert_eq!(
        hydrus_store::Store::open(store.dir())
            .unwrap()
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap(),
        store
            .read::<SiblingConnectorColours>(settings::get)
            .unwrap()
    );
}
