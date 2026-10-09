//! Service settings: the `dictionary_string` of each row in `services`.
//!
//! Each service stores a `SerialisableDictionary` of named settings whose
//! keys depend on the service type (`ClientServices.GenerateDefaultServiceDictionary`
//! and each `Service` subclass's `_LoadFromDictionary`). Settings missing
//! from an old dictionary take the reference's defaults, exactly as the
//! reference does on load.

use std::collections::BTreeMap;

use hydrus_core::{ContentType, ServiceType, Sha256};

use super::util::{
    DecodeResult, Settings, boolean, float, hex_bytes, int, list, malformed, opt_int, opt_string,
    string, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{Meta, SerialisableObject, SerialisableType};

const KIND: SerialisableType = SerialisableType::DICTIONARY;

/// The decoded settings of a service, by kind of service.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceConfig {
    /// Services without settings: tag and file domains, trash, the
    /// combined/virtual services, notes.
    Plain,
    LikeRating(LikeRatingConfig),
    NumericalRating(NumericalRatingConfig),
    IncDecRating(RatingDisplay),
    ClientApi(ClientApiServiceConfig),
    /// Tag, file and rating repositories.
    Repository(RepositoryConfig),
    /// Server administration and message depot services.
    Restricted(RestrictedConfig),
    Ipfs(IpfsConfig),
}

/// An RGB colour.
pub type Rgb = [u8; 3];

/// A rating's visual state (`ClientRatings.LIKE` ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RatingState {
    Like = 0,
    Dislike = 1,
    Null = 2,
    Set = 3,
    Mixed = 4,
}

impl RatingState {
    pub const fn from_code(code: i64) -> Option<RatingState> {
        match code {
            0 => Some(RatingState::Like),
            1 => Some(RatingState::Dislike),
            2 => Some(RatingState::Null),
            3 => Some(RatingState::Set),
            4 => Some(RatingState::Mixed),
            _ => None,
        }
    }

    pub const fn code(self) -> i64 {
        self as i64
    }
}

/// Border and fill colour for one rating state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatingColours {
    pub border: Rgb,
    pub fill: Rgb,
}

/// Display settings every local rating service has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatingDisplay {
    pub colours: BTreeMap<RatingState, RatingColours>,
    pub show_in_thumbnail: bool,
    pub show_in_thumbnail_even_when_null: bool,
}

macro_rules! star_shapes {
    ($( $variant:ident = $code:literal, $label:literal; )*) => {
        /// A drawn rating shape (`ClientRatings.CIRCLE` ...).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum StarShape {
            $( $variant, )*
        }

        impl StarShape {
            pub const ALL: &'static [StarShape] = &[ $( StarShape::$variant, )* ];

            pub const fn code(self) -> i64 {
                match self { $( StarShape::$variant => $code, )* }
            }

            pub const fn from_code(code: i64) -> Option<StarShape> {
                match code {
                    $( $code => Some(StarShape::$variant), )*
                    _ => None,
                }
            }

            /// The name the Client API reports (`shape_to_str_lookup_dict`).
            pub const fn label(self) -> &'static str {
                match self { $( StarShape::$variant => $label, )* }
            }
        }
    };
}

star_shapes! {
    Circle = 0, "circle";
    Square = 1, "square";
    FatStar = 2, "fat star";
    PentagramStar = 3, "pentagram star";
    SixPointStar = 4, "six point star";
    EightPointStar = 5, "eight point star";
    XShape = 6, "x shape";
    Cross = 7, "square cross";
    TriangleUp = 30, "triangle up";
    TriangleDown = 31, "triangle down";
    TriangleRight = 32, "triangle right";
    TriangleLeft = 33, "triangle left";
    Diamond = 40, "diamond";
    RhombusRight = 42, "rhombus right";
    RhombusLeft = 43, "rhombus left";
    Hourglass = 44, "hourglass";
    Pentagon = 50, "pentagon";
    Hexagon = 60, "hexagon";
    SmallHexagon = 61, "small hexagon";
    Heart = 101, "heart";
    Teardrop = 102, "teardrop";
    MoonCrescent = 103, "crescent moon";
}

/// How a like/dislike or numerical rating is drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarAppearance {
    /// `None` when an SVG is used instead.
    pub shape: Option<StarShape>,
    /// The name of a rating SVG, if one is used.
    pub rating_svg: Option<String>,
}

