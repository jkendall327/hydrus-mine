//! Software rendering needs outline emoji without replacing ordinary script fonts.
use i_slint_core::textlayout::sharedparley::{
    FontContext,
    fontique::{FamilyId, GenericFamily, QueryStatus, Script},
};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use std::rc::Rc;

struct NoWindows;

impl Platform for NoWindows {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Err(PlatformError::NoPlatform)
    }
}

fn ordinary_fallbacks(fonts: &mut FontContext) -> Vec<Vec<FamilyId>> {
    let mut families: Vec<_> = [
        GenericFamily::SansSerif,
        GenericFamily::Serif,
        GenericFamily::Monospace,
        GenericFamily::SystemUi,
    ]
    .into_iter()
    .map(|generic| fonts.collection.generic_families(generic).collect())
    .collect();
    for (script, language) in [(*b"Hani", "ja"), (*b"Hani", "zh-CN"), (*b"Hang", "ko")] {
        families.push(
            fonts
                .collection
                .fallback_families((Script::from_bytes(script), language))
                .collect(),
        );
    }
    families
}

#[test]
fn outline_fox_is_selected_without_replacing_platform_text_fallbacks() {
    // A fresh thread also proves that this adapter cannot silently select a backend.
    std::thread::spawn(|| {
        assert!(matches!(
            hydrus_gui::fonts::install_emoji_fallback(),
            Err(PlatformError::NoPlatform)
        ));
        slint::platform::set_platform(Box::new(NoWindows)).unwrap();
        let (ordinary, prior_emoji) = i_slint_core::with_global_context(
            || Err(PlatformError::NoPlatform),
            |context| {
                let mut fonts = context.font_context().borrow_mut();
                (
                    ordinary_fallbacks(&mut fonts),
                    fonts
                        .collection
                        .generic_families(GenericFamily::Emoji)
                        .collect::<Vec<_>>(),
                )
            },
        )
        .unwrap();

        // Both first installation and repeated embedding/headless setup retain order.
        for _ in 0..2 {
            hydrus_gui::fonts::install_emoji_fallback().unwrap();
            i_slint_core::with_global_context(
                || Err(PlatformError::NoPlatform),
                |context| {
                    let mut fonts = context.font_context().borrow_mut();
                    assert_eq!(ordinary_fallbacks(&mut fonts), ordinary);
                    let outline = fonts.collection.family_id("Noto Emoji").unwrap();
                    let expected: Vec<_> = std::iter::once(outline)
                        .chain(prior_emoji.iter().copied().filter(|id| *id != outline))
                        .collect();
                    assert_eq!(
                        fonts
                            .collection
                            .generic_families(GenericFamily::Emoji)
                            .collect::<Vec<_>>(),
                        expected
                    );
                    let inner = &mut fonts.inner;
                    let mut query = inner.collection.query(&mut inner.source_cache);
                    query.set_families([GenericFamily::Emoji]);
                    let mut found = false;
                    query.matches_with(|font| {
                        if let Some(glyph) = font.charmap().and_then(|map| map.map('🦊')) {
                            assert_ne!(glyph, 0);
                            assert_eq!(font.family.0, outline);
                            assert_eq!(
                                font.blob.as_ref(),
                                include_bytes!("../../assets/fonts/NotoEmoji.ttf")
                            );
                            found = true;
                            QueryStatus::Stop
                        } else {
                            QueryStatus::Continue
                        }
                    });
                    assert!(found, "the first fox glyph must come from the outline font");
                },
            )
            .unwrap();
        }
    })
    .join()
    .unwrap();
}
