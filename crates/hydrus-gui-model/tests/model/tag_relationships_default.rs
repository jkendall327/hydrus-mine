//! The siblings and parents dialogs open on the default tag service tab, and
//! remember a tab change only when the option to do so is on (the reference's
//! `save_default_tag_service_tab_on_change`).
use hydrus_gui_model::tag_relationships::{RelationKind, Relationships};
use hydrus_store::{Store, settings, tag_editing::TagEditingSettings};

fn run(kind: RelationKind) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write_and_refresh(|ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &hydrus_core::ServiceKey::new(vec![55; 16]),
                "zzz second tags",
                &hydrus_store::services::ServiceKind::LocalTags,
            )
        })
        .unwrap();
    let snap = store.snapshot();
    let mine = snap.services.by_name("my tags").unwrap().key.clone();
    let second = snap
        .services
        .by_name("zzz second tags")
        .unwrap()
        .key
        .clone();
    let set = |remember: bool, default: &hydrus_core::ServiceKey| {
        let default = default.clone();
        store
            .write(move |ctx| {
                let mut editing: TagEditingSettings = settings::get(ctx.conn())?;
                editing.remember_service = remember;
                editing.default_service = default;
                settings::set(ctx.conn(), &editing)
            })
            .unwrap();
    };
    let editing = || -> TagEditingSettings { store.read(settings::get).unwrap() };
    let index_of = |editor: &Relationships, name: &str| {
        editor
            .service_names()
            .iter()
            .position(|n| n == name)
            .unwrap()
    };
    // opens on the default service's tab, wherever it is in the list
    for (default, name) in [(&second, "zzz second tags"), (&mine, "my tags")] {
        set(true, default);
        let editor = Relationships::new(store.clone(), kind).unwrap();
        assert_eq!(editor.service(), index_of(&editor, name));
    }
    // a tab change is remembered when the option is on ...
    set(true, &mine);
    let mut editor = Relationships::new(store.clone(), kind).unwrap();
    let other = index_of(&editor, "zzz second tags");
    editor.choose_service_remembered(other).unwrap();
    assert_eq!(editor.service(), other);
    assert_eq!(editing().default_service, second);
    assert_eq!(
        Relationships::new(store.clone(), kind).unwrap().service(),
        other
    );
    // ... and not when it is off
    set(false, &mine);
    let mut editor = Relationships::new(store.clone(), kind).unwrap();
    let other = index_of(&editor, "zzz second tags");
    editor.choose_service_remembered(other).unwrap();
    assert_eq!(editor.service(), other, "the tab still changes");
    assert_eq!(editing().default_service, mine);
    assert_eq!(
        Relationships::new(store, kind).unwrap().service(),
        index_of(&editor, "my tags")
    );
}

// leaf: audit-media-relationships-parents-default-service
#[test]
fn parents_open_on_and_remember_the_default_service_tab() {
    run(RelationKind::Parents);
}

// leaf: audit-media-relationships-siblings-default-service
#[test]
fn siblings_open_on_and_remember_the_default_service_tab() {
    run(RelationKind::Siblings);
}