impl StarAppearance {
    /// The shape actually drawn: with neither shape nor SVG the reference
    /// falls back to a fat star (`ClientRatings.StarType`).
    pub fn effective_shape(&self) -> Option<StarShape> {
        match (self.shape, &self.rating_svg) {
            (None, None) => Some(StarShape::FatStar),
            (shape, _) => shape,
        }
    }
}

/// A local like/dislike rating service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LikeRatingConfig {
    pub display: RatingDisplay,
    pub appearance: StarAppearance,
}

/// Where the "3/5" text goes next to numerical ratings (`ClientRatings.DRAW_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FractionPosition {
    None = 0,
    Left = 1,
    Right = 2,
}

/// A local numerical ("stars") rating service.
///
/// Ratings are stored as a fraction in `0.0..=1.0`; see
/// [`NumericalRatingConfig::stars_from_rating`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumericalRatingConfig {
    pub display: RatingDisplay,
    pub appearance: StarAppearance,
    pub num_stars: u32,
    /// Whether zero stars is a rating (otherwise the minimum is one).
    pub allow_zero: bool,
    pub custom_pad: i64,
    pub show_fraction_beside_stars: FractionPosition,
}

impl NumericalRatingConfig {
    /// Convert a stored rating to stars (`ClientRatings.ConvertRatingToStars`).
    pub fn stars_from_rating(&self, rating: f64) -> i64 {
        let n = f64::from(self.num_stars);
        if self.allow_zero {
            python_round(rating * n)
        } else {
            python_round(rating * (n - 1.0)) + 1
        }
    }

    /// Convert stars to the stored rating (`ClientRatings.ConvertStarsToRating`).
    pub fn rating_from_stars(&self, stars: i64) -> f64 {
        let n = i64::from(self.num_stars);
        let stars = stars.min(n);
        if self.allow_zero {
            stars.max(0) as f64 / n as f64
        } else {
            (stars.max(1) - 1) as f64 / (n - 1) as f64
        }
    }
}

/// Python 3's `round`, which rounds halves to even.
fn python_round(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// The Client API service's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientApiServiceConfig {
    /// `None` means the API is not running.
    pub port: Option<u16>,
    pub allow_non_local_connections: bool,
    pub support_cors: bool,
    pub log_requests: bool,
    /// Serve the plain "welcome" page instead of the ASCII art one.
    pub use_normie_eris: bool,
    pub use_https: bool,
    pub external_scheme_override: Option<String>,
    pub external_host_override: Option<String>,
    /// Text in the reference's editor (an empty string drops the `:`); old
    /// clients stored a number.
    pub external_port_override: Option<String>,
    /// Bandwidth usage history (type 39), kept verbatim.
    pub bandwidth_tracker: SerialisableObject,
    /// Bandwidth limits (type 38), kept verbatim.
    pub bandwidth_rules: SerialisableObject,
}

/// Where a remote service lives and how to authenticate (type 35).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub host: String,
    pub port: i64,
    pub access_key: Option<Vec<u8>>,
}

/// Settings every remote service has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteConfig {
    pub credentials: Credentials,
    pub no_requests_reason: String,
    /// Seconds since the epoch before which no requests are made.
    pub no_requests_until: i64,
}

/// Settings of services that need an account on a hydrus server.
#[derive(Debug, Clone, PartialEq)]
pub struct RestrictedConfig {
    pub remote: RemoteConfig,
    /// The cached account tuple, verbatim; the server is the source of truth
    /// and it is re-fetched on the next account sync.
    pub account: Option<PyJson>,
    pub next_account_sync: i64,
    pub network_sync_paused: bool,
    /// Options the server sent, verbatim (a dictionary object).
    pub service_options: SerialisableObject,
}

/// Settings of a repository service.
#[derive(Debug, Clone, PartialEq)]
pub struct RepositoryConfig {
    pub restricted: RestrictedConfig,
    pub metadata: RepositoryMetadata,
    pub do_a_full_metadata_resync: bool,
    pub update_downloading_paused: bool,
    pub update_processing_paused: bool,
    /// Per content type processing pauses, in stored order.
    pub update_processing_content_types_paused: Vec<(ContentType, bool)>,
}

/// An IPFS daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpfsConfig {
    pub remote: RemoteConfig,
    pub multihash_prefix: String,
}

/// Which update files a repository has published (type 37).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepositoryMetadata {
    pub updates: Vec<RepositoryUpdatePeriod>,
    /// Seconds since the epoch.
    pub next_update_due: i64,
}

