//! Actual Qt chooser order, domain gating and every offered page-query key.
use hydrus_core::ServiceKey;
use hydrus_gui_model::page_chooser::{NewPage, PageChooser};
use hydrus_store::{
    Store, services,
    settings::{self, PageChooserSettings},
};

// leaf: audit-options-gui-pages-opening-and-closing-in-new-page-chooser-show-combined-local-file-domains-if-appropriate
// leaf: audit-options-gui-pages-opening-and-closing-in-new-page-chooser-show-hydrus-local-file-storage
// leaf: audit-options-gui-pages-opening-and-closing-put-it-at-the-top
#[test]
fn all_flag_combinations_replay_real_service_order_layout_and_choices() {
    let fixture = hydrus_testkit::fixture_json("page_chooser_options.json");
    for count in [1, 2, 10] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let domains = fixture["services"].as_array().unwrap()[..count].to_vec();
        store
            .write_and_refresh(move |ctx| {
                ctx.conn().execute(
                    "UPDATE services SET name = 'domain 01' WHERE service_key = ?",
                    [ServiceKey::new(
                        hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
                    )],
                )?;
                for domain in domains.iter().skip(1) {
                    services::insert(
                        ctx.conn(),
                        &ServiceKey::from_hex(domain["key"].as_str().unwrap()).unwrap(),
                        domain["name"].as_str().unwrap(),
                        &services::ServiceKind::LocalFiles,
                    )?;
                }
                Ok(())
            })
            .unwrap();
        for step in fixture["steps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|step| step["count"].as_u64().unwrap() == count as u64)
        {
            let flags = step["flags"].as_array().unwrap();
            let config = PageChooserSettings {
                show_combined: flags[0].as_bool().unwrap(),
                combined_at_top: flags[1].as_bool().unwrap(),
                show_storage: flags[2].as_bool().unwrap(),
                storage_at_top: flags[3].as_bool().unwrap(),
            };
            store
                .write(move |ctx| settings::set(ctx.conn(), &config))
                .unwrap();
            let mut chooser = PageChooser::new(&store);
            chooser.press(8);
            let expected: Vec<_> = step["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(chooser.labels().as_slice(), expected, "{step}");
            assert_eq!(chooser.press(0), None);
            assert_eq!(chooser.press(10), None);
            for choice in step["choices"].as_array().unwrap() {
                let Some(NewPage::Search { domain, name }) =
                    chooser.press(usize::try_from(choice["button"].as_u64().unwrap()).unwrap())
                else {
                    panic!("recorded page-query choice");
                };
                assert_eq!(domain.to_hex(), choice["key"].as_str().unwrap());
                assert_eq!(
                    name,
                    expected[usize::try_from(choice["button"].as_u64().unwrap()).unwrap() - 1]
                );
                assert_eq!(choice["current"], serde_json::json!([domain.to_hex()]));
                assert_eq!(choice["deleted"], serde_json::json!([]));
            }
            // Closing and reopening never commits or clears a hidden top choice.
            assert_eq!(
                store.read(settings::get::<PageChooserSettings>).unwrap(),
                config
            );
            let mut reopened = PageChooser::new(&store);
            reopened.press(8);
            assert_eq!(reopened.labels(), chooser.labels());
        }
    }
}

// leaf: audit-options-gui-pages-opening-and-closing-put-it-at-the-top
#[test]
fn legacy_checkbox_defaults_are_imported_and_top_choices_remain_independent() {
    let fixture = hydrus_testkit::fixture_json("page_chooser_options.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let config = store.read(settings::get::<PageChooserSettings>).unwrap();
    assert_eq!(
        [
            config.show_combined,
            config.combined_at_top,
            config.show_storage,
            config.storage_at_top
        ],
        std::array::from_fn::<_, 4, _>(|i| fixture["controls"][i]["default"].as_bool().unwrap())
    );
    let settings = store
        .read(hydrus_gui_model::options::Settings::load)
        .unwrap();
    let pages = hydrus_gui_model::options::pages(&settings);
    let index = pages
        .iter()
        .position(|page| page.name == "gui pages")
        .unwrap();
    let options = pages[index].options();
    let first = options
        .iter()
        .position(|option| option.label == fixture["labels"][0].as_str().unwrap())
        .unwrap();
    let values = hydrus_gui_model::options::values(&pages, &settings);
    for i in 0..4 {
        assert_eq!(
            options[first + i].label,
            fixture["labels"][i].as_str().unwrap()
        );
        assert_eq!(
            values[index][first + i],
            hydrus_gui_model::options::Value::Check(
                fixture["controls"][i]["default"].as_bool().unwrap()
            )
        );
        assert!(matches!(
            options[first + i].kind,
            hydrus_gui_model::options::Kind::Check
        ));
    }
}
