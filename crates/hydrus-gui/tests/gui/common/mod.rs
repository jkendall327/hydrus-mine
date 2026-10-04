pub mod menus;

/// An explicit combined-local-media page for recordings that span every local
/// domain. GUI opening defaults are user settings and can select one domain.
pub fn all_local_page(store: std::sync::Arc<hydrus_store::Store>) -> hydrus_gui::SearchPage {
    store
        .write_and_refresh(|ctx| {
            let mut defaults: hydrus_store::settings::SearchDefaults =
                hydrus_store::settings::get(ctx.conn())?;
            defaults.local_location = hydrus_search::LocationContext::default();
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
    hydrus_gui::SearchPage::new(store)
}
