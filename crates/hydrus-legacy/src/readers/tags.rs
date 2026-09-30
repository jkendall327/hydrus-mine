//! Tag mappings, siblings and parents, and how siblings and parents are
//! applied for display.
//!
//! Each real tag service (local tags and tag repositories) has four tables
//! of each kind, one per [`ContentStatus`]: `current_mappings_N`,
//! `deleted_mappings_N`, `pending_mappings_N`, `petitioned_mappings_N`,
//! and likewise `*_tag_siblings_N` and `*_tag_parents_N`. Petitions (and
//! pending siblings/parents) carry a reason.

use std::collections::BTreeMap;

use hydrus_core::{ContentStatus, HashId, ServiceId, ServiceType, TagId, TextId};

use super::column;
use crate::db::LegacyDb;
use crate::error::Result;
use crate::rows::{Paging, Rows};

/// A tag on a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapping {
    pub tag_id: TagId,
    pub hash_id: HashId,
    /// The petition reason; only petitioned mappings have one.
    pub reason: Option<TextId>,
}

/// A sibling (`from` is shown as `to`) or parent (`from` implies `to`) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagPair {
    /// The "bad" tag of a sibling pair, or the child of a parent pair.
    pub from: TagId,
    /// The "good" tag of a sibling pair, or the parent of a parent pair.
    pub to: TagId,
    /// Pending and petitioned pairs carry the reason given.
    pub reason: Option<TextId>,
}

fn status_prefix(status: ContentStatus) -> &'static str {
    match status {
        ContentStatus::Current => "current",
        ContentStatus::Deleted => "deleted",
        ContentStatus::Pending => "pending",
        ContentStatus::Petitioned => "petitioned",
    }
}

impl LegacyDb {
    /// Tag mappings of one tag service in one status.
    pub fn mappings(&self, service: ServiceId, status: ContentStatus) -> Result<Rows<'_, Mapping>> {
        let prefix = format!("{}_mappings", status_prefix(status));
        let table = self.service_table(
            service,
            "external_mappings",
            &prefix,
            ServiceType::is_real_tag_service,
            "a tag service",
        )?;
        let with_reason = status == ContentStatus::Petitioned;
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["tag_id", "hash_id"],
                columns: if with_reason { &["reason_id"] } else { &[] },
                join: "",
            },
            Box::new(move |row| {
                Ok(Mapping {
                    tag_id: column(row, 0, &name)?,
                    hash_id: column(row, 1, &name)?,
                    reason: if with_reason {
                        Some(column(row, 2, &name)?)
                    } else {
                        None
                    },
                })
            }),
        ))
    }

    fn tag_pairs(
        &self,
        service: ServiceId,
        status: ContentStatus,
        kind: &str,
        from: &'static str,
        to: &'static str,
    ) -> Result<Rows<'_, TagPair>> {
        let prefix = format!("{}_{kind}", status_prefix(status));
        let table = self.service_table(
            service,
            "main",
            &prefix,
            ServiceType::is_real_tag_service,
            "a tag service",
        )?;
        let with_reason = matches!(status, ContentStatus::Pending | ContentStatus::Petitioned);
        let name = table.clone();
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &[from, to],
                columns: if with_reason { &["reason_id"] } else { &[] },
                join: "",
            },
            Box::new(move |row| {
                Ok(TagPair {
                    from: column(row, 0, &name)?,
                    to: column(row, 1, &name)?,
                    reason: if with_reason {
                        Some(column(row, 2, &name)?)
                    } else {
                        None
                    },
                })
            }),
        ))
    }

    /// Tag siblings (`bad` -> `good`) of one tag service in one status.
    pub fn tag_siblings(
        &self,
        service: ServiceId,
        status: ContentStatus,
    ) -> Result<Rows<'_, TagPair>> {
        self.tag_pairs(service, status, "tag_siblings", "bad_tag_id", "good_tag_id")
    }

    /// Tag parents (`child` -> `parent`) of one tag service in one status.
    pub fn tag_parents(
        &self,
        service: ServiceId,
        status: ContentStatus,
    ) -> Result<Rows<'_, TagPair>> {
        self.tag_pairs(
            service,
            status,
            "tag_parents",
            "child_tag_id",
            "parent_tag_id",
        )
    }

    fn application(&self, table: &str) -> Result<BTreeMap<ServiceId, Vec<ServiceId>>> {
        let qualified = self.table("main", table)?;
        let mut statement = self.connection().prepare(&format!(
            "SELECT master_service_id, application_service_id FROM {qualified} \
             ORDER BY master_service_id, service_index"
        ))?;
        let mut rows = statement.query([])?;
        let mut out: BTreeMap<ServiceId, Vec<ServiceId>> = BTreeMap::new();
        while let Some(row) = rows.next()? {
            out.entry(column(row, 0, table)?)
                .or_default()
                .push(column(row, 1, table)?);
        }
        Ok(out)
    }

    /// For each tag service, whose siblings apply to its display tags, in
    /// priority order (`tag_sibling_application`).
    pub fn sibling_application(&self) -> Result<BTreeMap<ServiceId, Vec<ServiceId>>> {
        self.application("tag_sibling_application")
    }

    /// For each tag service, whose parents apply to its display tags, in
    /// priority order (`tag_parent_application`).
    pub fn parent_application(&self) -> Result<BTreeMap<ServiceId, Vec<ServiceId>>> {
        self.application("tag_parent_application")
    }
}
