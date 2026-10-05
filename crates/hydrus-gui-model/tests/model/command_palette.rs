//! Live Qt provider output and queue editing, without GUI-thread dependencies.
use hydrus_core::pages::{FavouriteSearch, PageKey};
use hydrus_gui_model::command_palette::{
    self as palette, Action, CommandPaletteSettings, MenuItem, OpenPage, Provider, ProviderOrder,
    Results, Snapshot, Suggestion,
};
use serde_json::{Value, json};

fn fixture() -> Value {
    hydrus_testkit::fixture_json("command_palette.json")
}
fn key(number: u8) -> PageKey {
    PageKey([number; 32])
}
fn favourite(folder: Option<&str>, name: &str) -> FavouriteSearch {
    FavouriteSearch {
        folder: folder.map(str::to_owned),
        name: name.to_owned(),
        search: hydrus_core::search::context::FileSearchContext::default(),
        synchronised: folder.is_none(),
        sort: None,
        collect: None,
    }
}
fn snapshot() -> Snapshot {
    Snapshot {
        pages: vec![
            OpenPage {
                key: key(1),
                name: "Palette Alpha".into(),
                parent_name: None,
                notebook: false,
            },
            OpenPage {
                key: key(2),
                name: "Palette Nested".into(),
                parent_name: None,
                notebook: true,
            },
            OpenPage {
                key: key(3),
                name: "Palette Beta".into(),
                parent_name: Some("Palette Nested".into()),
                notebook: false,
            },
            OpenPage {
                key: key(4),
                name: "Palette Gamma".into(),
                parent_name: None,
                notebook: false,
            },
        ],
        history: vec![
            (key(1), "Palette Alpha".into()),
            (key(3), "Palette Beta".into()),
            (key(4), "Palette Gamma".into()),
        ],
        favourites: vec![
            favourite(Some("Palette Folder"), "Favourite Alpha"),
            favourite(None, "Favourite Beta"),
        ],
        main_menu: vec![
            MenuItem {
                label: "Palette &Launch".into(),
                parent: "Palette Synthetic".into(),
                checked: None,
                action: Some(Action::MainMenu(
                    hydrus_gui_model::main_menu::Command::Copy("launch".into()),
                )),
            },
            MenuItem {
                label: "Palette checked".into(),
                parent: "Palette Synthetic".into(),
                checked: Some(true),
                action: None,
            },
            MenuItem {
                label: "Palette disabled".into(),
                parent: "Palette Synthetic".into(),
                checked: None,
                action: None,
            },
        ],
        ..Default::default()
    }
}
fn plain(text: &str) -> String {
    text.replace("<b>", "")
        .replace("</b>", "")
        .replace("<i>", "")
        .replace("</i>", "")
}
fn row_json(row: &Suggestion) -> Value {
    json!({"text":[row.primary,row.secondary],"toggled":row.checked.unwrap_or(false),"close":row.action!=Some(Action::Calculator)})
}

#[test]
fn providers_match_real_qt_order_limits_and_thresholds() {
    let oracle = fixture();
    let data = snapshot();
    let mut settings = CommandPaletteSettings {
        initially_show_favourites: true,
        show_main_menu: true,
        ..Default::default()
    };
    for (index, event) in oracle["events"].as_array().unwrap().iter().enumerate() {
        let code = usize::try_from(event["provider"].as_u64().unwrap()).unwrap();
        match index {
            10 => settings.show_notebooks = true,
            11 => settings.page_limit = Some(1),
            12 => {
                settings.page_limit = None;
                settings.history_limit = Some(1);
            }
            13 => {
                settings.history_limit = Some(10);
                settings.favourite_limit = Some(1);
            }
            14 => {
                settings.favourite_limit = None;
                settings.threshold = 4;
            }
            _ => {}
        }
        let provider = Provider::from_code(code).unwrap();
        let rows = palette::query(provider, event["query"].as_str().unwrap(), &settings, &data);
        let ours: Vec<_> = rows.iter().map(row_json).collect();
        let mut theirs = event["rows"].as_array().unwrap().clone();
        for row in &mut theirs {
            for text in row["text"].as_array_mut().unwrap() {
                *text = plain(text.as_str().unwrap()).into();
            }
        }
        assert_eq!(ours, theirs, "provider {provider:?}, event {index}");
    }
    let rows = palette::query(Provider::Pages, "Palette Alpha", &settings, &data);
    assert_eq!(rows[0].action, Some(Action::Page(key(1))));
    let rows = palette::query(Provider::Favourites, "Favourite Alpha", &settings, &data);
    assert_eq!(
        rows[0].action,
        Some(Action::Favourite(Box::new(data.favourites[0].clone())))
    );
}

