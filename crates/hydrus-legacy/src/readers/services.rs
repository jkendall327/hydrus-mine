//! The `services` table, with decoded settings.

use hydrus_core::{ServiceId, ServiceKey, ServiceType};

use super::column;
use crate::db::LegacyDb;
use crate::error::{LegacyError, Result};
use crate::objects::ServiceConfig;
use crate::serialisable::SerialisableObject;

/// A service: its identity and settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    pub id: ServiceId,
    /// Must be preserved exactly: API clients refer to services by key.
    pub key: ServiceKey,
    pub service_type: ServiceType,
    pub name: String,
    pub config: ServiceConfig,
    /// The stored settings dictionary, verbatim.
    pub dictionary: SerialisableObject,
}

impl LegacyDb {
    /// Every service, in id order, with decoded settings.
    pub fn services(&self) -> Result<Vec<Service>> {
        let mut statement = self.connection().prepare(
            "SELECT service_id, service_key, service_type, name, dictionary_string \
             FROM main.services ORDER BY service_id",
        )?;
        let mut rows = statement.query([])?;
        let mut services = Vec::new();
        while let Some(row) = rows.next()? {
            let id: ServiceId = column(row, 0, "services")?;
            let info = self.service_info(id)?.clone();
            let location = format!("services row {id} ({})", info.name);
            let text: String = column(row, 4, "services")?;
            let dictionary = SerialisableObject::from_tuple_str(&text)
                .map_err(|e| LegacyError::serialisable(&location, e))?;
            let config = ServiceConfig::decode(info.service_type, &dictionary)
                .map_err(|e| LegacyError::serialisable(&location, e))?;
            services.push(Service {
                id,
                key: info.key,
                service_type: info.service_type,
                name: info.name,
                config,
                dictionary,
            });
        }
        Ok(services)
    }
}
