//! Services and their typed configuration.
//!
//! A [`Service`] is a domain that content lives in. Its [`ServiceKind`] fixes
//! both its type and its settings, so e.g. a like/dislike rating service
//! cannot carry a star count. The table stores the reference-compatible type
//! code plus the variant's settings as JSON.
//!
//! The in-memory [`ServiceRegistry`] is an immutable snapshot; changes go
//! through the writer, which publishes a fresh snapshot after commit.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use hydrus_core::service::builtin_keys;
use hydrus_core::{ServiceId, ServiceKey, ServiceType};

use crate::error::{Result, StoreError};
use crate::schema;

/// An sRGB colour, serialised as `#RRGGBB`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);

impl fmt::Debug for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "#{r:02X}{g:02X}{b:02X}")
    }
}

impl Serialize for Rgb {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let hex = s.strip_prefix('#').unwrap_or(&s);
        let mut out = [0u8; 3];
        hex::decode_to_slice(hex, &mut out).map_err(serde::de::Error::custom)?;
        Ok(Rgb(out))
    }
}

/// Border and fill of a rating shape in one state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PenBrush {
    pub pen: Rgb,
    pub brush: Rgb,
}

/// How a rating is drawn in each state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatingColours {
    pub like: PenBrush,
    pub dislike: PenBrush,
    pub null: PenBrush,
    pub mixed: PenBrush,
}

impl Default for RatingColours {
    fn default() -> Self {
        let black = Rgb([0, 0, 0]);
        Self {
            like: PenBrush {
                pen: black,
                brush: Rgb([80, 200, 120]),
            },
            dislike: PenBrush {
                pen: black,
                brush: Rgb([255, 255, 255]),
            },
            null: PenBrush {
                pen: black,
                brush: Rgb([191, 191, 191]),
            },
            mixed: PenBrush {
                pen: black,
                brush: Rgb([95, 95, 95]),
            },
        }
    }
}

/// The shape ratings are drawn with. Codes match the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StarShape(pub u8);

impl StarShape {
    pub const CIRCLE: StarShape = StarShape(0);
    pub const FAT_STAR: StarShape = StarShape(2);

    /// Human name, as the Client API reports it.
    pub fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0 => "circle",
            1 => "square",
            2 => "fat star",
            3 => "pentagram star",
            4 => "six point star",
            5 => "eight point star",
            6 => "x shape",
            7 => "square cross",
            30 => "triangle up",
            31 => "triangle down",
            32 => "triangle right",
            33 => "triangle left",
            40 => "diamond",
            42 => "rhombus right",
            43 => "rhombus left",
            44 => "hourglass",
            50 => "pentagon",
            60 => "hexagon",
            61 => "small hexagon",
            101 => "heart",
            102 => "teardrop",
            103 => "crescent moon",
            _ => return None,
        })
    }
}

/// How a like/dislike or numerical rating is drawn: a built-in shape or a
/// named SVG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarAppearance {
    Shape(StarShape),
    Svg(String),
}

impl Default for StarAppearance {
    fn default() -> Self {
        StarAppearance::Shape(StarShape::FAT_STAR)
    }
}

impl StarAppearance {
    /// The label the Client API reports (`star_shape`).
    pub fn label(&self) -> &'static str {
        match self {
            StarAppearance::Shape(shape) => shape.name().unwrap_or("circle"),
            StarAppearance::Svg(_) => "svg",
        }
    }
}

