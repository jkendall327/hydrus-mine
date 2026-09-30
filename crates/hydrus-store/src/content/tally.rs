//! Incremental maintenance of the derived tag counts (see [`crate::counts`]
//! for what they mean and the rebuild path).
//!
//! Every operation that changes mappings or file-domain membership reports
//! what changed, *against the database state at that moment*, and the
//! resulting count deltas accumulate here until the writer finishes. Because
//! each delta is computed against then-current state, any interleaving of
//! operations stays consistent; the property tests in `content::tests` check
//! that incremental counts always equal a rebuild.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};

use hydrus_core::{HashId, ServiceId, TagId};

use crate::display::DisplayGraph;
use crate::error::Result;
use crate::master::id_array;
use crate::schema::MappingTables;

/// Which count column a mapping status feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Column {
    Current,
    Pending,
}

impl Column {
    pub(crate) fn table(self, t: &MappingTables) -> &str {
        match self {
            Column::Current => &t.current,
            Column::Pending => &t.pending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    service: ServiceId,
    display: bool,
    domain: ServiceId,
    tag: TagId,
}

/// Accumulated count deltas.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    deltas: HashMap<Key, [i64; 2]>,
}

/// Some mappings of one tag flipping in one column: exactly the hashes whose
/// state changed, gaining (`sign` = 1) or losing (-1) the tag.
pub(crate) struct MappingChange<'h> {
    pub column: Column,
    pub tag: TagId,
    pub hashes: &'h [HashId],
    pub sign: i64,
}

/// What the tally needs to know about the file domains.
pub(crate) struct Domains<'a> {
    /// "All known files": counts every mapping, whatever the file's domains.
    pub all_known_files: Option<ServiceId>,
    /// Every other file domain; counts only files currently in it.
    pub counted: &'a HashSet<ServiceId>,
}

impl Tally {
    fn add(&mut self, key: Key, column: Column, delta: i64) {
        let entry = self.deltas.entry(key).or_default();
        entry[column as usize] += delta;
    }

    fn add_both(
        &mut self,
        service: ServiceId,
        domain: ServiceId,
        tag: TagId,
        column: Column,
        delta: i64,
    ) {
        for display in [false, true] {
            self.add(
                Key {
                    service,
                    display,
                    domain,
                    tag,
                },
                column,
                delta,
            );
        }
    }

