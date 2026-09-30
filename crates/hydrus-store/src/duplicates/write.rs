//! Changing file relationships: setting a pair's relationship, choosing a
//! duplicate group's king, and dropping potential pairs.
//!
//! The rules are the reference's, so a migrated database keeps meaning the
//! same thing. The model is described in the parent module. In short, setting
//! a relationship between two files acts on their groups:
//!
//! - *better* / *same quality* merge the two duplicate groups;
//! - *alternate* merges the two alternates groups (and records the pair as
//!   confirmed, so the similar-files search won't propose it again);
//! - *false positive* records that the two alternates groups are unrelated;
//! - *potential* proposes the pair for the duplicate filter.
//!
//! Every relationship but *potential* also removes the pair from the
//! potentials, whatever it did otherwise.

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension, params};

use hydrus_core::{HashId, ServiceId};

use super::GroupId;
use crate::error::Result;

/// An alternates group's id.
type AltGroupId = u32;

/// A relationship to set between two files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairRelationship {
    /// The files are not related.
    FalsePositive,
    /// The files are related (e.g. a costume change) but not duplicates.
    Alternate,
    /// The first file is a better duplicate of the second.
    Better,
    /// The files are duplicates of the same quality.
    SameQuality,
    /// The files may be duplicates; the duplicate filter should ask.
    Potential,
}

/// Writes to the relationship tables, on a connection inside a write
/// transaction.
#[derive(Debug)]
pub struct RelationshipWriter<'c> {
    conn: &'c Connection,
    /// "hydrus local file storage": potentials are only kept between groups
    /// whose king is stored here.
    local_storage: ServiceId,
}

impl<'c> RelationshipWriter<'c> {
    pub fn new(conn: &'c Connection, local_storage: ServiceId) -> Self {
        Self {
            conn,
            local_storage,
        }
    }

    /// Set the relationship between files `a` and `b`.
    pub fn set_pair(&self, relationship: PairRelationship, a: HashId, b: HashId) -> Result<()> {
        let mut group_a = self.group_or_create(a)?;
        let mut group_b = self.group_or_create(b)?;
        self.delete_potentials(&[ordered(group_a, group_b)])?;
        if a == b {
            return Ok(());
        }
        match relationship {
            PairRelationship::FalsePositive => {
                let alt_a = self.alt_group_or_create(group_a)?;
                let alt_b = self.alt_group_or_create(group_b)?;
                self.set_false_positive(alt_a, alt_b)?;
            }
            PairRelationship::Alternate => {
                if group_a == group_b {
                    // they were duplicates; the non-king leaves to become an
                    // alternate of the rest
                    let leaver = if self.king(group_a)? == a { b } else { a };
                    self.remove_group_member(leaver)?;
                    group_a = self.group_or_create(a)?;
                    group_b = self.group_or_create(b)?;
                }
                self.set_alternates(group_a, group_b)?;
            }
            PairRelationship::Better => {
                if group_a == group_b {
                    if self.king(group_b)? == b {
                        self.set_king_of(group_a, a)?;
                    }
                } else {
                    // a worse file that isn't its group's king comes over
                    // alone; its group's other files aren't known to be worse
                    if self.king(group_b)? != b {
                        self.remove_group_member(b)?;
                        group_b = self.group_or_create(b)?;
                    }
                    self.set_duplicates(group_a, group_b)?;
                }
            }
            PairRelationship::SameQuality => {
                if group_a != group_b {
                    let a_is_king = self.king(group_a)? == a;
                    let b_is_king = self.king(group_b)? == b;
                    let (superior, mergee) = match (a_is_king, b_is_king) {
                        (false, false) => {
                            self.remove_group_member(b)?;
                            (group_a, self.group_or_create(b)?)
                        }
                        (_, true) => (group_a, group_b),
                        (true, false) => (group_b, group_a),
                    };
                    self.set_duplicates(superior, mergee)?;
                }
            }
            PairRelationship::Potential => {
                self.add_potentials(group_a, &[(group_b, 0)])?;
            }
        }
        Ok(())
    }

    /// Make `hash_id` the king of its duplicate group.
    pub fn set_king(&self, hash_id: HashId) -> Result<()> {
        let group = self.group_or_create(hash_id)?;
        self.set_king_of(group, hash_id)
    }

    /// Drop every potential pair involving `hash_id`'s duplicate group.
    pub fn remove_potentials(&self, hash_id: HashId) -> Result<()> {
        if let Some(group) = super::group_of(self.conn, hash_id)? {
            self.delete_potentials_of(group)?;
        }
        Ok(())
    }

