//! The two Database > clear confirmations and completion notices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Clear,
    Cull,
}
impl Operation {
    pub const fn question(self) -> &'static str {
        match self {
            Self::Clear => {
                "Are you sure you want to delete _all_ file view count/duration and 'last time viewed' records? This cannot be undone."
            }
            Self::Cull => {
                "If your file viewing statistics have some erroneous values due to many short views or accidental long views, this routine will cull your current numbers to compensate. For instance:\n\nIf you have a file with 100 views over 100 seconds and a minimum view time of 2 seconds, this will cull the views to 50.\n\nIf you have a file with 10 views over 100000 seconds and a maximum view time of 60 seconds, this will cull the total viewtime to 600 seconds.\n\nIt will work for both preview and media views based on their separate rules."
            }
        }
    }
    pub const fn completed(self) -> &'static str {
        match self {
            Self::Clear => "Delete done! Please restart the client to see the changes in the UI.",
            Self::Cull => "Cull done! Please restart the client to see the changes in the UI.",
        }
    }
    pub fn apply(self, store: &hydrus_store::Store) -> hydrus_store::Result<()> {
        store.write(move |ctx| match self {
            Self::Clear => hydrus_store::viewing_maintenance::clear(ctx.conn()),
            Self::Cull => hydrus_store::viewing_maintenance::cull(ctx.conn()),
        })
    }
}