#[test]
fn defaults_and_provider_queue_match_the_qt_options_editor() {
    let oracle = fixture();
    let settings = CommandPaletteSettings::default();
    assert_eq!(
        settings.initially_show_pages,
        oracle["initial"]["initially_show_all_pages"]
    );
    assert_eq!(
        settings.initially_show_history,
        oracle["initial"]["initially_show_history"]
    );
    assert_eq!(
        settings.initially_show_favourites,
        oracle["initial"]["initially_show_favourite_searches"]
    );
    assert_eq!(
        settings.show_notebooks,
        oracle["initial"]["show_page_of_pages"]
    );
    assert_eq!(
        settings.favourites_new_page,
        oracle["initial"]["fav_searches_open_new_page"]
    );
    assert_eq!(settings.history_limit, Some(10));
    assert_eq!(settings.page_limit, None);
    assert_eq!(settings.favourite_limit, None);
    let codes = |order: &[Provider]| {
        order
            .iter()
            .map(|p| Provider::ALL.iter().position(|v| v == p).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        json!(codes(&settings.provider_order)),
        oracle["initial"]["order"]
    );
    let mut editor = ProviderOrder::new(settings.provider_order.clone());
    editor.click(0, false, false);
    editor.move_selected(true);
    assert_eq!(json!(codes(&editor.order)), oracle["queue_events"][0]);
    assert_eq!(editor.removal_question().unwrap(), oracle["questions"][0]);
    editor.remove_selected();
    assert_eq!(json!(codes(&editor.order)), oracle["queue_events"][1]);
    assert_eq!(editor.missing(), [Provider::Calculator]);
    assert!(!editor.add(None));
    assert_eq!(json!(codes(&editor.order)), oracle["queue_events"][2]);
    assert!(editor.add(Some(Provider::Calculator)));
    assert_eq!(json!(codes(&editor.order)), oracle["persisted"]["order"]);
    assert!(!editor.add(Some(Provider::Calculator)));
    assert!(editor.missing().is_empty());
    assert_eq!(
        settings.provider_order[0],
        Provider::Calculator,
        "editing must not mutate the owner's original preferences"
    );
}

#[test]
fn late_workers_cannot_repopulate_a_new_query_or_closed_owner() {
    let data = snapshot();
    let settings = CommandPaletteSettings::default();
    let mut results = Results::default();
    let old = results.begin(&settings.provider_order);
    let current = results.begin(&[Provider::Pages, Provider::History]);
    assert!(!results.accept(
        old,
        Provider::Pages,
        palette::query(Provider::Pages, "Alpha", &settings, &data)
    ));
    assert!(results.accept(
        current,
        Provider::History,
        palette::query(Provider::History, "Palette", &settings, &data)
    ));
    assert!(results.accept(
        current,
        Provider::Pages,
        palette::query(Provider::Pages, "Palette", &settings, &data)
    ));
    assert_eq!(
        results.ordered()[0].0,
        Provider::Pages,
        "worker arrival order must not change provider order"
    );
    assert_eq!(results.ordered().len(), 6);
    assert!(!results.accept(current, Provider::Favourites, Vec::new()));
    results.close();
    assert!(results.ordered().is_empty());
    assert!(!results.accept(
        current,
        Provider::Pages,
        palette::query(Provider::Pages, "Palette", &settings, &data)
    ));
}

#[test]
fn each_live_provider_policy_reaches_its_results_and_preferences_persist() {
    let data = snapshot();
    let mut settings = CommandPaletteSettings::default();
    assert_eq!(
        palette::query(Provider::Pages, "", &settings, &data).len(),
        3
    );
    settings.initially_show_pages = false;
    assert!(palette::query(Provider::Pages, "", &settings, &data).is_empty());
    settings.initially_show_history = false;
    assert!(palette::query(Provider::History, "", &settings, &data).is_empty());
    assert!(palette::query(Provider::Favourites, "", &settings, &data).is_empty());
    settings.initially_show_favourites = true;
    assert_eq!(
        palette::query(Provider::Favourites, "", &settings, &data).len(),
        2
    );
    assert!(palette::query(Provider::MainMenu, "Palette", &settings, &data).is_empty());
    settings.show_main_menu = true;
    assert_eq!(
        palette::query(Provider::MainMenu, "Palette", &settings, &data).len(),
        3
    );
    let mut without_history = data.clone();
    without_history.history.clear();
    assert!(palette::query(Provider::MainMenu, "Palette", &settings, &without_history).is_empty());
    settings.provider_order.retain(|p| *p != Provider::Pages);
    assert!(palette::query(Provider::Pages, "Palette", &settings, &data).is_empty());
    let directory = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    let written = settings.clone();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &written))
        .unwrap();
    let reopened = store
        .read(hydrus_store::settings::get::<CommandPaletteSettings>)
        .unwrap();
    assert_eq!(reopened, settings);
}

