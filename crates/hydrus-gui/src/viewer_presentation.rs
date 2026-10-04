//! Canvas/scanbar settings refreshed on file changes and Options Apply.
use hydrus_core::HashId;
use hydrus_store::{
    Store,
    media::FileFlags,
    settings::{self, ViewerCanvasSettings},
};

pub(crate) fn refresh(window: &crate::MediaViewerWindow, store: &Store, file: HashId) {
    let options: ViewerCanvasSettings = store.read(settings::get).unwrap_or_default();
    let transparent = store
        .read(|conn| {
            let flags: u32 =
                conn.query_row("SELECT flags FROM files WHERE hash_id = ?", [file], |row| {
                    row.get(0)
                })?;
            Ok(FileFlags(flags).has(FileFlags::TRANSPARENCY))
        })
        .unwrap_or(false);
    let mode = if !transparent || !options.transparency_checkerboard {
        0
    } else if options.transparency_greenscreen {
        2
    } else {
        1
    };
    window.set_transparency_mode(mode);
    window.set_seek_height(options.seek_height.clamp(1, 255) as f32);
    window.set_seek_hidden_height(
        options
            .seek_hidden_height
            .map_or(0, |height| height.clamp(1, 255)) as f32,
    );
    window.set_seek_nub_width(options.seek_nub_width.clamp(1, 63) as f32);
}