/// One update period of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryUpdatePeriod {
    pub index: i64,
    pub update_hashes: Vec<Sha256>,
    /// Seconds since the epoch.
    pub begin: i64,
    pub end: i64,
}

impl ServiceConfig {
    /// Decode a service's settings dictionary.
    pub fn decode(
        service_type: ServiceType,
        dictionary: &SerialisableObject,
    ) -> DecodeResult<Self> {
        dictionary.expect_kind(KIND)?;
        let settings = Settings::new(KIND, dictionary)?;
        Ok(match service_type {
            ServiceType::LocalRatingLike => ServiceConfig::LikeRating(LikeRatingConfig {
                display: rating_display(&settings, service_type)?,
                appearance: appearance(&settings)?,
            }),
            ServiceType::LocalRatingNumerical => {
                ServiceConfig::NumericalRating(NumericalRatingConfig {
                    display: rating_display(&settings, service_type)?,
                    appearance: appearance(&settings)?,
                    num_stars: get(&settings, "num_stars", |v| {
                        let n = int(KIND, v, "num_stars")?;
                        u32::try_from(n)
                            .ok()
                            .filter(|n| *n > 0)
                            .ok_or_else(|| malformed(KIND, format!("bad num_stars {n}")))
                    })?
                    .unwrap_or(5),
                    allow_zero: get(&settings, "allow_zero", |v| boolean(KIND, v, "allow_zero"))?
                        .unwrap_or(true),
                    custom_pad: get(&settings, "custom_pad", |v| int(KIND, v, "custom_pad"))?
                        .unwrap_or(4),
                    show_fraction_beside_stars: get(
                        &settings,
                        "show_fraction_beside_stars",
                        |v| match int(KIND, v, "show_fraction_beside_stars")? {
                            0 => Ok(FractionPosition::None),
                            1 => Ok(FractionPosition::Left),
                            2 => Ok(FractionPosition::Right),
                            other => Err(malformed(KIND, format!("bad fraction position {other}"))),
                        },
                    )?
                    .unwrap_or(FractionPosition::None),
                })
            }
            ServiceType::LocalRatingIncDec => {
                ServiceConfig::IncDecRating(rating_display(&settings, service_type)?)
            }
            ServiceType::ClientApiService => ServiceConfig::ClientApi(client_api(&settings)?),
            ServiceType::TagRepository
            | ServiceType::FileRepository
            | ServiceType::RatingLikeRepository
            | ServiceType::RatingNumericalRepository => {
                ServiceConfig::Repository(repository(&settings, service_type)?)
            }
            ServiceType::ServerAdmin | ServiceType::MessageDepot => {
                ServiceConfig::Restricted(restricted(&settings, service_type)?)
            }
            ServiceType::Ipfs => ServiceConfig::Ipfs(IpfsConfig {
                remote: remote(&settings, service_type)?,
                multihash_prefix: get(&settings, "multihash_prefix", |v| {
                    string(KIND, v, "multihash_prefix")
                })?
                .unwrap_or_default(),
            }),
            _ => ServiceConfig::Plain,
        })
    }
}

/// Decode a plain setting if present.
fn get<T>(
    settings: &Settings<'_>,
    key: &str,
    decode: impl FnOnce(&PyJson) -> DecodeResult<T>,
) -> DecodeResult<Option<T>> {
    settings.plain(key)?.as_ref().map(decode).transpose()
}

/// A setting holding a serialisable object, if present and not `None`.
fn object(settings: &Settings<'_>, key: &str) -> DecodeResult<Option<SerialisableObject>> {
    match settings.get(key) {
        None | Some(Meta::Json(PyJson::Null)) => Ok(None),
        Some(Meta::Object(object)) => Ok(Some((**object).clone())),
        Some(_) => Err(malformed(KIND, format!("{key} should be an object"))),
    }
}

/// A fresh `HydrusNetworking.BandwidthTracker()`: ten empty usage buckets.
const EMPTY_BANDWIDTH_TRACKER: &str = "[39, 1, [[], [], [], [], [], [], [], [], [], []]]";
/// A fresh `HydrusNetworking.BandwidthRules()`: no limits.
const EMPTY_BANDWIDTH_RULES: &str = "[38, 1, []]";
const EMPTY_DICTIONARY: &str = "[21, 2, []]";

/// A setting holding an object, or the reference's default object.
fn object_or(
    settings: &Settings<'_>,
    key: &str,
    default: &str,
) -> DecodeResult<SerialisableObject> {
    match object(settings, key)? {
        Some(object) => Ok(object),
        None => SerialisableObject::from_tuple_str(default),
    }
}

