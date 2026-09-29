//! Services: the domains that files, tags, ratings and notes live in.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! service_types {
    ($( $variant:ident = $code:literal, $name:literal, $short:literal; )*) => {
        /// The kind of a service. Codes are stable and exposed by the Client API.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum ServiceType {
            $( $variant = $code, )*
        }

        impl ServiceType {
            pub const ALL: &'static [ServiceType] = &[ $( ServiceType::$variant, )* ];

            pub const fn from_code(code: u8) -> Option<ServiceType> {
                match code {
                    $( $code => Some(ServiceType::$variant), )*
                    _ => None,
                }
            }

            /// Long human-readable name, e.g. "local tag domain".
            pub const fn name(self) -> &'static str {
                match self { $( ServiceType::$variant => $name, )* }
            }

            /// Short human-readable name, e.g. "trash".
            pub const fn short_name(self) -> &'static str {
                match self { $( ServiceType::$variant => $short, )* }
            }
        }
    };
}

service_types! {
    TagRepository = 0, "hydrus tag repository", "tag repository";
    FileRepository = 1, "hydrus file repository", "file repository";
    LocalFileDomain = 2, "local file domain", "local file domain";
    MessageDepot = 3, "hydrus message depot", "hydrus message depot";
    LocalTag = 5, "local tag domain", "local tag domain";
    LocalRatingNumerical = 6, "local numerical rating service", "numerical ratings";
    LocalRatingLike = 7, "local like/dislike rating service", "like/dislike ratings";
    RatingNumericalRepository = 8, "hydrus numerical rating repository", "hydrus numerical rating repository";
    RatingLikeRepository = 9, "hydrus like/dislike rating repository", "hydrus like/dislike rating repository";
    CombinedTag = 10, "virtual combined tag domain", "all known tags";
    CombinedFile = 11, "virtual combined file domain", "all known files";
    LocalBooru = 12, "client local booru", "local booru";
    Ipfs = 13, "ipfs daemon", "ipfs";
    LocalFileTrashDomain = 14, "local trash file domain", "trash";
    HydrusLocalFileStorage = 15, "virtual combined local file domain", "hydrus local file storage";
    TestService = 16, "test service", "test service";
    LocalNotes = 17, "local file notes service", "notes";
    ClientApiService = 18, "client api", "client api";
    CombinedDeletedFile = 19, "virtual deleted file service", "virtual deleted file service";
    LocalFileUpdateDomain = 20, "local update file domain", "local update file domain";
    CombinedLocalFileDomains = 21, "virtual combined local media domain", "combined local file domains";
    LocalRatingIncDec = 22, "local inc/dec rating service", "inc/dec ratings";
    ServerAdmin = 99, "hydrus server administration service", "hydrus server administration";
    NullService = 100, "null service", "null service";
}

impl ServiceType {
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// Services whose content is tag mappings (real, not virtual).
    pub const fn is_real_tag_service(self) -> bool {
        matches!(self, ServiceType::LocalTag | ServiceType::TagRepository)
    }

    /// Any tag service, including the virtual "all known tags".
    pub const fn is_tag_service(self) -> bool {
        self.is_real_tag_service() || matches!(self, ServiceType::CombinedTag)
    }

    /// File domains the user can hold files in directly.
    pub const fn is_specific_local_file_service(self) -> bool {
        matches!(
            self,
            ServiceType::LocalFileDomain
                | ServiceType::LocalFileUpdateDomain
                | ServiceType::LocalFileTrashDomain
        )
    }

    /// Every service that describes a set of files.
    pub const fn is_file_service(self) -> bool {
        matches!(
            self,
            ServiceType::LocalFileDomain
                | ServiceType::LocalFileUpdateDomain
                | ServiceType::LocalFileTrashDomain
                | ServiceType::HydrusLocalFileStorage
                | ServiceType::CombinedLocalFileDomains
                | ServiceType::CombinedDeletedFile
                | ServiceType::CombinedFile
                | ServiceType::FileRepository
                | ServiceType::Ipfs
        )
    }

    pub const fn is_remote_file_service(self) -> bool {
        matches!(self, ServiceType::FileRepository | ServiceType::Ipfs)
    }

    pub const fn is_rating_service(self) -> bool {
        matches!(
            self,
            ServiceType::LocalRatingLike
                | ServiceType::LocalRatingNumerical
                | ServiceType::LocalRatingIncDec
                | ServiceType::RatingLikeRepository
                | ServiceType::RatingNumericalRepository
        )
    }

