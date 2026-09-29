//! Client API access keys and their permissions (`ClientAPI.APIManager`,
//! type 75, holding `ClientAPI.APIPermissions`, type 76).
//!
//! Access keys must be carried over exactly: they are what API clients
//! (browser extensions, downloaders) authenticate with.

use std::collections::BTreeSet;

use super::tag_filter::TagFilter;
use super::util::{DecodeResult, boolean, hex_bytes, int, list, malformed, nested, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableError, SerialisableObject, SerialisableType};

macro_rules! permissions {
    ($( $variant:ident = $code:literal, $label:literal; )*) => {
        /// A basic Client API permission (`ClientAPI.CLIENT_API_PERMISSION_*`).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum ApiPermission {
            $( $variant, )*
        }

        impl ApiPermission {
            pub const ALL: &'static [ApiPermission] = &[ $( ApiPermission::$variant, )* ];

            pub const fn code(self) -> i64 {
                match self { $( ApiPermission::$variant => $code, )* }
            }

            pub const fn from_code(code: i64) -> Option<ApiPermission> {
                match code {
                    $( $code => Some(ApiPermission::$variant), )*
                    _ => None,
                }
            }

            /// The reference's human-readable description.
            pub const fn label(self) -> &'static str {
                match self { $( ApiPermission::$variant => $label, )* }
            }
        }
    };
}

permissions! {
    AddUrls = 0, "import and edit urls";
    AddFiles = 1, "import and delete files";
    AddTags = 2, "edit file tags";
    SearchFiles = 3, "search for and fetch files";
    ManagePages = 4, "manage pages";
    ManageHeaders = 5, "manage cookies and headers";
    ManageDatabase = 6, "manage database";
    AddNotes = 7, "edit file notes";
    ManageFileRelationships = 8, "edit file relationships";
    EditRatings = 9, "edit file ratings";
    ManagePopups = 10, "manage popups";
    EditTimes = 11, "edit file times";
    CommitPending = 12, "commit pending";
    SeeLocalPaths = 13, "see local file paths";
}

/// Everything the Client API remembers: one entry per access key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientApiManager {
    /// In stored order.
    pub permissions: Vec<ApiPermissions>,
}

impl ClientApiManager {
    const KIND: SerialisableType = SerialisableType::CLIENT_API_MANAGER;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        let permissions = list(Self::KIND, &info, "permissions")?
            .iter()
            .map(|p| ApiPermissions::from_object(&nested(Self::KIND, p, "permissions")?))
            .collect::<DecodeResult<_>>()?;
        Ok(ClientApiManager { permissions })
    }
}

/// One access key and what it may do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiPermissions {
    /// The user's label for this key.
    pub name: String,
    pub access_key: Vec<u8>,
    /// If set, every permission is granted and the tag filter is ignored.
    pub permits_everything: bool,
    pub basic_permissions: BTreeSet<ApiPermission>,
    /// Restricts which tags searches may use, when searching is permitted.
    pub search_tag_filter: TagFilter,
}

/// The permissions that meant "everything" when v2 introduced the explicit
/// flag; a v1 key holding all of them is upgraded to permit everything.
const V1_EVERYTHING: [ApiPermission; 10] = [
    ApiPermission::AddFiles,
    ApiPermission::AddTags,
    ApiPermission::AddUrls,
    ApiPermission::SearchFiles,
    ApiPermission::ManagePages,
    ApiPermission::ManageHeaders,
    ApiPermission::ManageDatabase,
    ApiPermission::AddNotes,
    ApiPermission::ManageFileRelationships,
    ApiPermission::EditRatings,
];

impl ApiPermissions {
    const KIND: SerialisableType = SerialisableType::CLIENT_API_PERMISSIONS;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let name = object
            .name
            .clone()
            .ok_or_else(|| malformed(Self::KIND, "permissions object has no name"))?;
        let info = object.info();
        let (key, everything, basic, filter) = match object.version {
            1 => {
                let [key, basic, filter] = tuple::<3>(Self::KIND, &info, "permissions")?;
                (key, None, basic, filter)
            }
            2 => {
                let [key, everything, basic, filter] =
                    tuple::<4>(Self::KIND, &info, "permissions")?;
                (key, Some(everything), basic, filter)
            }
            version => {
                return Err(SerialisableError::UnsupportedVersion {
                    kind: Self::KIND,
                    version,
                    detail: "unknown permissions version",
                });
            }
        };
        let basic_permissions = list(Self::KIND, basic, "basic permissions")?
            .iter()
            .map(|p| {
                let code = int(Self::KIND, p, "permission")?;
                ApiPermission::from_code(code)
                    .ok_or_else(|| malformed(Self::KIND, format!("unknown permission {code}")))
            })
            .collect::<DecodeResult<BTreeSet<_>>>()?;
        let permits_everything = match everything {
            Some(value) => boolean(Self::KIND, value, "permits everything")?,
            None => V1_EVERYTHING.iter().all(|p| basic_permissions.contains(p)),
        };
        Ok(ApiPermissions {
            name,
            access_key: hex_bytes(Self::KIND, key, "access key")?,
            permits_everything,
            basic_permissions,
            search_tag_filter: TagFilter::from_tuple(filter)?,
        })
    }

    pub fn from_tuple(value: &PyJson) -> DecodeResult<Self> {
        Self::from_object(&nested(Self::KIND, value, "permissions")?)
    }

    /// Whether this key may do `permission` (`APIPermissions._HasPermission`).
    pub fn has_permission(&self, permission: ApiPermission) -> bool {
        self.permits_everything || self.basic_permissions.contains(&permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_permissions_upgrade() {
        let all_old = r#"[76, "old", 1, ["abcd", [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], [44, 1, []]]]"#;
        let p = ApiPermissions::from_tuple(&PyJson::parse(all_old).unwrap()).unwrap();
        assert!(p.permits_everything);
        assert_eq!(p.access_key, vec![0xab, 0xcd]);

        let some = r#"[76, "old", 1, ["abcd", [3], [44, 1, []]]]"#;
        let p = ApiPermissions::from_tuple(&PyJson::parse(some).unwrap()).unwrap();
        assert!(!p.permits_everything);
        assert!(p.has_permission(ApiPermission::SearchFiles));
        assert!(!p.has_permission(ApiPermission::AddTags));
    }
}