fn rgb(value: &PyJson) -> DecodeResult<Rgb> {
    let channels = tuple::<3>(KIND, value, "colour")?;
    let mut out = [0u8; 3];
    for (slot, channel) in out.iter_mut().zip(channels) {
        let c = int(KIND, channel, "colour channel")?;
        *slot = u8::try_from(c).map_err(|_| malformed(KIND, format!("colour channel {c}")))?;
    }
    Ok(out)
}

fn default_colours(service_type: ServiceType) -> BTreeMap<RatingState, RatingColours> {
    let black = [0, 0, 0];
    let second = match service_type {
        ServiceType::LocalRatingLike => [200, 80, 120],
        ServiceType::LocalRatingNumerical | ServiceType::LocalRatingIncDec => [255, 255, 255],
        _ => return BTreeMap::new(),
    };
    [
        (RatingState::Like, [80, 200, 120]),
        (RatingState::Dislike, second),
        (RatingState::Null, [191, 191, 191]),
        (RatingState::Mixed, [95, 95, 95]),
    ]
    .into_iter()
    .map(|(state, fill)| {
        (
            state,
            RatingColours {
                border: black,
                fill,
            },
        )
    })
    .collect()
}

fn rating_display(
    settings: &Settings<'_>,
    service_type: ServiceType,
) -> DecodeResult<RatingDisplay> {
    let colours = match settings.plain("colours")? {
        None => default_colours(service_type),
        Some(value) => list(KIND, &value, "colours")?
            .iter()
            .map(|entry| {
                let [state, pair] = tuple::<2>(KIND, entry, "colour entry")?;
                let code = int(KIND, state, "rating state")?;
                let state = RatingState::from_code(code)
                    .ok_or_else(|| malformed(KIND, format!("unknown rating state {code}")))?;
                let [border, fill] = tuple::<2>(KIND, pair, "colour pair")?;
                Ok((
                    state,
                    RatingColours {
                        border: rgb(border)?,
                        fill: rgb(fill)?,
                    },
                ))
            })
            .collect::<DecodeResult<_>>()?,
    };
    Ok(RatingDisplay {
        colours,
        show_in_thumbnail: get(settings, "show_in_thumbnail", |v| {
            boolean(KIND, v, "show_in_thumbnail")
        })?
        .unwrap_or(false),
        show_in_thumbnail_even_when_null: get(settings, "show_in_thumbnail_even_when_null", |v| {
            boolean(KIND, v, "show_in_thumbnail_even_when_null")
        })?
        .unwrap_or(false),
    })
}

fn appearance(settings: &Settings<'_>) -> DecodeResult<StarAppearance> {
    let shape = match settings.plain("shape")? {
        // absent: the default circle; present but None: an SVG is used
        None => Some(StarShape::Circle),
        Some(PyJson::Null) => None,
        Some(value) => {
            let code = int(KIND, &value, "shape")?;
            Some(
                StarShape::from_code(code)
                    .ok_or_else(|| malformed(KIND, format!("unknown star shape {code}")))?,
            )
        }
    };
    Ok(StarAppearance {
        shape,
        rating_svg: get(settings, "rating_svg", |v| {
            opt_string(KIND, v, "rating_svg")
        })?
        .flatten(),
    })
}

fn opt_bool_setting(settings: &Settings<'_>, key: &str) -> DecodeResult<bool> {
    Ok(get(settings, key, |v| boolean(KIND, v, key))?.unwrap_or(false))
}

fn client_api(settings: &Settings<'_>) -> DecodeResult<ClientApiServiceConfig> {
    let opt_str = |key: &str| -> DecodeResult<Option<String>> {
        Ok(get(settings, key, |v| opt_string(KIND, v, key))?.flatten())
    };
    Ok(ClientApiServiceConfig {
        port: get(settings, "port", |v| {
            opt_int(KIND, v, "port")?
                .map(|p| u16::try_from(p).map_err(|_| malformed(KIND, format!("bad port {p}"))))
                .transpose()
        })?
        .flatten(),
        allow_non_local_connections: opt_bool_setting(settings, "allow_non_local_connections")?,
        support_cors: opt_bool_setting(settings, "support_cors")?,
        log_requests: opt_bool_setting(settings, "log_requests")?,
        use_normie_eris: opt_bool_setting(settings, "use_normie_eris")?,
        use_https: opt_bool_setting(settings, "use_https")?,
        external_scheme_override: opt_str("external_scheme_override")?,
        external_host_override: opt_str("external_host_override")?,
        external_port_override: get(settings, "external_port_override", |v| match v {
            PyJson::Null => Ok(None),
            v if v.as_i64().is_some() => Ok(v.as_i64().map(|p| p.to_string())),
            v => opt_string(KIND, v, "external_port_override"),
        })?
        .flatten(),
        bandwidth_tracker: object_or(settings, "bandwidth_tracker", EMPTY_BANDWIDTH_TRACKER)?,
        bandwidth_rules: object_or(settings, "bandwidth_rules", EMPTY_BANDWIDTH_RULES)?,
    })
}

