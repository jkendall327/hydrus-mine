//! Outline emoji fallback for Slint's software renderer.
//!
//! Slint 1.18.1 renders font outlines, not bitmap-only colour emoji. Its public
//! font registration API does not select an emoji fallback, so this small adapter
//! uses the matching, pinned core font context. Configure it after selecting the
//! platform and before creating windows; ordinary text and script fallbacks stay
//! owned by the platform. No process environment or application global is changed.

use i_slint_core::textlayout::sharedparley::fontique::GenericFamily;
use slint::PlatformError;

const EMOJI: &[u8] = include_bytes!("../assets/fonts/NotoEmoji.ttf");

/// Prefer the bundled outline emoji font, retaining platform emoji fallbacks.
/// The platform must already be selected, and windows must not yet be created.
/// Repeating this setup does not register duplicate fonts or fallback entries.
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
            // Fontique appends system families after this application mapping.
            // Copying generic_families() here would duplicate that system tail.
            fonts
                .collection
                .set_generic_families(GenericFamily::Emoji, std::iter::once(family));
            Ok(())
        },
    )?
}