/// Settings every local rating service has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RatingDisplay {
    pub colours: RatingColours,
    pub show_in_thumbnail: bool,
    pub show_in_thumbnail_even_when_null: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LikeRatingConfig {
    #[serde(flatten)]
    pub display: RatingDisplay,
    pub appearance: StarAppearance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericalRatingConfig {
    #[serde(flatten)]
    pub display: RatingDisplay,
    pub appearance: StarAppearance,
    pub num_stars: u32,
    pub allow_zero: bool,
    pub custom_pad: i32,
    pub show_fraction_beside_stars: u8,
}

impl NumericalRatingConfig {
    /// Convert a stored rating (a fraction in `[0, 1]`) to stars, as the
    /// reference does (Python `round`, i.e. ties to even).
    pub fn stars(&self, rating: f64) -> u32 {
        let n = f64::from(self.num_stars);
        if self.allow_zero {
            (rating * n).round_ties_even() as u32
        } else {
            (rating * (n - 1.0)).round_ties_even() as u32 + 1
        }
    }

    /// Convert stars to the stored fraction.
    pub fn rating(&self, stars: u32) -> f64 {
        let n = f64::from(self.num_stars);
        if self.allow_zero {
            f64::from(stars) / n
        } else if self.num_stars <= 1 {
            1.0
        } else {
            f64::from(stars.saturating_sub(1)) / (n - 1.0)
        }
    }

    pub fn min_stars(&self) -> u32 {
        u32::from(!self.allow_zero)
    }
}

/// Settings of a local HTTP server (the Client API).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    pub port: Option<u16>,
    pub allow_non_local_connections: bool,
    pub support_cors: bool,
    pub log_requests: bool,
    pub use_normie_eris: bool,
    pub use_https: bool,
    pub external_scheme_override: Option<String>,
    pub external_host_override: Option<String>,
    pub external_port_override: Option<u16>,
}

/// Settings of a remote repository. Not functional yet; the fields the
/// importer can't place yet are kept verbatim in `legacy`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RepositoryConfig {
    pub legacy: serde_json::Value,
}

/// What a service is, with its settings.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceKind {
    LocalTags,
    TagRepository(RepositoryConfig),
    /// A user-facing local file domain, e.g. "my files".
    LocalFiles,
    /// Where repository update files live.
    LocalUpdates,
    Trash,
    /// Union of every [`ServiceKind::LocalFiles`] domain ("combined local file domains").
    CombinedLocalMedia,
    /// Every file physically stored: local domains, updates and trash
    /// ("hydrus local file storage").
    LocalFileStorage,
    /// Every file deleted from anywhere.
    CombinedDeleted,
    /// Any file at all, known or not.
    AllKnownFiles,
    /// Union of every tag service.
    AllKnownTags,
    FileRepository(RepositoryConfig),
    Ipfs(RepositoryConfig),
    LocalNotes,
    ClientApi(ServerConfig),
    RatingLike(LikeRatingConfig),
    RatingNumerical(NumericalRatingConfig),
    RatingIncDec(RatingDisplay),
    /// A service type we preserve but do not use (e.g. the defunct local booru).
    Unsupported {
        service_type: ServiceType,
        config: serde_json::Value,
    },
}

impl ServiceKind {
    pub fn service_type(&self) -> ServiceType {
        match self {
            ServiceKind::LocalTags => ServiceType::LocalTag,
            ServiceKind::TagRepository(_) => ServiceType::TagRepository,
            ServiceKind::LocalFiles => ServiceType::LocalFileDomain,
            ServiceKind::LocalUpdates => ServiceType::LocalFileUpdateDomain,
            ServiceKind::Trash => ServiceType::LocalFileTrashDomain,
            ServiceKind::CombinedLocalMedia => ServiceType::CombinedLocalFileDomains,
            ServiceKind::LocalFileStorage => ServiceType::HydrusLocalFileStorage,
            ServiceKind::CombinedDeleted => ServiceType::CombinedDeletedFile,
            ServiceKind::AllKnownFiles => ServiceType::CombinedFile,
            ServiceKind::AllKnownTags => ServiceType::CombinedTag,
            ServiceKind::FileRepository(_) => ServiceType::FileRepository,
            ServiceKind::Ipfs(_) => ServiceType::Ipfs,
            ServiceKind::LocalNotes => ServiceType::LocalNotes,
            ServiceKind::ClientApi(_) => ServiceType::ClientApiService,
            ServiceKind::RatingLike(_) => ServiceType::LocalRatingLike,
            ServiceKind::RatingNumerical(_) => ServiceType::LocalRatingNumerical,
            ServiceKind::RatingIncDec(_) => ServiceType::LocalRatingIncDec,
            ServiceKind::Unsupported { service_type, .. } => *service_type,
        }
    }