    /// The similar-files search found `found` (files and their distances)
    /// for `hash_id`: propose each one's duplicate group as a potential
    /// duplicate of `hash_id`'s (`AddPotentialDuplicates`), the first
    /// distance found for a group standing.
    pub fn add_similar_files(&self, hash_id: HashId, found: &[(HashId, u32)]) -> Result<()> {
        let group = self.group_or_create(hash_id)?;
        let mut others: Vec<(GroupId, u32)> = Vec::new();
        for &(other, distance) in found {
            let other = self.group_or_create(other)?;
            if !others.iter().any(|(g, _)| *g == other) {
                others.push((other, distance));
            }
        }
        self.add_potentials(group, &others)
    }

    // groups -------------------------------------------------------------

    /// `hash_id`'s duplicate group, creating a group of one if it has none.
    fn group_or_create(&self, hash_id: HashId) -> Result<GroupId> {
        if let Some(group) = super::group_of(self.conn, hash_id)? {
            return Ok(group);
        }
        let group: GroupId = self
            .conn
            .prepare_cached("INSERT INTO dup_groups (king_hash_id) VALUES (?) RETURNING group_id")?
            .query_row([hash_id], |r| r.get(0))?;
        self.conn
            .prepare_cached("INSERT INTO dup_group_members (group_id, hash_id) VALUES (?, ?)")?
            .execute(params![group, hash_id])?;
        Ok(group)
    }

    fn king(&self, group: GroupId) -> Result<HashId> {
        Ok(super::king_of(self.conn, group)?.expect("every duplicate group has a king"))
    }

    fn set_king_of(&self, group: GroupId, hash_id: HashId) -> Result<()> {
        self.conn
            .prepare_cached("UPDATE dup_groups SET king_hash_id = ? WHERE group_id = ?")?
            .execute(params![hash_id, group])?;
        Ok(())
    }

    /// Take `hash_id` out of its duplicate group. A king takes the whole
    /// group apart, as there is no one to succeed it.
    fn remove_group_member(&self, hash_id: HashId) -> Result<()> {
        let Some(group) = super::group_of(self.conn, hash_id)? else {
            return Ok(());
        };
        if self.king(group)? == hash_id {
            self.dissolve_group(group)
        } else {
            self.conn
                .prepare_cached("DELETE FROM dup_group_members WHERE hash_id = ?")?
                .execute([hash_id])?;
            self.reset_search(&[hash_id])
        }
    }

    fn dissolve_group(&self, group: GroupId) -> Result<()> {
        self.remove_alternate_member(group)?;
        self.delete_potentials_of(group)?;
        let members = super::members_of(self.conn, group)?;
        self.conn
            .prepare_cached("DELETE FROM dup_group_members WHERE group_id = ?")?
            .execute([group])?;
        self.conn
            .prepare_cached("DELETE FROM dup_groups WHERE group_id = ?")?
            .execute([group])?;
        self.reset_search(&members)
    }

    /// Fold duplicate group `mergee` into `superior`, whose king stays king.
    fn set_duplicates(&self, superior: GroupId, mergee: GroupId) -> Result<()> {
        if superior == mergee {
            return Ok(());
        }
        let superior_alt = self.alt_group_or_create(superior)?;
        if let Some(mergee_alt) = super::alternates_group_of(self.conn, mergee)?
            && mergee_alt != superior_alt
        {
            self.set_alternates(superior, mergee)?;
        }
        // the mergee's potentials and confirmed alternates become the superior's
        let potentials: Vec<(GroupId, u32)> = self
            .query_pairs(
                "SELECT smaller_group_id, distance FROM potential_pairs WHERE larger_group_id = ?1
                 UNION SELECT larger_group_id, distance FROM potential_pairs WHERE smaller_group_id = ?1",
                mergee,
            )?
            .into_iter()
            .filter(|(other, _)| *other != mergee && *other != superior)
            .collect();
        if !potentials.is_empty() {
            self.add_potentials(superior, &potentials)?;
        }
        for other in self.confirmed_alternates(mergee)? {
            if other != mergee && other != superior {
                self.set_alternates(superior, other)?;
            }
        }
        self.conn
            .prepare_cached("UPDATE dup_group_members SET group_id = ? WHERE group_id = ?")?
            .execute(params![superior, mergee])?;
        self.dissolve_group(mergee)
    }

    // alternates ---------------------------------------------------------

