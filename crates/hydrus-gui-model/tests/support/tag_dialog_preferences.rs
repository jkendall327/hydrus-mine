//! Recorded synthetic inputs shared by pure model and native consumer regressions.
use hydrus_core::{HashId, Sha256, Tag};
use hydrus_store::{
    Store,
    content::tag_relations::{self, RelationAction, RelationUpdate},
    display::RelationKind,
};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn seed(fixture: &Value) -> (tempfile::TempDir, Arc<Store>, Vec<HashId>) {
    let ([_legacy, directory], store, files) = seed_owned(fixture);
    (directory, store, files)
}

/// Native viewer/decoder consumers retain the extracted physical files as well.
pub fn seed_owned(fixture: &Value) -> ([tempfile::TempDir; 2], Arc<Store>, Vec<HashId>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let corpus = fixture["corpus"].clone();
    store
        .write_content(move |writer| {
            for row in corpus.as_array().unwrap() {
                let tag = hydrus_store::master::intern_tag(
                    writer.conn(),
                    &Tag::new(row["tag"].as_str().unwrap()).unwrap(),
                )?;
                let files = row["hashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|hash| {
                        let hash: Sha256 = hash.as_str().unwrap().parse().unwrap();
                        hydrus_store::master::hash_id(writer.conn(), &hash).map(Option::unwrap)
                    })
                    .collect::<hydrus_store::Result<Vec<_>>>()?;
                writer.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &files,
                )?;
            }
            Ok(())
        })
        .unwrap();
    for (kind, key) in [
        (RelationKind::Siblings, "siblings"),
        (RelationKind::Parents, "parents"),
    ] {
        tag_relations::apply(
            &store,
            kind,
            fixture[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| RelationUpdate {
                    service,
                    left: Tag::new(pair[0].as_str().unwrap()).unwrap(),
                    right: Tag::new(pair[1].as_str().unwrap()).unwrap(),
                    action: RelationAction::Add,
                })
                .collect(),
        )
        .unwrap();
    }
    let hashes: Vec<Sha256> = fixture["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hash| hash.as_str().unwrap().parse().unwrap())
        .collect();
    let files = store
        .read(|conn| {
            let ids = hydrus_store::master::hash_ids(conn, &hashes)?;
            Ok(hashes.iter().map(|hash| ids[hash]).collect())
        })
        .unwrap();
    ([legacy, directory], store, files)
}

pub fn preferences(options: &hydrus_store::tag_editing::TagEditingSettings) -> Value {
    json!({"listbook":options.use_listbook,"parents":options.tag_list_show_parents,"expanded":options.tag_list_expand_parents,"siblings":options.tag_list_show_siblings})
}

pub fn set_preferences(store: &Store, values: &Value) {
    let values = values.clone();
    let service = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    store
        .write(move |ctx| {
            let mut options: hydrus_store::tag_editing::TagEditingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            options.default_service = service;
            options.use_listbook = values["listbook"].as_bool().unwrap();
            options.tag_list_show_parents = values["parents"].as_bool().unwrap();
            options.tag_list_expand_parents = values["expanded"].as_bool().unwrap();
            options.tag_list_show_siblings = values["siblings"].as_bool().unwrap();
            hydrus_store::settings::set(ctx.conn(), &options)
        })
        .unwrap();
}

pub fn canonical(mut rows: Value) -> Value {
    // Qt's unsorted inherited parent collection has no relative row-order contract.
    // Keep logical tag order, primary labels, counts and the entire inherited bag.
    for row in rows.as_array_mut().unwrap() {
        row["rows"].as_array_mut().unwrap()[1..]
            .sort_by(|a, b| a.as_str().unwrap().cmp(b.as_str().unwrap()));
    }
    rows
}
