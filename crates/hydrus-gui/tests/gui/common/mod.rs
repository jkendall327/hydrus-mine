pub mod menus;

/// An explicit combined-local-media page for recordings that span every local
/// domain. GUI opening defaults are user settings and can select one domain.
pub fn all_local_page(store: std::sync::Arc<hydrus_store::Store>) -> hydrus_gui::SearchPage {
    let mut page = hydrus_gui::SearchPage::new(store);
    page.choose_location(hydrus_search::LocationContext::default());
    page
}