    fn alt_group_or_create(&self, group: GroupId) -> Result<AltGroupId> {
        if let Some(alt) = super::alternates_group_of(self.conn, group)? {
            return Ok(alt);
        }
        let alt: AltGroupId = self
            .conn
            .prepare_cached("INSERT INTO alt_groups DEFAULT VALUES RETURNING alt_group_id")?
            .query_row([], |r| r.get(0))?;
        self.conn
            .prepare_cached("INSERT INTO alt_group_members (alt_group_id, group_id) VALUES (?, ?)")?
            .execute(params![alt, group])?;
        Ok(alt)
    }

    /// Put duplicate groups `superior` and `mergee` in one alternates group
    /// (`superior`'s), and confirm them as alternates.
    fn set_alternates(&self, superior: GroupId, mergee: GroupId) -> Result<()> {
        if superior == mergee {
            return Ok(());
        }
        let pair = ordered(superior, mergee);
        self.delete_potentials(&[pair])?;
        let alt_a = self.alt_group_or_create(superior)?;
        let alt_b = self.alt_group_or_create(mergee)?;
        if alt_a != alt_b {
            // anything B was unrelated to, A is now unrelated to
            for other in super::false_positive_alternates_groups(self.conn, alt_b)? {
                if other != alt_a && other != alt_b {
                    self.set_false_positive(alt_a, other)?;
                }
            }
            self.conn
                .prepare_cached(
                    "UPDATE alt_group_members SET alt_group_id = ? WHERE alt_group_id = ?",
                )?
                .execute(params![alt_a, alt_b])?;
            self.dissolve_alt_group(alt_b)?;
        }
        self.conn
            .prepare_cached(
                "INSERT OR IGNORE INTO alt_confirmed_pairs (smaller_group_id, larger_group_id) VALUES (?, ?)",
            )?
            .execute(params![pair.0, pair.1])?;
        Ok(())
    }

