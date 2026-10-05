pub mod menus;

/// Recordings that expect trash to prune a view opt into that saved policy.
/// The reference's false default otherwise retains page and viewer rows.
pub fn remove_trashed_from_view(store: &hydrus_store::Store) {
    store
        .write(|ctx| {
            let mut preferences: hydrus_store::settings::FileViewRemoval =
                hydrus_store::settings::get(ctx.conn())?;
            preferences.trashed = true;
            hydrus_store::settings::set(ctx.conn(), &preferences)
        })
        .unwrap();
}

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