    /// Settings as stored in the `config` column.
    pub fn config_json(&self) -> Result<String> {
        let value = match self {
            ServiceKind::TagRepository(c)
            | ServiceKind::FileRepository(c)
            | ServiceKind::Ipfs(c) => serde_json::to_value(c)?,
            ServiceKind::ClientApi(c) => serde_json::to_value(c)?,
            ServiceKind::RatingLike(c) => serde_json::to_value(c)?,
            ServiceKind::RatingNumerical(c) => serde_json::to_value(c)?,
            ServiceKind::RatingIncDec(c) => serde_json::to_value(c)?,
            ServiceKind::Unsupported { config, .. } => config.clone(),
            _ => serde_json::Value::Object(serde_json::Map::new()),
        };
        Ok(value.to_string())
    }

    /// Rebuild from the stored type code and settings.
    pub fn from_stored(service_type: ServiceType, config: &str) -> Result<Self> {
        let json: serde_json::Value = serde_json::from_str(config)?;
        Ok(match service_type {
            ServiceType::LocalTag => ServiceKind::LocalTags,
            ServiceType::TagRepository => ServiceKind::TagRepository(serde_json::from_value(json)?),
            ServiceType::LocalFileDomain => ServiceKind::LocalFiles,
            ServiceType::LocalFileUpdateDomain => ServiceKind::LocalUpdates,
            ServiceType::LocalFileTrashDomain => ServiceKind::Trash,
            ServiceType::CombinedLocalFileDomains => ServiceKind::CombinedLocalMedia,
            ServiceType::HydrusLocalFileStorage => ServiceKind::LocalFileStorage,
            ServiceType::CombinedDeletedFile => ServiceKind::CombinedDeleted,
            ServiceType::CombinedFile => ServiceKind::AllKnownFiles,
            ServiceType::CombinedTag => ServiceKind::AllKnownTags,
            ServiceType::FileRepository => {
                ServiceKind::FileRepository(serde_json::from_value(json)?)
            }
            ServiceType::Ipfs => ServiceKind::Ipfs(serde_json::from_value(json)?),
            ServiceType::LocalNotes => ServiceKind::LocalNotes,
            ServiceType::ClientApiService => ServiceKind::ClientApi(serde_json::from_value(json)?),
            ServiceType::LocalRatingLike => ServiceKind::RatingLike(serde_json::from_value(json)?),
            ServiceType::LocalRatingNumerical => {
                ServiceKind::RatingNumerical(serde_json::from_value(json)?)
            }
            ServiceType::LocalRatingIncDec => {
                ServiceKind::RatingIncDec(serde_json::from_value(json)?)
            }
            other => ServiceKind::Unsupported {
                service_type: other,
                config: json,
            },
        })
    }

    /// Whether this service stores tag mappings in per-service tables.
    pub fn has_mappings(&self) -> bool {
        matches!(self, ServiceKind::LocalTags | ServiceKind::TagRepository(_))
    }
}

/// A service.
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    pub id: ServiceId,
    pub key: ServiceKey,
    pub name: String,
    pub kind: ServiceKind,
}

impl Service {
    pub fn service_type(&self) -> ServiceType {
        self.kind.service_type()
    }
}

/// An immutable snapshot of every service.
#[derive(Debug, Clone, Default)]
pub struct ServiceRegistry {
    services: Vec<Arc<Service>>,
    by_id: HashMap<ServiceId, usize>,
    by_key: HashMap<ServiceKey, usize>,
}

