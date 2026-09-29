//! File domains named by a request (`file_service_key(s)`,
//! `deleted_file_service_key(s)`), as the reference's `ParseLocationContext`.

use hydrus_core::{ServiceId, ServiceKey};
use hydrus_store::Snapshot;

use crate::error::{ApiError, ApiResult};
use crate::params::Params;

/// The file domains a request names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Domains {
    pub current: Vec<ServiceId>,
    pub deleted: Vec<ServiceId>,
}

/// The file domains a request names, or `None` if it names none. Deleted
/// domains are only read when `deleted_allowed`.
pub fn parse(
    snap: &Snapshot,
    params: &Params,
    deleted_allowed: bool,
) -> ApiResult<Option<Domains>> {
    let mut domains = Domains::default();
    let add = |list: &mut Vec<ServiceId>, key: &[u8]| -> ApiResult<()> {
        let id = check_file_service(snap, key)?;
        if !list.contains(&id) {
            list.push(id);
        }
        Ok(())
    };
    if let Some(key) = params.optional::<Vec<u8>>("file_service_key")? {
        add(&mut domains.current, &key)?;
    }
    for key in params.or::<Vec<Vec<u8>>>("file_service_keys", Vec::new())? {
        add(&mut domains.current, &key)?;
    }
    if deleted_allowed {
        if let Some(key) = params.optional::<Vec<u8>>("deleted_file_service_key")? {
            add(&mut domains.deleted, &key)?;
        }
        for key in params.or::<Vec<Vec<u8>>>("deleted_file_service_keys", Vec::new())? {
            add(&mut domains.deleted, &key)?;
        }
    }
    Ok((!domains.current.is_empty() || !domains.deleted.is_empty()).then_some(domains))
}

/// The id of the file service with this key (reference `CheckFileService`).
pub fn check_file_service(snap: &Snapshot, key: &[u8]) -> ApiResult<ServiceId> {
    let service = snap
        .services
        .by_key(&ServiceKey::new(key.to_vec()))
        .map_err(|_| {
            ApiError::bad_request(format!(
                "Could not find the file service \"{}\"!",
                hex::encode(key)
            ))
        })?;
    if !service.service_type().is_file_service() {
        return Err(ApiError::bad_request(format!(
            "Sorry, the service key \"{}\" did not give a file service!",
            hex::encode(key)
        )));
    }
    Ok(service.id)
}

/// The tag service with this key (reference `CheckTagService`).
pub fn check_tag_service<'s>(
    snap: &'s Snapshot,
    key: &[u8],
) -> ApiResult<&'s std::sync::Arc<hydrus_store::services::Service>> {
    let service = snap
        .services
        .by_key(&ServiceKey::new(key.to_vec()))
        .map_err(|_| {
            ApiError::bad_request(format!(
                "Could not find the tag service \"{}\"!",
                hex::encode(key)
            ))
        })?;
    if !service.service_type().is_tag_service() {
        return Err(ApiError::bad_request(format!(
            "Sorry, the service key \"{}\" did not give a tag service!",
            hex::encode(key)
        )));
    }
    Ok(service)
}