#[test]
fn calculator_matches_qt_precedence_integer_types_errors_and_special_values() {
    let oracle = fixture();
    let settings = CommandPaletteSettings {
        threshold: 64,
        ..Default::default()
    };
    for event in oracle["calculator_events"].as_array().unwrap() {
        let query = event["query"].as_str().unwrap();
        let rows = palette::query(Provider::Calculator, query, &settings, &Snapshot::default());
        let expected = event["rows"].as_array().unwrap();
        assert_eq!(rows.len(), expected.len(), "{query}");
        if expected.is_empty() {
            continue;
        }
        assert_eq!(rows[0].secondary, "Calculator");
        assert_eq!(rows[0].action, Some(Action::Calculator));
        if query == "random()" {
            let value: f64 = rows[0].primary.parse().unwrap();
            assert!((0.0..1.0).contains(&value));
        } else if matches!(
            query.split_once('(').map(|(name, _)| name),
            Some(
                "exp"
                    | "log"
                    | "log2"
                    | "log10"
                    | "acos"
                    | "asin"
                    | "atan"
                    | "cos"
                    | "sin"
                    | "tan"
                    | "acosh"
                    | "asinh"
                    | "atanh"
                    | "cosh"
                    | "sinh"
                    | "tanh"
                    | "erf"
                    | "erfc"
                    | "gamma"
                    | "lgamma"
            )
        ) && !query.contains("inf")
        {
            // These finite transcendental results use platform libm (and
            // Python's own Lanczos gamma). Only their final floating bits are
            // portable within four ULPs or four relative machine epsilons.
            // Zero/sign, special values and integer/arithmetic display stay exact.
            let expected_text = expected[0]["text"][0].as_str().unwrap();
            let ours: f64 = rows[0].primary.parse().unwrap();
            let theirs: f64 = expected_text.parse().unwrap();
            if theirs.is_finite() && theirs.classify() != std::num::FpCategory::Zero {
                assert!(ours.is_finite(), "{query}: {ours}");
                assert_eq!(
                    ours.is_sign_negative(),
                    theirs.is_sign_negative(),
                    "{query}"
                );
                assert!(
                    rows[0].primary.contains('.') || rows[0].primary.contains('e'),
                    "{query}: float result type"
                );
                let ulps = ours.to_bits().abs_diff(theirs.to_bits());
                assert!(
                    ulps <= 4 || (ours - theirs).abs() <= theirs.abs() * 4.0 * f64::EPSILON,
                    "{query}: {ours} / {theirs}, {ulps} ULPs"
                );
            } else {
                assert_eq!(rows[0].primary, expected_text, "{query}");
            }
        } else {
            assert_eq!(rows[0].primary, expected[0]["text"][0], "{query}");
        }
    }
    let mut removed = settings;
    removed
        .provider_order
        .retain(|p| *p != Provider::Calculator);
    assert!(palette::query(Provider::Calculator, "2+2", &removed, &Snapshot::default()).is_empty());
}

#[test]
fn unicode_casefold_expands_before_provider_and_menu_thresholds() {
    let name = "Unicode Straße ffi ff";
    let data = Snapshot {
        pages: vec![OpenPage {
            key: key(1),
            name: name.into(),
            parent_name: None,
            notebook: false,
        }],
        history: vec![(key(1), name.into())],
        favourites: vec![favourite(None, name)],
        main_menu: vec![MenuItem {
            label: name.into(),
            parent: "Palette Synthetic".into(),
            checked: None,
            action: None,
        }],
        ..Default::default()
    };
    for event in fixture()["unicode_events"].as_array().unwrap() {
        let settings = CommandPaletteSettings {
            threshold: event["threshold"].as_u64().unwrap() as usize,
            show_main_menu: true,
            ..Default::default()
        };
        let provider = Provider::from_code(event["provider"].as_u64().unwrap() as usize).unwrap();
        let rows = palette::query(provider, event["query"].as_str().unwrap(), &settings, &data);
        let ours: Vec<_> = rows.iter().map(|row| row.primary.as_str()).collect();
        let theirs: Vec<_> = event["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| plain(row["text"][0].as_str().unwrap()))
            .collect();
        assert_eq!(ours, theirs, "{event}");
    }
}
