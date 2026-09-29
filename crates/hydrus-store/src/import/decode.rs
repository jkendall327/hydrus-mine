//! Decoding a reference install's serialised settings into [`ImportInput`].
//!
//! The SQL half of the importer copies rows; this half turns the reference's
//! serialised objects (service settings, Client API keys, client options)
//! into native settings, via `hydrus-legacy`'s typed decoders.

use std::collections::BTreeSet;

use serde_json::{Value as Json, json};

use hydrus_core::ServiceType;
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_core::thumbnail::{ThumbnailScale, ThumbnailSettings};
use hydrus_legacy::LegacyDb;
use hydrus_legacy::objects::{self as legacy, ServiceConfig, TagRule};
use hydrus_legacy::readers::Service as LegacyService;

use super::{ApiPermissionsRow, ImportInput, settingless_kind};
use crate::error::{Result, StoreError};
use crate::services::{
    LikeRatingConfig, NumericalRatingConfig, PenBrush, RatingColours, RatingDisplay,
    RepositoryConfig, Rgb, ServerConfig, ServiceKind, StarAppearance, StarShape,
};
use crate::settings::{FavouriteTags, Setting};

/// Decode everything the importer needs from the reference install `db`.
pub fn decode_input(db: &LegacyDb) -> Result<ImportInput> {
    let mut input = ImportInput::default();
    for service in db.services()? {
        if settingless_kind(service.service_type).is_none() {
            input
                .service_kinds
                .insert(service.id, service_kind(&service)?);
        }
    }
    if let Some(manager) = db.client_api_manager()? {
        input.api_permissions = manager.permissions.iter().map(api_permissions).collect();
    }
    let options = db.client_options()?;
    let legacy_options = db.legacy_options()?;

    let mut thumbnails = ThumbnailSettings::default();
    if let Some((w, h)) = legacy_options.thumbnail_dimensions() {
        thumbnails.bounding_width = positive(w, "thumbnail width")?;
        thumbnails.bounding_height = positive(h, "thumbnail height")?;
    }
    if let Some(options) = &options {
        if let Some(&code) = options.integers.get("thumbnail_scale_type") {
            thumbnails.scale = match code {
                0 => ThumbnailScale::DownOnly,
                1 => ThumbnailScale::ToFit,
                2 => ThumbnailScale::ToFill,
                other => {
                    return Err(StoreError::Invalid(format!(
                        "unknown thumbnail scale type {other}"
                    )));
                }
            };
        }
        if let Some(&dpr) = options.integers.get("thumbnail_dpr_percent") {
            thumbnails.dpr_percent = positive(dpr, "thumbnail dpr percent")?;
        }
        if let Some(tags) = options.string_lists.get("favourite_tags") {
            insert_setting(&mut input, &FavouriteTags(tags.clone()))?;
        }
    }
    insert_setting(&mut input, &thumbnails)?;
    Ok(input)
}

fn insert_setting<S: Setting>(input: &mut ImportInput, value: &S) -> Result<()> {
    input
        .settings
        .insert(S::KEY.to_owned(), serde_json::to_value(value)?);
    Ok(())
}

fn positive(value: i64, what: &str) -> Result<u32> {
    u32::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| StoreError::Invalid(format!("{what} {value} is out of range")))
}

fn service_kind(service: &LegacyService) -> Result<ServiceKind> {
    // The verbatim settings of services we don't run yet, so nothing is lost.
    let verbatim = || RepositoryConfig {
        legacy: Json::String(service.dictionary.to_tuple_string()),
    };
    Ok(match (&service.config, service.service_type) {
        (ServiceConfig::LikeRating(c), _) => ServiceKind::RatingLike(LikeRatingConfig {
            display: rating_display(&c.display),
            appearance: appearance(&c.appearance)?,
        }),
        (ServiceConfig::NumericalRating(c), _) => {
            ServiceKind::RatingNumerical(NumericalRatingConfig {
                display: rating_display(&c.display),
                appearance: appearance(&c.appearance)?,
                num_stars: c.num_stars,
                allow_zero: c.allow_zero,
                custom_pad: i32::try_from(c.custom_pad).map_err(|_| {
                    StoreError::Invalid(format!("custom pad {} is out of range", c.custom_pad))
                })?,
                show_fraction_beside_stars: c.show_fraction_beside_stars as u8,
            })
        }
        (ServiceConfig::IncDecRating(display), _) => {
            ServiceKind::RatingIncDec(rating_display(display))
        }
        (ServiceConfig::ClientApi(c), _) => ServiceKind::ClientApi(ServerConfig {
            port: c.port,
            allow_non_local_connections: c.allow_non_local_connections,
            support_cors: c.support_cors,
            log_requests: c.log_requests,
            use_normie_eris: c.use_normie_eris,
            use_https: c.use_https,
            external_scheme_override: c.external_scheme_override.clone(),
            external_host_override: c.external_host_override.clone(),
            external_port_override: c.external_port_override.and_then(|p| u16::try_from(p).ok()),
        }),
        (_, ServiceType::TagRepository) => ServiceKind::TagRepository(verbatim()),
        (_, ServiceType::FileRepository) => ServiceKind::FileRepository(verbatim()),
        (_, ServiceType::Ipfs) => ServiceKind::Ipfs(verbatim()),
        (_, service_type) => ServiceKind::Unsupported {
            service_type,
            config: verbatim().legacy,
        },
    })
}

fn rating_display(display: &legacy::RatingDisplay) -> RatingDisplay {
    use legacy::RatingState;
    let defaults = RatingColours::default();
    let colour = |state, default: PenBrush| {
        display
            .colours
            .get(&state)
            .map_or(default, |c: &legacy::RatingColours| PenBrush {
                pen: Rgb(c.border),
                brush: Rgb(c.fill),
            })
    };
    RatingDisplay {
        colours: RatingColours {
            like: colour(RatingState::Like, defaults.like),
            dislike: colour(RatingState::Dislike, defaults.dislike),
            null: colour(RatingState::Null, defaults.null),
            mixed: colour(RatingState::Mixed, defaults.mixed),
        },
        show_in_thumbnail: display.show_in_thumbnail,
        show_in_thumbnail_even_when_null: display.show_in_thumbnail_even_when_null,
    }
}

fn appearance(appearance: &legacy::StarAppearance) -> Result<StarAppearance> {
    // the reference draws the shape if there is one, else the SVG
    if let Some(shape) = appearance.effective_shape() {
        let code = u8::try_from(shape.code()).map_err(|_| {
            StoreError::Invalid(format!("star shape {} is out of range", shape.code()))
        })?;
        return Ok(StarAppearance::Shape(StarShape(code)));
    }
    Ok(appearance
        .rating_svg
        .clone()
        .map_or_else(StarAppearance::default, StarAppearance::Svg))
}

fn api_permissions(p: &legacy::ApiPermissions) -> ApiPermissionsRow {
    let codes: BTreeSet<i64> = p.basic_permissions.iter().map(|p| p.code()).collect();
    let mut filter = TagFilter::new();
    for (slice, rule) in p.search_tag_filter.effective_rules() {
        let rule = match rule {
            TagRule::Allow => FilterRule::Whitelist,
            TagRule::Block => FilterRule::Blacklist,
        };
        filter.set_rule(slice, rule);
    }
    ApiPermissionsRow {
        access_key: p.access_key.clone(),
        name: p.name.clone(),
        permits_everything: p.permits_everything,
        permissions: json!(codes),
        search_tag_filter: (filter != TagFilter::default())
            .then(|| serde_json::to_value(&filter).expect("a tag filter serialises")),
    }
}
