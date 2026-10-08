//! The parents and siblings source-application queues: add, reorder, remove,
//! explicit empty queues, reopen, and the daemon store seeing the change.
use hydrus_core::Tag;
use hydrus_gui_model::tag_display::TagDisplayEditor;
use hydrus_store::Store;
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::display::RelationKind;

fn add(store: &Store, kind: RelationKind, service: hydrus_core::ServiceId, l: &str, r: &str) {
    tag_relations::apply(
        store,
        kind,
        vec![RelationUpdate {
            service,
            left: Tag::new(l).unwrap(),
            right: Tag::new(r).unwrap(),
            action: RelationAction::Add,
        }],
    )
    .unwrap();
}

fn queue_test(parents: bool) {
    let kind = if parents {
        RelationKind::Parents
    } else {
        RelationKind::Siblings
    };
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let daemon = Store::open(dir.path()).unwrap();
    store
        .write_and_refresh(|ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &hydrus_core::ServiceKey::new(vec![77; 16]),
                "third tags",
                &hydrus_store::services::ServiceKind::LocalTags,
            )
        })
        .unwrap();
    let snap = store.snapshot();
    let third = snap.services.by_name("third tags").unwrap().clone();
    let mine = snap.services.by_name("my tags").unwrap().clone();
    let other = snap.services.by_name("downloader tags").unwrap().clone();
    add(&store, kind, mine.id, "queue old", "queue own");
    add(&store, kind, other.id, "queue old", "queue other");
    let queue = |editor: &TagDisplayEditor| {
        let s = editor
            .services()
            .iter()
            .find(|s| s.key == mine.key)
            .unwrap();
        if parents {
            s.parents.clone()
        } else {
            s.siblings.clone()
        }
    };
    let position = |editor: &TagDisplayEditor| {
        editor
            .services()
            .iter()
            .position(|s| s.key == mine.key)
            .unwrap()
    };
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    editor.choose(position(&editor));
    let initial = queue(&editor);
    // A source cannot be added twice, and unknown services are refused.
    assert!(editor.add_source(parents, other.key.clone()));
    assert!(!editor.add_source(parents, other.key.clone()));
    assert!(!editor.add_source(parents, hydrus_core::ServiceKey::new(vec![200; 16])));
    assert!(editor.add_source(parents, third.key.clone()));
    let mut expected = initial.clone();
    expected.push(other.key.clone());
    expected.push(third.key.clone());
    assert_eq!(queue(&editor), expected);
    // Reorder: moving the last one up swaps it with its neighbour.
    editor.change_source(parents, expected.len() - 1, Some(-1));
    let n = expected.len();
    expected.swap(n - 1, n - 2);
    assert_eq!(queue(&editor), expected);
    assert_eq!(expected[n - 2], third.key);
    // Nothing is written until Apply.
    let fresh = TagDisplayEditor::new(store.clone()).unwrap();
    assert_eq!(queue(&fresh), initial);
    editor.apply().unwrap();
    assert!(daemon.refresh_if_changed().unwrap());
    let reopened = TagDisplayEditor::new(daemon.clone()).unwrap();
    assert_eq!(queue(&reopened), expected);
    // Remove every source: an explicit empty queue stays empty after reopen.
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    editor.choose(position(&editor));
    while !queue(&editor).is_empty() {
        editor.change_source(parents, 0, None);
    }
    editor.apply().unwrap();
    let reopened = TagDisplayEditor::new(store.clone()).unwrap();
    assert!(queue(&reopened).is_empty());
    let graph = store.snapshot().display.get(mine.id);
    let old = store
        .read(|c| hydrus_store::master::intern_tag(c, &Tag::new("queue old").unwrap()))
        .unwrap();
    if parents {
        assert!(graph.ancestors(old).is_empty());
    } else {
        assert_eq!(graph.ideal(old), old);
    }
}

// leaf: audit-media-application-parents
#[test]
fn parent_source_queue_adds_reorders_removes_and_reopens() {
    queue_test(true);
}

// leaf: audit-media-application-siblings
#[test]
fn sibling_source_queue_adds_reorders_removes_and_reopens() {
    queue_test(false);
}