fn credentials(object: &SerialisableObject) -> DecodeResult<Credentials> {
    let kind = SerialisableType::CREDENTIALS;
    object.expect_kind(kind)?;
    object.check_not_future()?;
    let info = object.info();
    let [host, port, key] = tuple::<3>(kind, &info, "credentials")?;
    Ok(Credentials {
        host: string(kind, host, "host")?,
        port: int(kind, port, "port")?,
        access_key: match key {
            PyJson::Null => None,
            key => Some(hex_bytes(kind, key, "access key")?),
        },
    })
}

fn remote(settings: &Settings<'_>, service_type: ServiceType) -> DecodeResult<RemoteConfig> {
    let credentials = match object(settings, "credentials")? {
        Some(object) => credentials(&object)?,
        None if service_type == ServiceType::Ipfs => Credentials {
            host: "127.0.0.1".into(),
            port: 5001,
            access_key: None,
        },
        None => Credentials {
            host: "hostname".into(),
            port: 45871,
            access_key: None,
        },
    };
    Ok(RemoteConfig {
        credentials,
        no_requests_reason: get(settings, "no_requests_reason", |v| {
            string(KIND, v, "no_requests_reason")
        })?
        .unwrap_or_default(),
        no_requests_until: get(settings, "no_requests_until", |v| {
            int(KIND, v, "no_requests_until")
        })?
        .unwrap_or(0),
    })
}

fn restricted(
    settings: &Settings<'_>,
    service_type: ServiceType,
) -> DecodeResult<RestrictedConfig> {
    // 'paused' is the pre-split name of 'network_sync_paused'
    let network_sync_paused = match get(settings, "network_sync_paused", |v| {
        boolean(KIND, v, "network_sync_paused")
    })? {
        Some(paused) => paused,
        None => opt_bool_setting(settings, "paused")?,
    };
    Ok(RestrictedConfig {
        remote: remote(settings, service_type)?,
        account: settings.plain("account")?,
        next_account_sync: get(settings, "next_account_sync", |v| {
            int(KIND, v, "next_account_sync")
        })?
        .unwrap_or(0),
        network_sync_paused,
        service_options: object_or(settings, "service_options", EMPTY_DICTIONARY)?,
    })
}

fn default_content_types(service_type: ServiceType) -> &'static [ContentType] {
    match service_type {
        ServiceType::TagRepository => &[
            ContentType::Mappings,
            ContentType::TagParents,
            ContentType::TagSiblings,
        ],
        ServiceType::FileRepository => &[ContentType::Files],
        _ => &[],
    }
}

fn repository(
    settings: &Settings<'_>,
    service_type: ServiceType,
) -> DecodeResult<RepositoryConfig> {
    // 'paused' predates the split into downloading and processing pauses;
    // the reference lets it override the newer settings when both exist
    let legacy_paused = get(settings, "paused", |v| boolean(KIND, v, "paused"))?;
    let paused = |key: &str| -> DecodeResult<bool> {
        match legacy_paused {
            Some(paused) => Ok(paused),
            None => Ok(get(settings, key, |v| boolean(KIND, v, key))?.unwrap_or(false)),
        }
    };
    let content_types_paused = match settings.plain("update_processing_content_types_paused")? {
        None => default_content_types(service_type)
            .iter()
            .map(|ct| (*ct, false))
            .collect(),
        Some(value) => list(KIND, &value, "content type pauses")?
            .iter()
            .map(|entry| {
                let [ct, paused] = tuple::<2>(KIND, entry, "content type pause")?;
                let code = int(KIND, ct, "content type")?;
                let ct = u8::try_from(code)
                    .ok()
                    .and_then(ContentType::from_code)
                    .ok_or_else(|| malformed(KIND, format!("unknown content type {code}")))?;
                Ok((ct, boolean(KIND, paused, "paused")?))
            })
            .collect::<DecodeResult<_>>()?,
    };
    Ok(RepositoryConfig {
        restricted: restricted(settings, service_type)?,
        metadata: match object(settings, "metadata")? {
            Some(object) => repository_metadata(&object)?,
            None => RepositoryMetadata::default(),
        },
        do_a_full_metadata_resync: opt_bool_setting(settings, "do_a_full_metadata_resync")?,
        update_downloading_paused: paused("update_downloading_paused")?,
        update_processing_paused: paused("update_processing_paused")?,
        update_processing_content_types_paused: content_types_paused,
    })
}