impl ServiceRegistry {
    pub fn load(conn: &Connection) -> Result<Self> {
        let mut stmt =
            conn.prepare("SELECT service_id, service_key, service_type, name, config FROM services ORDER BY service_id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, ServiceId>(0)?,
                r.get::<_, ServiceKey>(1)?,
                r.get::<_, u8>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut services = Vec::new();
        for row in rows {
            let (id, key, type_code, name, config) = row?;
            let service_type = ServiceType::from_code(type_code).ok_or_else(|| {
                StoreError::Corrupt(format!("service {id} has unknown type {type_code}"))
            })?;
            let kind = ServiceKind::from_stored(service_type, &config)?;
            services.push(Service {
                id,
                key,
                name,
                kind,
            });
        }
        Ok(Self::from_services(services))
    }

    fn from_services(services: Vec<Service>) -> Self {
        let services: Vec<Arc<Service>> = services.into_iter().map(Arc::new).collect();
        let by_id = services
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id, i))
            .collect();
        let by_key = services
            .iter()
            .enumerate()
            .map(|(i, s)| (s.key.clone(), i))
            .collect();
        Self {
            services,
            by_id,
            by_key,
        }
    }

    pub fn all(&self) -> impl Iterator<Item = &Arc<Service>> {
        self.services.iter()
    }

    pub fn get(&self, id: ServiceId) -> Result<&Arc<Service>> {
        self.by_id
            .get(&id)
            .map(|&i| &self.services[i])
            .ok_or(StoreError::NoSuchService(id))
    }

    pub fn by_key(&self, key: &ServiceKey) -> Result<&Arc<Service>> {
        self.by_key
            .get(key)
            .map(|&i| &self.services[i])
            .ok_or_else(|| StoreError::NoSuchServiceKey(key.to_hex()))
    }

    pub fn by_name(&self, name: &str) -> Option<&Arc<Service>> {
        self.services.iter().find(|s| s.name == name)
    }

    pub fn of_type(&self, service_type: ServiceType) -> impl Iterator<Item = &Arc<Service>> {
        self.services
            .iter()
            .filter(move |s| s.service_type() == service_type)
    }

    /// The single service of a built-in kind, looked up by its fixed key.
    pub fn builtin(&self, key: &[u8]) -> Result<&Arc<Service>> {
        self.by_key(&ServiceKey::new(key.to_vec()))
    }

    /// Every real tag service (local tag domains and tag repositories).
    pub fn tag_services(&self) -> impl Iterator<Item = &Arc<Service>> {
        self.services.iter().filter(|s| s.kind.has_mappings())
    }
}

/// Insert a service row, creating its per-service tables.
pub fn insert(
    conn: &Connection,
    key: &ServiceKey,
    name: &str,
    kind: &ServiceKind,
) -> Result<ServiceId> {
    conn.execute(
        "INSERT INTO services (service_key, service_type, name, config) VALUES (?, ?, ?, ?)",
        params![key, kind.service_type().code(), name, kind.config_json()?],
    )?;
    let id = ServiceId(
        u32::try_from(conn.last_insert_rowid())
            .map_err(|_| StoreError::Corrupt("service id out of range".into()))?,
    );
    if kind.has_mappings() {
        schema::create_tag_service_tables(conn, id)?;
    }
    Ok(id)
}

/// Insert a service row with a specific id (used by the importer so ids match
/// the source database).
pub fn insert_with_id(
    conn: &Connection,
    id: ServiceId,
    key: &ServiceKey,
    name: &str,
    kind: &ServiceKind,
) -> Result<()> {
    conn.execute(
        "INSERT INTO services (service_id, service_key, service_type, name, config) VALUES (?, ?, ?, ?, ?)",
        params![id, key, kind.service_type().code(), name, kind.config_json()?],
    )?;
    if kind.has_mappings() {
        schema::create_tag_service_tables(conn, id)?;
    }
    Ok(())
}

/// Delete a service row and its per-service tables. Callers must first remove
/// the service's content from shared tables (file domains, ratings, ...).
pub fn delete(conn: &Connection, id: ServiceId) -> Result<()> {
    conn.execute("DELETE FROM services WHERE service_id = ?", [id])?;
    conn.execute("DELETE FROM tag_display_application WHERE display_service_id = ?1 OR source_service_id = ?1", [id])?;
    schema::drop_tag_service_tables(conn, id)?;
    Ok(())
}

