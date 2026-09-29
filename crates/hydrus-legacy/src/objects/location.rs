//! Search domains: which files (`LocationContext`, type 103) and which tags
//! (`TagContext`, type 80) a search or sort looks at.

use hydrus_core::ServiceKey;
use hydrus_core::service::builtin_keys;

use super::util::{DecodeResult, boolean, nested, service_key, service_keys, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableError, SerialisableObject, SerialisableType};

/// A file domain for searching: files current in any of `current`, or
/// deleted from any of `deleted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationContext {
    /// In stored order (the reference keeps sets, so the order is arbitrary).
    pub current: Vec<ServiceKey>,
    pub deleted: Vec<ServiceKey>,
}

impl LocationContext {
    const KIND: SerialisableType = SerialisableType::LOCATION_CONTEXT;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        let [current, deleted] = tuple::<2>(Self::KIND, &info, "location context")?;
        Ok(LocationContext {
            current: service_keys(Self::KIND, current, "current service keys")?,
            deleted: service_keys(Self::KIND, deleted, "deleted service keys")?,
        })
    }

    pub fn from_tuple(value: &PyJson) -> DecodeResult<Self> {
        Self::from_object(&nested(Self::KIND, value, "location context")?)
    }

    /// A location context of one file service, as the reference creates in
    /// upgrades (`LocationContext.STATICCreateSimple`).
    pub fn simple(service_key: ServiceKey) -> Self {
        LocationContext {
            current: vec![service_key],
            deleted: Vec::new(),
        }
    }
}

/// A tag domain for searching or sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagContext {
    pub service_key: ServiceKey,
    pub include_current_tags: bool,
    pub include_pending_tags: bool,
    /// Whose sibling/parent display rules apply.
    pub display_service_key: ServiceKey,
}

impl TagContext {
    const KIND: SerialisableType = SerialisableType::TAG_CONTEXT;

    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(Self::KIND)?;
        object.check_not_future()?;
        let info = object.info();
        match object.version {
            1 => {
                // v2 added the display service, defaulting to the search service
                let [key, current, pending] = tuple::<3>(Self::KIND, &info, "tag context")?;
                let service_key = service_key(Self::KIND, key, "service key")?;
                Ok(TagContext {
                    display_service_key: service_key.clone(),
                    service_key,
                    include_current_tags: boolean(Self::KIND, current, "include current")?,
                    include_pending_tags: boolean(Self::KIND, pending, "include pending")?,
                })
            }
            2 => {
                let [key, current, pending, display] =
                    tuple::<4>(Self::KIND, &info, "tag context")?;
                Ok(TagContext {
                    service_key: service_key(Self::KIND, key, "service key")?,
                    include_current_tags: boolean(Self::KIND, current, "include current")?,
                    include_pending_tags: boolean(Self::KIND, pending, "include pending")?,
                    display_service_key: service_key(Self::KIND, display, "display service key")?,
                })
            }
            version => Err(SerialisableError::UnsupportedVersion {
                kind: Self::KIND,
                version,
                detail: "unknown tag context version",
            }),
        }
    }

    pub fn from_tuple(value: &PyJson) -> DecodeResult<Self> {
        Self::from_object(&nested(Self::KIND, value, "tag context")?)
    }

    /// "all known tags", current and pending: the reference's default.
    pub fn all_known_tags() -> Self {
        let key = ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec());
        TagContext {
            service_key: key.clone(),
            include_current_tags: true,
            include_pending_tags: true,
            display_service_key: key,
        }
    }
}
