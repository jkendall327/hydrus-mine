//! Canvas/scanbar settings refreshed on file changes and Options Apply.
use hydrus_core::HashId;
use hydrus_store::{
    Store,
    media::FileFlags,
    settings::{
        self, ViewerBackgroundSettings, ViewerCanvasSettings, ViewerFocusSettings,
        ViewerHoverSettings, ViewerPointerSettings,
    },
};

pub(crate) fn refresh(window: &crate::MediaViewerWindow, store: &Store, file: HashId) {
    window.set_tag_banner(hydrus_gui_model::thumbnail_icons::viewer_banner(store, file).into());
    let options: ViewerCanvasSettings = store.read(settings::get).unwrap_or_default();
    let focus: ViewerFocusSettings = store.read(settings::get).unwrap_or_default();
    window.set_seek_requires_focus(focus.seek_requires_focus);
    window.set_hovers_require_focus(focus.hovers_require_focus);
    let background: ViewerBackgroundSettings = store.read(settings::get).unwrap_or_default();
    window.set_draw_tags_background(background.tags);
    window.set_draw_information_background(background.information);
    window.set_draw_ratings_background(background.ratings);
    window.set_draw_notes_background(background.notes);
    let hovers: ViewerHoverSettings = store.read(settings::get).unwrap_or_default();
    window.set_hover_tags_enabled(hovers.tags);
    window.set_hover_ratings_enabled(hovers.ratings);
    window.set_hover_notes_enabled(hovers.notes);
    window.set_draw_index_background(hovers.index_background);
    let pointer: ViewerPointerSettings = store.read(settings::get).unwrap_or_default();
    window.set_disallow_duration_drag(pointer.disallow_duration_drag);
    window.set_hide_during_drag(pointer.hide_during_drag);
    window.set_anchor_drag(pointer.anchor_drag);
    window.set_touch_drag_unanchor(pointer.touch_unanchors);
    let (transparent, has_duration) = store
        .read(|conn| {
            let (flags, duration): (u32, Option<i64>) = conn.query_row(
                "SELECT flags, duration_ms FROM files WHERE hash_id = ?",
                [file],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            Ok((
                FileFlags(flags).has(FileFlags::TRANSPARENCY),
                duration.is_some(),
            ))
        })
        .unwrap_or((false, false));
    window.set_media_has_duration(has_duration);
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
