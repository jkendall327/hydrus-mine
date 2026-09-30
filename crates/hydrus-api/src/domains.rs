//! The file domain and tag domain parameters many endpoints share.
//!
//! A file domain is given by `file_service_key(s)` (files currently in any
//! of them) and `deleted_file_service_key(s)` (files deleted from any of
//! them); a tag domain by `tag_service_key`. Unknown keys and keys of the
//! wrong kind of service are 400s.

use hydrus_core::service::builtin_keys;
use hydrus_core::{ServiceId, ServiceKey};
use hydrus_store::services::{Service, ServiceRegistry};

use crate::error::{ApiError, ApiResult};
use crate::params::Params;

/// A set of files: those currently in any of `current` or deleted from any
/// of `deleted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDomain {
    pub current: Vec<ServiceId>,
    pub deleted: Vec<ServiceId>,
}

impl FileDomain {
    /// Whether this is "all known files" (which absorbs every other domain).
    pub fn is_all_known_files(&self, registry: &ServiceRegistry) -> bool {
        registry
            .builtin(builtin_keys::COMBINED_FILE)
            .is_ok_and(|akf| self.current == [akf.id])
    }
}

fn file_service<'a>(registry: &'a ServiceRegistry, key: &ServiceKey) -> ApiResult<&'a Service> {
    let service = registry.by_key(key).map_err(|_| {
        ApiError::bad_request(format!(
            "Could not find the file service \"{}\"!",
            key.to_hex()
        ))
    })?;
    if !service.service_type().is_file_service() {
        return Err(ApiError::bad_request(format!(
            "Sorry, the service key \"{}\" did not give a file service!",
            key.to_hex()
        )));
    }
    Ok(service)
}

/// The file domain a request names, or the service with key `default` if
/// it names none.
pub fn parse_file_domain(
    registry: &ServiceRegistry,
    params: &Params,
    default: &[u8],
    deleted_allowed: bool,
) -> ApiResult<FileDomain> {
    let keys = |single: &str, list: &str| -> ApiResult<Vec<ServiceKey>> {
        let mut out: Vec<ServiceKey> = params.optional::<ServiceKey>(single)?.into_iter().collect();
        if let Some(many) = params.optional::<Vec<Vec<u8>>>(list)? {
            out.extend(many.into_iter().map(ServiceKey::new));
        }
        Ok(out)
    };
    let current_keys = keys("file_service_key", "file_service_keys")?;
    let deleted_keys = if deleted_allowed {
        keys("deleted_file_service_key", "deleted_file_service_keys")?
    } else {
        Vec::new()
    };
    let resolve = |keys: &[ServiceKey]| -> ApiResult<Vec<ServiceId>> {
        let mut ids = keys
            .iter()
            .map(|k| file_service(registry, k).map(|s| s.id))
            .collect::<ApiResult<Vec<_>>>()?;
        ids.sort_unstable();
        ids.dedup();
        Ok(ids)
    };
    let mut domain = FileDomain {
        current: resolve(&current_keys)?,
        deleted: resolve(&deleted_keys)?,
    };
    if domain.current.is_empty() && domain.deleted.is_empty() {
        let default = file_service(registry, &ServiceKey::new(default.to_vec()))?;
        domain.current.push(default.id);
    }
    if let Ok(akf) = registry.builtin(builtin_keys::COMBINED_FILE)
        && domain.current.contains(&akf.id)
    {
        domain = FileDomain {
            current: vec![akf.id],
            deleted: Vec::new(),
        };
    }
    Ok(domain)
}

/// A tag domain: one tag service, or `None` for all known tags.
pub fn parse_tag_service(
    registry: &ServiceRegistry,
    params: &Params,
    name: &str,
) -> ApiResult<Option<ServiceId>> {
    let Some(key) = params.optional::<ServiceKey>(name)? else {
        return Ok(None);
    };
    let service = registry.by_key(&key).map_err(|_| {
        ApiError::bad_request(format!(
            "Could not find the tag service \"{}\"!",
            key.to_hex()
        ))
    })?;
    let service_type = service.service_type();
    if !service_type.is_tag_service() {
        return Err(ApiError::bad_request(format!(
            "Sorry, the service key \"{}\" did not give a tag service!",
            key.to_hex()
        )));
    }
    // "all known tags" is the only tag service that isn't real
    Ok(service_type.is_real_tag_service().then_some(service.id))
}