    fn confirmed_alternates(&self, group: GroupId) -> Result<Vec<GroupId>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT smaller_group_id FROM alt_confirmed_pairs WHERE larger_group_id = ?1
             UNION SELECT larger_group_id FROM alt_confirmed_pairs WHERE smaller_group_id = ?1",
        )?;
        let rows = stmt.query_map([group], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Take duplicate group `group` out of its alternates group.
    fn remove_alternate_member(&self, group: GroupId) -> Result<()> {
        let Some(alt) = super::alternates_group_of(self.conn, group)? else {
            return Ok(());
        };
        let was_last = super::groups_in_alternates_group(self.conn, alt)?.len() == 1;
        self.conn
            .prepare_cached("DELETE FROM alt_group_members WHERE group_id = ?")?
            .execute([group])?;
        self.conn
            .prepare_cached(
                "DELETE FROM alt_confirmed_pairs WHERE smaller_group_id = ?1 OR larger_group_id = ?1",
            )?
            .execute([group])?;
        if was_last {
            self.delete_alt_group_row(alt)?;
        }
        self.reset_search(&super::members_of(self.conn, group)?)
    }

    fn dissolve_alt_group(&self, alt: AltGroupId) -> Result<()> {
        for group in super::groups_in_alternates_group(self.conn, alt)? {
            self.dissolve_group(group)?;
        }
        self.delete_alt_group_row(alt)
    }

    fn delete_alt_group_row(&self, alt: AltGroupId) -> Result<()> {
        self.conn
            .prepare_cached("DELETE FROM alt_groups WHERE alt_group_id = ?")?
            .execute([alt])?;
        self.conn
            .prepare_cached(
                "DELETE FROM false_positive_pairs WHERE smaller_alt_group_id = ?1 OR larger_alt_group_id = ?1",
            )?
            .execute([alt])?;
        Ok(())
    }

    fn set_false_positive(&self, alt_a: AltGroupId, alt_b: AltGroupId) -> Result<()> {
        if alt_a == alt_b {
            return Ok(());
        }
        let groups_a: BTreeSet<GroupId> = super::groups_in_alternates_group(self.conn, alt_a)?
            .into_iter()
            .collect();
        let groups_b: BTreeSet<GroupId> = super::groups_in_alternates_group(self.conn, alt_b)?
            .into_iter()
            .collect();
        self.clear_potentials_between(&groups_a, &groups_b)?;
        let (smaller, larger) = ordered(alt_a, alt_b);
        self.conn
            .prepare_cached(
                "INSERT OR IGNORE INTO false_positive_pairs (smaller_alt_group_id, larger_alt_group_id) VALUES (?, ?)",
            )?
            .execute(params![smaller, larger])?;
        Ok(())
    }

    // potentials ---------------------------------------------------------

    /// Propose `group` as a potential duplicate of each of `others`, unless
    /// that is already settled or either king isn't stored locally.
    fn add_potentials(&self, group: GroupId, others: &[(GroupId, u32)]) -> Result<()> {
        if !self.is_stored(self.king(group)?)? {
            return Ok(());
        }
        let alt = super::alternates_group_of(self.conn, group)?;
        let mut insert = self.conn.prepare_cached(
            "INSERT OR IGNORE INTO potential_pairs (smaller_group_id, larger_group_id, distance) VALUES (?, ?, ?)",
        )?;
        for &(other, distance) in others {
            if other == group || !self.is_stored(self.king(other)?)? {
                continue;
            }
            if let (Some(alt), Some(other_alt)) =
                (alt, super::alternates_group_of(self.conn, other)?)
                && self.are_false_positives(alt, other_alt)?
            {
                continue;
            }
            let pair = ordered(group, other);
            if self.are_confirmed_alternates(pair)? {
                continue;
            }
            insert.execute(params![pair.0, pair.1, distance])?;
        }
        Ok(())
    }

    fn delete_potentials(&self, pairs: &[(GroupId, GroupId)]) -> Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "DELETE FROM potential_pairs WHERE smaller_group_id = ? AND larger_group_id = ?",
        )?;
        for (smaller, larger) in pairs {
            stmt.execute(params![smaller, larger])?;
        }
        Ok(())
    }

    fn delete_potentials_of(&self, group: GroupId) -> Result<()> {
        self.conn
            .prepare_cached(
                "DELETE FROM potential_pairs WHERE smaller_group_id = ?1 OR larger_group_id = ?1",
            )?
            .execute([group])?;
        Ok(())
    }

    /// Drop potentials between a group in `a` and a group in `b` (but not
    /// within either).
    fn clear_potentials_between(&self, a: &BTreeSet<GroupId>, b: &BTreeSet<GroupId>) -> Result<()> {
        let mut deletees = Vec::new();
        for &group in a {
            for (other, _) in self.query_pairs(
                "SELECT smaller_group_id, distance FROM potential_pairs WHERE larger_group_id = ?1
                 UNION SELECT larger_group_id, distance FROM potential_pairs WHERE smaller_group_id = ?1",
                group,
            )? {
                if b.contains(&other) {
                    deletees.push(ordered(group, other));
                }
            }
        }
        self.delete_potentials(&deletees)
    }

    fn are_false_positives(&self, a: AltGroupId, b: AltGroupId) -> Result<bool> {
        let (smaller, larger) = ordered(a, b);
        Ok(self
            .conn
            .prepare_cached(
                "SELECT 1 FROM false_positive_pairs WHERE smaller_alt_group_id = ? AND larger_alt_group_id = ?",
            )?
            .query_row(params![smaller, larger], |_| Ok(()))
            .optional()?
            .is_some())
    }

    fn are_confirmed_alternates(&self, (smaller, larger): (GroupId, GroupId)) -> Result<bool> {
        Ok(self
            .conn
            .prepare_cached(
                "SELECT 1 FROM alt_confirmed_pairs WHERE smaller_group_id = ? AND larger_group_id = ?",
            )?
            .query_row(params![smaller, larger], |_| Ok(()))
            .optional()?
            .is_some())
    }

    // helpers ------------------------------------------------------------

    fn is_stored(&self, hash_id: HashId) -> Result<bool> {
        Ok(self
            .conn
            .prepare_cached(
                "SELECT 1 FROM file_domain_current WHERE service_id = ? AND hash_id = ?",
            )?
            .query_row(params![self.local_storage, hash_id], |_| Ok(()))
            .optional()?
            .is_some())
    }

    fn query_pairs(&self, sql: &str, group: GroupId) -> Result<Vec<(GroupId, u32)>> {
        let mut stmt = self.conn.prepare_cached(sql)?;
        let rows = stmt.query_map([group], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Files whose relationships changed get searched for similar files again.
    fn reset_search(&self, hash_ids: &[HashId]) -> Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE similar_search_status SET searched_distance = NULL WHERE hash_id = ?",
        )?;
        for hash_id in hash_ids {
            stmt.execute([hash_id])?;
        }
        Ok(())
    }
}

fn ordered<T: Ord>(a: T, b: T) -> (T, T) {
    if a <= b { (a, b) } else { (b, a) }
}