    pub const fn is_local_rating_service(self) -> bool {
        matches!(
            self,
            ServiceType::LocalRatingLike
                | ServiceType::LocalRatingNumerical
                | ServiceType::LocalRatingIncDec
        )
    }

    pub const fn is_repository(self) -> bool {
        matches!(
            self,
            ServiceType::TagRepository
                | ServiceType::FileRepository
                | ServiceType::RatingLikeRepository
                | ServiceType::RatingNumericalRepository
        )
    }

    /// Whether the user may create/delete services of this type.
    pub const fn is_user_addable(self) -> bool {
        matches!(
            self,
            ServiceType::LocalTag
                | ServiceType::LocalFileDomain
                | ServiceType::LocalRatingLike
                | ServiceType::LocalRatingNumerical
                | ServiceType::LocalRatingIncDec
                | ServiceType::TagRepository
                | ServiceType::FileRepository
                | ServiceType::Ipfs
        )
    }
}

impl fmt::Display for ServiceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Serialize for ServiceType {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for ServiceType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(d)?;
        ServiceType::from_code(code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown service type {code}")))
    }
}

/// The globally unique, stable identity of a service.
///
/// User-created services get 32 random bytes. The built-in services use short
/// ASCII keys (e.g. `b"local tags"`), which is why this is variable length.
/// Clients of the API refer to services by the hex of this key, so it must be
/// preserved exactly across migration.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServiceKey(Box<[u8]>);

impl ServiceKey {
    pub fn new(bytes: impl Into<Box<[u8]>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        hex::decode(s).map(|v| Self(v.into_boxed_slice()))
    }
}

/// Keys of the services every client has.
pub mod builtin_keys {
    pub const COMBINED_TAG: &[u8] = b"all known tags";
    pub const COMBINED_FILE: &[u8] = b"all known files";
    pub const COMBINED_DELETED_FILE: &[u8] = b"all deleted files";
    pub const HYDRUS_LOCAL_FILE_STORAGE: &[u8] = b"all local files";
    pub const COMBINED_LOCAL_FILE_DOMAINS: &[u8] = b"all local media";
    pub const MY_FILES: &[u8] = b"local files";
    pub const LOCAL_UPDATE: &[u8] = b"repository updates";
    pub const TRASH: &[u8] = b"trash";
    pub const MY_TAGS: &[u8] = b"local tags";
    pub const DOWNLOADER_TAGS: &[u8] = b"downloader tags";
    pub const LOCAL_NOTES: &[u8] = b"local notes";
    pub const CLIENT_API: &[u8] = b"client api";
    pub const FAVOURITES: &[u8] = b"favourites";
}

impl fmt::Debug for ServiceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match std::str::from_utf8(&self.0) {
            Ok(s) if s.bytes().all(|b| b.is_ascii_graphic() || b == b' ') => {
                write!(f, "ServiceKey({s:?})")
            }
            _ => write!(f, "ServiceKey({})", self.to_hex()),
        }
    }
}

impl fmt::Display for ServiceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for ServiceKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ServiceKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        ServiceKey::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "sqlite")]
impl rusqlite::ToSql for ServiceKey {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Borrowed(
            rusqlite::types::ValueRef::Blob(&self.0),
        ))
    }
}

#[cfg(feature = "sqlite")]
impl rusqlite::types::FromSql for ServiceKey {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(ServiceKey::new(value.as_blob()?.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::constants;

    #[test]
    fn table_matches_reference_implementation() {
        let fixture = constants();
        let types = fixture["service_types"].as_array().unwrap();
        assert_eq!(types.len(), ServiceType::ALL.len());
        for t in types {
            let code = u8::try_from(t["id"].as_u64().unwrap()).unwrap();
            let st = ServiceType::from_code(code).unwrap_or_else(|| panic!("missing {code}"));
            assert_eq!(st.name(), t["string"].as_str().unwrap());
            assert_eq!(st.short_name(), t["short"].as_str().unwrap());
        }
    }

    #[test]
    fn builtin_keys_render_readably() {
        let key = ServiceKey::new(builtin_keys::MY_TAGS.to_vec());
        assert_eq!(key.to_hex(), "6c6f63616c2074616773");
        assert_eq!(format!("{key:?}"), "ServiceKey(\"local tags\")");
    }
}
