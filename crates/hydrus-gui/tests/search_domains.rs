//! A search page's file and tag domains: new pages search the options'
//! default tag service, as the reference's `NewPageQuery` makes them.

use std::sync::Arc;

use hydrus_core::ServiceKey;
use hydrus_core::pages::PageContent;
use hydrus_core::service::builtin_keys;
use hydrus_gui::Pages;
use hydrus_gui::page_chooser::NewPage;
use hydrus_search::{LocationContext, TagContext};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::settings::{self, SearchDefaults};

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn key(k: &[u8]) -> ServiceKey {
    ServiceKey::new(k.to_vec())
}

/// The page shown's file and tag domains.
fn domains(pages: &Pages) -> (LocationContext, TagContext) {
    match &pages.shown().content {
        PageContent::Search { search, .. } => (search.location.clone(), search.tags.clone()),
        other => panic!("not a search page: {other:?}"),
    }
}

fn set_tag_service(store: &Store, service: ServiceKey) {
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &SearchDefaults {
                    tag_service: service,
                    ..SearchDefaults::default()
                },
            )
        })
        .unwrap();
}

#[test]
fn new_search_pages_search_the_default_tag_service() {
    let (_dirs, store) = store();
    let mut pages = Pages::open(store.clone()).unwrap();
    let my_files = LocationContext::single(key(builtin_keys::MY_FILES));
    // hydrus's default: every tag service
    pages.new_search_page();
    assert_eq!(
        domains(&pages),
        (
            my_files.clone(),
            TagContext::new(key(builtin_keys::COMBINED_TAG), true, true)
        )
    );
    // the options' "my tags": new pages, those the page chooser opens, and
    // files opened in a new page
    set_tag_service(&store, key(builtin_keys::MY_TAGS));
    let my_tags = TagContext::new(key(builtin_keys::MY_TAGS), true, true);
    pages.new_search_page();
    assert_eq!(domains(&pages), (my_files, my_tags.clone()));
    let trash = LocationContext::single(key(builtin_keys::TRASH));
    pages
        .new_page(&NewPage::Search {
            domain: key(builtin_keys::TRASH),
            name: "trash".into(),
        })
        .unwrap();
    assert_eq!(domains(&pages), (trash.clone(), my_tags.clone()));
    pages.open_files(trash.clone(), Vec::new(), None, None);
    assert_eq!(domains(&pages), (trash, my_tags.clone()));
    // all known files, searched with a tag service
    let all_known_files = LocationContext::single(key(builtin_keys::COMBINED_FILE));
    pages.open_search(all_known_files.clone(), Vec::new(), "files");
    assert_eq!(domains(&pages), (all_known_files.clone(), my_tags));
    // but with every tag service, all the files stored here instead
    set_tag_service(&store, key(builtin_keys::COMBINED_TAG));
    pages.open_search(all_known_files, Vec::new(), "files");
    assert_eq!(
        domains(&pages).0,
        LocationContext::single(key(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE))
    );
    // a tag service that is gone: every tag service
    set_tag_service(&store, key(b"a service deleted since"));
    pages.new_search_page();
    assert!(domains(&pages).1.is_all_known_tags());
}