/// The services every new client starts with, mirroring the reference's
/// first-boot set (same names and keys).
pub fn default_services() -> Vec<(ServiceKey, String, ServiceKind)> {
    let favourites = LikeRatingConfig {
        display: RatingDisplay {
            colours: RatingColours {
                like: PenBrush {
                    pen: Rgb([0, 0, 0]),
                    brush: Rgb([240, 240, 65]),
                },
                dislike: PenBrush {
                    pen: Rgb([0, 0, 0]),
                    brush: Rgb([200, 80, 120]),
                },
                ..RatingColours::default()
            },
            ..RatingDisplay::default()
        },
        appearance: StarAppearance::Shape(StarShape::FAT_STAR),
    };
    let key = |k: &[u8]| ServiceKey::new(k.to_vec());
    vec![
        (
            key(builtin_keys::COMBINED_TAG),
            "all known tags".into(),
            ServiceKind::AllKnownTags,
        ),
        (
            key(builtin_keys::COMBINED_FILE),
            "all known files".into(),
            ServiceKind::AllKnownFiles,
        ),
        (
            key(builtin_keys::COMBINED_DELETED_FILE),
            "deleted from anywhere".into(),
            ServiceKind::CombinedDeleted,
        ),
        (
            key(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE),
            "hydrus local file storage".into(),
            ServiceKind::LocalFileStorage,
        ),
        (
            key(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS),
            "combined local file domains".into(),
            ServiceKind::CombinedLocalMedia,
        ),
        (
            key(builtin_keys::MY_FILES),
            "my files".into(),
            ServiceKind::LocalFiles,
        ),
        (
            key(builtin_keys::LOCAL_UPDATE),
            "repository updates".into(),
            ServiceKind::LocalUpdates,
        ),
        (key(builtin_keys::TRASH), "trash".into(), ServiceKind::Trash),
        (
            key(builtin_keys::MY_TAGS),
            "my tags".into(),
            ServiceKind::LocalTags,
        ),
        (
            key(builtin_keys::DOWNLOADER_TAGS),
            "downloader tags".into(),
            ServiceKind::LocalTags,
        ),
        (
            key(builtin_keys::LOCAL_NOTES),
            "local notes".into(),
            ServiceKind::LocalNotes,
        ),
        (
            key(builtin_keys::FAVOURITES),
            "favourites".into(),
            ServiceKind::RatingLike(favourites),
        ),
        (
            key(builtin_keys::CLIENT_API),
            "client api".into(),
            ServiceKind::ClientApi(ServerConfig::default()),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        schema::configure(&conn).unwrap();
        schema::migrate(&mut conn).unwrap();
        conn
    }

    #[test]
    fn default_services_round_trip() {
        let c = conn();
        for (key, name, kind) in default_services() {
            insert(&c, &key, &name, &kind).unwrap();
        }
        let reg = ServiceRegistry::load(&c).unwrap();
        assert_eq!(reg.all().count(), 13);
        let my_tags = reg.builtin(builtin_keys::MY_TAGS).unwrap();
        assert_eq!(my_tags.name, "my tags");
        assert_eq!(my_tags.id, ServiceId(9));
        assert_eq!(reg.tag_services().count(), 2);
        let fav = reg.builtin(builtin_keys::FAVOURITES).unwrap();
        assert!(
            matches!(&fav.kind, ServiceKind::RatingLike(c) if c.appearance == StarAppearance::Shape(StarShape::FAT_STAR))
        );
        // mapping tables exist for tag services
        c.execute("INSERT INTO mappings_9_current VALUES (1, 1)", [])
            .unwrap();
    }

    #[test]
    fn numerical_stars_match_reference_rounding() {
        let five = NumericalRatingConfig {
            display: RatingDisplay::default(),
            appearance: StarAppearance::Shape(StarShape::CIRCLE),
            num_stars: 5,
            allow_zero: true,
            custom_pad: 4,
            show_fraction_beside_stars: 0,
        };
        for stars in 0..=5 {
            assert_eq!(five.stars(five.rating(stars)), stars);
        }
        // 0.5 * 5 = 2.5 rounds to even
        assert_eq!(five.stars(0.5), 2);
        let no_zero = NumericalRatingConfig {
            allow_zero: false,
            ..five
        };
        for stars in 1..=5 {
            assert_eq!(no_zero.stars(no_zero.rating(stars)), stars);
        }
        assert_eq!(no_zero.min_stars(), 1);
    }

    #[test]
    fn colours_serialise_as_hex() {
        assert_eq!(
            serde_json::to_string(&Rgb([80, 200, 120])).unwrap(),
            "\"#50C878\""
        );
        assert_eq!(
            serde_json::from_str::<Rgb>("\"#50c878\"").unwrap(),
            Rgb([80, 200, 120])
        );
    }
}