/// Decode repository metadata (type 37).
pub fn repository_metadata(object: &SerialisableObject) -> DecodeResult<RepositoryMetadata> {
    let kind = SerialisableType::METADATA;
    object.expect_kind(kind)?;
    object.check_not_future()?;
    let info = object.info();
    let [updates, next_due] = tuple::<2>(kind, &info, "metadata")?;
    let updates = list(kind, updates, "updates")?
        .iter()
        .map(|update| {
            let [index, hashes, begin, end] = tuple::<4>(kind, update, "update")?;
            Ok(RepositoryUpdatePeriod {
                index: int(kind, index, "update index")?,
                update_hashes: list(kind, hashes, "update hashes")?
                    .iter()
                    .map(|h| {
                        Sha256::from_slice(&hex_bytes(kind, h, "update hash")?)
                            .map_err(|e| malformed(kind, format!("update hash: {e}")))
                    })
                    .collect::<DecodeResult<_>>()?,
                begin: int(kind, begin, "begin")?,
                end: int(kind, end, "end")?,
            })
        })
        .collect::<DecodeResult<_>>()?;
    Ok(RepositoryMetadata {
        updates,
        next_update_due: float(kind, next_due, "next update due")? as i64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(service_type: ServiceType, dictionary: &str) -> ServiceConfig {
        let object = SerialisableObject::from_tuple_str(dictionary).unwrap();
        ServiceConfig::decode(service_type, &object).unwrap()
    }

    #[test]
    fn missing_settings_take_reference_defaults() {
        let ServiceConfig::NumericalRating(config) =
            decode(ServiceType::LocalRatingNumerical, "[21, 2, []]")
        else {
            panic!()
        };
        assert_eq!(config.num_stars, 5);
        assert!(config.allow_zero);
        assert_eq!(config.custom_pad, 4);
        assert_eq!(config.appearance.shape, Some(StarShape::Circle));
        assert_eq!(config.display.colours.len(), 4);
        assert_eq!(
            decode(ServiceType::LocalTag, "[21, 2, []]"),
            ServiceConfig::Plain
        );
    }

    #[test]
    fn star_conversions_match_reference_formulas() {
        let config = |allow_zero| NumericalRatingConfig {
            display: RatingDisplay {
                colours: BTreeMap::new(),
                show_in_thumbnail: false,
                show_in_thumbnail_even_when_null: false,
            },
            appearance: StarAppearance {
                shape: None,
                rating_svg: None,
            },
            num_stars: 5,
            allow_zero,
            custom_pad: 4,
            show_fraction_beside_stars: FractionPosition::None,
        };
        let zero = config(true);
        for stars in 0..=5 {
            assert_eq!(zero.stars_from_rating(zero.rating_from_stars(stars)), stars);
        }
        let one = config(false);
        assert!(one.rating_from_stars(1).abs() < f64::EPSILON);
        assert!((one.rating_from_stars(5) - 1.0).abs() < f64::EPSILON);
        assert_eq!(one.stars_from_rating(0.5), 3);
        // Python rounds halves to even: 0.5 * 5 = 2.5 -> 2
        assert_eq!(zero.stars_from_rating(0.5), 2);
    }

    #[test]
    fn legacy_paused_flag() {
        let ServiceConfig::Repository(config) = decode(
            ServiceType::TagRepository,
            r#"[21, 2, [[[0, "paused"], [0, true]]]]"#,
        ) else {
            panic!()
        };
        assert!(config.update_downloading_paused);
        assert!(config.update_processing_paused);
        assert!(config.restricted.network_sync_paused);
        assert_eq!(config.update_processing_content_types_paused.len(), 3);
        assert_eq!(config.restricted.remote.credentials.port, 45871);
    }
}
