//! Test fixtures: reference databases imported into native stores.

use std::path::Path;
use std::sync::Arc;

use hydrus_api::AppState;
use hydrus_core::ServiceId;
use hydrus_store::Store;
use hydrus_store::import::{ApiPermissionsRow, ImportInput, import};
use hydrus_store::services::{
    LikeRatingConfig, NumericalRatingConfig, PenBrush, RatingColours, RatingDisplay, Rgb,
    ServerConfig, ServiceKind, StarShape,
};
use serde_json::json;

/// An imported fixture: the unpacked reference db (whose client_files the
/// native store points at), and the native store with its API state.
pub struct Fixture {
    pub legacy_dir: tempfile::TempDir,
    /// Kept alive for the store's lifetime.
    pub _native_dir: tempfile::TempDir,
    pub state: Arc<AppState>,
}

fn unpack(name: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let tarball = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../oracle/fixtures/legacy_db/{name}.tar.gz"));
    let file = std::fs::File::open(&tarball).unwrap();
    tar::Archive::new(flate2::read::GzDecoder::new(file))
        .unpack(dir.path())
        .unwrap();
    dir
}

/// Decoded settings of the `basic` fixture.
// TODO: replace with the hydrus-legacy decoder once it lands, so tests use
// exactly the production import path.
fn basic_input() -> ImportInput {
    let black = Rgb([0, 0, 0]);
    let colours = |like: [u8; 3], dislike: [u8; 3]| RatingColours {
        like: PenBrush {
            pen: black,
            brush: Rgb(like),
        },
        dislike: PenBrush {
            pen: black,
            brush: Rgb(dislike),
        },
        null: PenBrush {
            pen: black,
            brush: Rgb([191, 191, 191]),
        },
        mixed: PenBrush {
            pen: black,
            brush: Rgb([95, 95, 95]),
        },
    };
    let display = |c| RatingDisplay {
        colours: c,
        show_in_thumbnail: false,
        show_in_thumbnail_even_when_null: false,
    };
    let mut input = ImportInput::default();
    input.service_kinds.insert(
        ServiceId(12),
        ServiceKind::RatingLike(LikeRatingConfig {
            display: display(colours([240, 240, 65], [200, 80, 120])),
            shape: StarShape::FAT_STAR,
            rating_svg: None,
        }),
    );
    input.service_kinds.insert(
        ServiceId(13),
        ServiceKind::ClientApi(ServerConfig {
            port: Some(45901),
            ..ServerConfig::default()
        }),
    );
    input.service_kinds.insert(
        ServiceId(16),
        ServiceKind::RatingNumerical(NumericalRatingConfig {
            display: display(colours([80, 200, 120], [255, 255, 255])),
            shape: StarShape::CIRCLE,
            rating_svg: None,
            num_stars: 5,
            allow_zero: true,
            custom_pad: 4,
            show_fraction_beside_stars: 0,
        }),
    );
    input.service_kinds.insert(
        ServiceId(17),
        ServiceKind::RatingIncDec(display(colours([80, 200, 120], [255, 255, 255]))),
    );
    input.api_permissions.push(ApiPermissionsRow {
        access_key: hex::decode("0123456789abcdef".repeat(4)).unwrap(),
        name: "oracle".into(),
        permits_everything: true,
        permissions: json!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]),
        search_tag_filter: None,
    });
    input.api_permissions.push(ApiPermissionsRow {
        access_key: hex::decode("fedcba9876543210".repeat(4)).unwrap(),
        name: "oracle restricted".into(),
        permits_everything: false,
        permissions: json!([3]),
        search_tag_filter: Some(
            json!({"rules": {"": "blacklist", ":": "blacklist", "safe": "whitelist"}}),
        ),
    });
    input
}

pub fn imported_store(name: &str) -> Fixture {
    assert_eq!(
        name, "basic",
        "only the basic fixture has decoded settings so far"
    );
    let legacy_dir = unpack(name);
    let native_dir = tempfile::tempdir().unwrap();
    import(
        legacy_dir.path(),
        &native_dir.path().join(hydrus_store::store::DB_FILE_NAME),
        &basic_input(),
    )
    .unwrap();
    let store = Store::open(native_dir.path()).unwrap();
    let state = AppState::new(store).unwrap();
    Fixture {
        legacy_dir,
        _native_dir: native_dir,
        state,
    }
}