    /// Some mappings changed in `service`.
    pub(crate) fn mapping_changed(
        &mut self,
        conn: &Connection,
        domains: &Domains<'_>,
        (service, graph): (ServiceId, &DisplayGraph),
        change: &MappingChange<'_>,
    ) -> Result<()> {
        let MappingChange {
            column,
            tag,
            hashes,
            sign,
        } = *change;
        if hashes.is_empty() {
            return Ok(());
        }
        let n = i64::try_from(hashes.len()).expect("fewer than 2^63 hashes");
        let touched = graph.touches(tag);
        let storage_and_maybe_display = |tally: &mut Self, domain: ServiceId, delta: i64| {
            if touched {
                tally.add(
                    Key {
                        service,
                        display: false,
                        domain,
                        tag,
                    },
                    column,
                    delta,
                );
            } else {
                tally.add_both(service, domain, tag, column, delta);
            }
        };

        if let Some(akf) = domains.all_known_files {
            storage_and_maybe_display(self, akf, sign * n);
        }
        let mut stmt = conn.prepare_cached(
            "SELECT service_id, count(*) FROM file_domain_current
             WHERE hash_id IN rarray(?1) GROUP BY service_id",
        )?;
        let rows = stmt.query_map([id_array(hashes)], |r| {
            Ok((r.get::<_, ServiceId>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (domain, count) = row?;
            if domains.counted.contains(&domain) {
                storage_and_maybe_display(self, domain, sign * count);
            }
        }
        if !touched {
            return Ok(());
        }

        // Display tags `tag` gives a file that none of its other tags do
        // appear or disappear with it.
        let display: Vec<TagId> = graph.display_tags(tag).collect();
        let tables = MappingTables::new(service);
        let mut other_tags = conn.prepare_cached(&format!(
            "SELECT tag_id FROM {} WHERE hash_id = ?1 AND tag_id != ?2",
            column.table(&tables)
        ))?;
        let mut file_domains =
            conn.prepare_cached("SELECT service_id FROM file_domain_current WHERE hash_id = ?1")?;
        for &hash in hashes {
            let mut covered: HashSet<TagId> = HashSet::new();
            for other in other_tags.query_map(params![hash, tag], |r| r.get::<_, TagId>(0))? {
                let other = other?;
                if graph.touches(other) {
                    covered.extend(graph.display_tags(other));
                }
            }
            let changed: Vec<TagId> = display
                .iter()
                .copied()
                .filter(|t| !covered.contains(t))
                .collect();
            if changed.is_empty() {
                continue;
            }
            let mut in_domains: Vec<ServiceId> = domains.all_known_files.into_iter().collect();
            for domain in file_domains.query_map([hash], |r| r.get::<_, ServiceId>(0))? {
                let domain = domain?;
                if domains.counted.contains(&domain) {
                    in_domains.push(domain);
                }
            }
            for &domain in &in_domains {
                for &t in &changed {
                    self.add(
                        Key {
                            service,
                            display: true,
                            domain,
                            tag: t,
                        },
                        column,
                        sign,
                    );
                }
            }
        }
        Ok(())
    }

    /// `hashes` entered (`sign` = 1) or left (-1) file domain `domain`: every
    /// tag they have in `service` gains or loses them there.
    pub(crate) fn domain_changed(
        &mut self,
        conn: &Connection,
        (service, graph): (ServiceId, &DisplayGraph),
        domain: ServiceId,
        hashes: &[HashId],
        sign: i64,
    ) -> Result<()> {
        let tables = MappingTables::new(service);
        for column in [Column::Current, Column::Pending] {
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT hash_id, tag_id FROM {} WHERE hash_id IN rarray(?1) ORDER BY hash_id",
                column.table(&tables)
            ))?;
            for chunk in hashes.chunks(4096) {
                let mut current_hash = None;
                let mut display: HashSet<TagId> = HashSet::new();
                let flush_display = |tally: &mut Self, display: &mut HashSet<TagId>| {
                    for t in display.drain() {
                        tally.add(
                            Key {
                                service,
                                display: true,
                                domain,
                                tag: t,
                            },
                            column,
                            sign,
                        );
                    }
                };
                let rows = stmt.query_map([id_array(chunk)], |r| {
                    Ok((r.get::<_, HashId>(0)?, r.get::<_, TagId>(1)?))
                })?;
                for row in rows {
                    let (hash, tag) = row?;
                    self.add(
                        Key {
                            service,
                            display: false,
                            domain,
                            tag,
                        },
                        column,
                        sign,
                    );
                    if current_hash != Some(hash) {
                        flush_display(self, &mut display);
                        current_hash = Some(hash);
                    }
                    display.extend(graph.display_tags(tag));
                }
                flush_display(self, &mut display);
            }
        }
        Ok(())
    }

    /// Write the accumulated deltas to the count tables.
    pub(crate) fn flush(&mut self, conn: &Connection) -> Result<()> {
        for (key, [current, pending]) in self.deltas.drain() {
            if current == 0 && pending == 0 {
                continue;
            }
            let tables = MappingTables::new(key.service);
            let table = if key.display {
                &tables.display_counts
            } else {
                &tables.counts
            };
            conn.prepare_cached(&format!(
                "INSERT INTO {table} (domain_id, tag_id, current, pending) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (domain_id, tag_id) DO UPDATE
                 SET current = current + excluded.current, pending = pending + excluded.pending"
            ))?
            .execute(params![key.domain, key.tag, current, pending])?;
            conn.prepare_cached(&format!(
                "DELETE FROM {table} WHERE domain_id = ?1 AND tag_id = ?2 AND current = 0 AND pending = 0"
            ))?
            .execute(params![key.domain, key.tag])?;
        }
        Ok(())
    }
}
