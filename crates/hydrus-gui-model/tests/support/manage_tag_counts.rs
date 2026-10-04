//! Actual Qt corpus; complete mappings on the same imported basic fixture.
use hydrus_core::{ContentStatus, HashId, Sha256, Tag};
use hydrus_store::{Store, content::MappingAction};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn seed(fixture: &Value) -> (tempfile::TempDir, Arc<Store>, Vec<HashId>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    let hashes: Vec<Sha256> = fixture["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hash| hash.as_str().unwrap().parse().unwrap())
        .collect();
    let files = store
        .read(|conn| {
            let ids = hydrus_store::master::hash_ids(conn, &hashes)?;
            Ok(hashes.iter().map(|hash| ids[hash]).collect::<Vec<_>>())
        })
        .unwrap();
    let corpus = fixture["corpus"].clone();
    let selected = files.clone();
    let snapshot = store.snapshot();
    store
        .write_content(move |writer| {
            for row in corpus.as_array().unwrap() {
                let service_name = row["service"].as_str().unwrap();
                let service = snapshot.services.by_name(service_name).ok_or_else(|| {
                    hydrus_store::StoreError::Invalid(format!(
                        "recorded tag service {service_name:?} is missing from the basic fixture"
                    ))
                })?.id;
                let tag = hydrus_store::master::intern_tag(
                    writer.conn(),
                    &Tag::new(row["tag"].as_str().unwrap()).unwrap(),
                )?;
                let files: Vec<_> = row["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|index| selected[usize::try_from(index.as_u64().unwrap()).unwrap()])
                    .collect();
                writer.update_mappings(service, &MappingAction::Add, tag, &files)?;
                if row["deleted"].as_bool().unwrap() {
                    writer.update_mappings(service, &MappingAction::Delete, tag, &files)?;
                }
            }
            Ok(())
        })
        .unwrap();
    let mine = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    store
        .write(move |ctx| {
            let mut defaults: hydrus_store::tag_editing::TagEditingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            defaults.default_service = mine;
            hydrus_store::settings::set(ctx.conn(), &defaults)?;
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::tag_editing::ManageTagsSettings::default(),
            )
        })
        .unwrap();
    (directory, store, files)
}
pub fn snapshot(model: &mut hydrus_gui_model::manage_tags::ManageTags) -> Value {
    let mut services = Vec::new();
    for (i, name) in model.service_names().into_iter().enumerate() {
        model.choose_service(i).unwrap();
        let rows: Vec<_> = model
            .display_rows()
            .into_iter()
            .filter(|row| !row.parent_row && row.tag.starts_with("checkpoint:"))
            .map(|row| json!({"tag":row.tag,"label":row.label}))
            .collect();
        services.push(json!({"service":name,"label":model.deleted_count_label(),"visible":model.deleted_count()>0,"tooltip":if model.show_deleted(){"hide deleted mappings"}else{"show deleted mappings"},"rows":rows}));
    }
    mine(model);
    json!({"show_deleted":model.show_deleted(),"services":services})
}
pub fn mine(model: &mut hydrus_gui_model::manage_tags::ManageTags) {
    let index = model
        .service_names()
        .iter()
        .position(|name| name == "my tags")
        .unwrap();
    model.choose_service(index).unwrap();
}
pub fn tags(
    store: &Store,
    files: &[HashId],
    service: &str,
    status: ContentStatus,
) -> Vec<Vec<String>> {
    let snapshot = store.snapshot();
    let service = snapshot.services.by_name(service).unwrap().id;
    let batch = store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, files))
        .unwrap();
    files
        .iter()
        .map(|file| {
            let media = batch
                .results
                .iter()
                .find(|media| media.hash_id == *file)
                .unwrap();
            let mut tags: Vec<_> = media
                .tags
                .get(&service)
                .and_then(|tags| tags.by_status.get(&status))
                .into_iter()
                .flatten()
                .map(|id| batch.tags[id].to_string())
                .collect();
            tags.sort();
            tags
        })
        .collect()
}
