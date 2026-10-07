//! Outline emoji fallback for Slint's software renderer.
//!
//! Slint 1.18.1 renders font outlines, not bitmap-only colour emoji. Its public
//! font registration API does not select an emoji fallback, so this small adapter
//! uses the matching, pinned core font context. Configure it after selecting the
//! platform and before creating windows. Preserve the platform fallback prefix
//! before its preferred emoji face; no process environment or application global
//! is changed.

use i_slint_core::textlayout::sharedparley::fontique::GenericFamily;
use slint::PlatformError;

const EMOJI: &[u8] = include_bytes!("../assets/fonts/NotoEmoji.ttf");

/// Prefer the bundled outline emoji font, retaining platform emoji fallbacks.
/// The platform must already be selected, and windows must not yet be created.
/// Repeating this setup does not register duplicate fonts or grow the mapping.
pub fn install_emoji_fallback() -> Result<(), PlatformError> {
    i_slint_core::with_global_context(
        || Err(PlatformError::NoPlatform),
        |context| {
            let mut fonts = context.font_context().borrow_mut();
            fonts.register_static_font(EMOJI);
            let family = fonts.collection.family_id("Noto Emoji").ok_or_else(|| {
                PlatformError::Other(
                    "the bundled outline emoji font could not be registered".into(),
                )
            })?;
            let platform_emoji = fonts
                .collection
                .generic_families(GenericFamily::Emoji)
                .find(|id| *id != family);
            // Slint text controls request SansSerif then SystemUi. Each generic
            // includes a broad system tail that can select bitmap emoji before
            // an Emoji-only mapping is ever considered. Retain the entire prefix
            // preceding the platform's preferred emoji face, then insert outlines.
            for generic in [GenericFamily::SansSerif, GenericFamily::SystemUi] {
                let existing: Vec<_> = fonts.collection.generic_families(generic).collect();
                if existing.contains(&family) {
                    continue;
                }
                let boundary = platform_emoji
                    .and_then(|emoji| existing.iter().position(|id| *id == emoji))
                    .unwrap_or(existing.len());
                // Preserve a configured primary even when it is also the emoji
                // face. An explicitly preferred bitmap primary remains outside
                // this fallback repair, just like a named bitmap font request.
                let boundary = boundary.max(usize::from(!existing.is_empty()));
                fonts.collection.set_generic_families(
                    generic,
                    existing[..boundary]
                        .iter()
                        .copied()
                        .chain(std::iter::once(family)),
                );
            }
            // Fontique appends system families after this application mapping.
            // The text mappings above necessarily repeat the retained prefix in
            // that tail; avoid copying the entire chain or growing it on retries.
            fonts
                .collection
                .set_generic_families(GenericFamily::Emoji, std::iter::once(family));
            Ok(())
        },
    )?
}
