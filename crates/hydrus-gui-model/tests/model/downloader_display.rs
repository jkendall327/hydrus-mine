//! Actual Qt fixture replay, concurrent edits and URL display consumers.
use hydrus_core::url::{AnyGug, DomainMask, Gug, Gugs, UrlClass, UrlClassSettings, UrlClasses};
use hydrus_gui_model::downloader_display::{self as model, Draft, ViewerUrls};
use hydrus_parse::Downloaders;
use hydrus_store::{Store, settings};

fn definitions() -> (Downloaders, UrlClassSettings) {
    let d = Downloaders {
        gugs: Gugs {
            gugs: [("alpha", 17), ("beta", 34)]
                .into_iter()
                .map(|(name, key)| {
                    AnyGug::Single(Gug {
                        name: name.into(),
                        key: hex::encode([key; 32]),
                        url_template: "https://example.com/search?q=%tags%".into(),
                        replacement_phrase: "%tags%".into(),
                        separator: "+".into(),
                        initial_search_text: String::new(),
                        example_search_text: String::new(),
                    })
                })
                .collect(),
            keys_to_display: vec![hex::encode([17; 32])],
        },
        ..Downloaders::default()
    };
    let classes = UrlClassSettings {
        url_classes: [
            ("example post", 51, vec!["example.com"]),
            ("multi post", 68, vec!["a.example", "b.example"]),
        ]
        .into_iter()
        .map(|(name, key, domains)| UrlClass {
            name: name.into(),
            key: vec![key; 32],
            domain_mask: DomainMask::new(
                domains.into_iter().map(str::to_owned).collect(),
                vec![],
                false,
                false,
            ),
            ..UrlClass::default()
        })
        .collect(),
        ..UrlClassSettings::default()
    };
    (d, classes)
}
fn rows(d: &Draft) -> serde_json::Value {
    serde_json::json!({"gugs":d.gugs.iter().map(|r|r.cells(false)).collect::<Vec<_>>(),"classes":d.classes.iter().map(|r|r.cells(true)).collect::<Vec<_>>()})
}
#[test]
fn replay_actual_tri_state_questions_and_rows() {
    let f = hydrus_testkit::fixture_json("downloader_display.json");
    let (d, c) = definitions();
    let mut draft = Draft::new(&d, &c, &ViewerUrls::default());
    assert_eq!(rows(&draft), f["initial"]);
    for (i, (classes, answer)) in [
        (false, None),
        (false, Some(true)),
        (false, Some(false)),
        (true, None),
        (true, Some(false)),
        (true, Some(true)),
    ]
    .into_iter()
    .enumerate()
    {
        let (title, message) = model::question(draft.rows(classes), &[0, 1], classes);
        assert_eq!(title, f["questions"][i]["title"]);
        assert_eq!(message, f["questions"][i]["message"]);
        draft.answer(classes, &[0, 1], answer);
        assert_eq!(rows(&draft), f["steps"][i]);
    }
    assert_eq!(
        model::question(&draft.gugs, &[0], false).1,
        "Show \"alpha\" in the main selector list?"
    );
}
#[test]
fn displayed_url_labels_filters_order_and_cap_replay_reference() {
    let f = hydrus_testkit::fixture_json("downloader_display.json");
    let (_, c) = definitions();
    let classes = UrlClasses::new(c);
    let urls: Vec<String> = serde_json::from_value(f["urls"].clone()).unwrap();
    for (i, (keys, show_unmatched)) in [
        (vec![hex::encode([51; 32])], true),
        (vec![hex::encode([68; 32])], true),
        (vec![], false),
    ]
    .into_iter()
    .enumerate()
    {
        let viewer = ViewerUrls {
            class_keys_to_display: Some(keys),
            show_unmatched,
        };
        assert_eq!(
            serde_json::to_value(model::viewer_links(&urls, &classes, &viewer)).unwrap(),
            f["viewer"][i]
        );
    }
    let cap_urls = serde_json::from_value::<Vec<String>>(f["cap_urls"].clone()).unwrap();
    let viewer = ViewerUrls {
        class_keys_to_display: Some(vec![hex::encode([51; 32])]),
        show_unmatched: true,
    };
    assert_eq!(
        serde_json::to_value(model::viewer_links(&cap_urls, &classes, &viewer)).unwrap(),
        f["cap"]
    );
}
#[test]
fn save_merges_only_edited_keys_and_preserves_concurrent_definitions() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let (d, c) = definitions();
    store
        .write_and_refresh(move |ctx| {
            settings::set(ctx.conn(), &d)?;
            settings::set(ctx.conn(), &c)
        })
        .unwrap();
    let mut draft = Draft::load(&store).unwrap();
    draft.answer(false, &[0], Some(false));
    draft.answer(false, &[1], Some(true));
    draft.answer(true, &[0], Some(false));
    draft.show_unmatched = false;
    // Cancelling merely drops a draft; persistent values have not changed.
    assert!(Draft::load(&store).unwrap().gugs[0].display);
    store
        .write_and_refresh(|ctx| {
            let mut d: Downloaders = settings::get(ctx.conn())?;
            if let AnyGug::Single(g) = &mut d.gugs.gugs[0] {
                g.name = "renamed concurrently".into();
            }
            d.gugs.keys_to_display.push("unrelated-key".into());
            settings::set(ctx.conn(), &d)?;
            let mut c: UrlClassSettings = settings::get(ctx.conn())?;
            c.parser_links
                .push(("concurrent".into(), Some("parser".into())));
            c.url_classes.push(UrlClass {
                name: "new concurrently".into(),
                key: vec![85; 32],
                ..UrlClass::default()
            });
            settings::set(ctx.conn(), &c)
        })
        .unwrap();
    draft.save(&store).unwrap();
    let reopened = Draft::load(&store).unwrap();
    assert!(!reopened.show_unmatched);
    assert!(
        reopened
            .gugs
            .iter()
            .any(|r| r.name == "renamed concurrently" && !r.display)
    );
    assert!(reopened.gugs.iter().any(|r| r.name == "beta" && r.display));
    assert!(
        !reopened
            .classes
            .iter()
            .find(|r| r.key == hex::encode([51; 32]))
            .unwrap()
            .display
    );
    assert!(
        reopened
            .classes
            .iter()
            .find(|r| r.key == hex::encode([85; 32]))
            .unwrap()
            .display
    );
    let d: Downloaders = store.read(settings::get).unwrap();
    assert!(d.gugs.keys_to_display.contains(&"unrelated-key".into()));
    let c: UrlClassSettings = store.read(settings::get).unwrap();
    assert_eq!(
        c.parser_links,
        vec![("concurrent".into(), Some("parser".into()))]
    );
}
