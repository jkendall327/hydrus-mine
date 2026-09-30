//! Counting duplicate-system relationships within the search's file domain.
//!
//! Files are grouped into duplicate groups (same image), which are grouped
//! into alternates groups; alternates groups can be marked false positives
//! of each other, and duplicate groups can be potential pairs. A file's
//! count only includes relations to files in the domain, as the reference
//! does:
//!
//! - duplicates: other members of its group in the domain;
//! - alternates: other duplicate groups in its alternates group that have a
//!   file in the domain;
//! - false positives: alternates groups marked false positive with its own
//!   that have a file in the domain;
//! - potential duplicates: potential pairs of its group whose two groups'
//!   best files are both in the domain.
//!
//! Only files in the domain get a count.

use std::collections::{HashMap, HashSet};

use roaring::RoaringBitmap;

use super::Result;
use super::context::{Domain, Env};
use crate::predicate::Relationship;

struct Groups {
    /// duplicate group -> members
    members: HashMap<u32, Vec<u32>>,
    /// duplicate group -> best file
    kings: HashMap<u32, u32>,
    /// duplicate groups with a member in the domain
    in_domain: RoaringBitmap,
}

fn load_groups(env: &Env<'_>) -> Result<(Groups, RoaringBitmap)> {
    let mut members: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut all_files = RoaringBitmap::new();
    {
        let mut stmt = env
            .conn
            .prepare_cached("SELECT group_id, hash_id FROM dup_group_members")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let (group, hash): (u32, u32) = (row.get(0)?, row.get(1)?);
            members.entry(group).or_default().push(hash);
            all_files.insert(hash);
        }
    }
    let mut kings = HashMap::new();
    {
        let mut stmt = env
            .conn
            .prepare_cached("SELECT group_id, king_hash_id FROM dup_groups")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let (group, king): (u32, u32) = (row.get(0)?, row.get(1)?);
            kings.insert(group, king);
            all_files.insert(king);
        }
    }
    // relations in "all known files" are not limited to a domain
    let files_in_domain = match env.domain {
        Domain::AllKnownFiles => all_files,
        Domain::Tables { .. } => env.restrict_to_domain(all_files)?,
    };
    let in_domain = members
        .iter()
        .filter(|(_, m)| m.iter().any(|h| files_in_domain.contains(*h)))
        .map(|(g, _)| *g)
        .collect();
    Ok((
        Groups {
            members,
            kings,
            in_domain,
        },
        files_in_domain,
    ))
}

/// Every file in the domain with a nonzero count of `relationship`.
pub(crate) fn relationship_counts(
    env: &Env<'_>,
    relationship: Relationship,
) -> Result<HashMap<u32, u64>> {
    let (groups, files) = load_groups(env)?;
    let mut counts: HashMap<u32, u64> = HashMap::new();
    // give every domain file of `group` the count `n`
    let mut assign = |group: u32, n: u64| {
        if n == 0 {
            return;
        }
        for &hash in groups.members.get(&group).into_iter().flatten() {
            if files.contains(hash) {
                *counts.entry(hash).or_default() += n;
            }
        }
    };
    match relationship {
        Relationship::Duplicates => {
            for (&group, members) in &groups.members {
                let n = members.iter().filter(|h| files.contains(**h)).count() as u64;
                assign(group, n.saturating_sub(1));
            }
        }
        Relationship::Alternates => {
            for alt in alternates_groups(env)?.values() {
                let n = alt
                    .iter()
                    .filter(|g| groups.in_domain.contains(**g))
                    .count() as u64;
                for &group in alt {
                    assign(group, n.saturating_sub(1));
                }
            }
        }
        Relationship::FalsePositives => {
            let alternates = alternates_groups(env)?;
            let valid = |alt: u32| {
                alternates
                    .get(&alt)
                    .is_some_and(|g| g.iter().any(|g| groups.in_domain.contains(*g)))
            };
            let mut partners: HashMap<u32, HashSet<u32>> = HashMap::new();
            let mut stmt = env.conn.prepare_cached(
                "SELECT smaller_alt_group_id, larger_alt_group_id FROM false_positive_pairs",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let (a, b): (u32, u32) = (row.get(0)?, row.get(1)?);
                partners.entry(a).or_default().insert(b);
                partners.entry(b).or_default().insert(a);
            }
            for (alt, others) in &partners {
                let n = others.iter().filter(|o| valid(**o)).count() as u64;
                for &group in alternates.get(alt).into_iter().flatten() {
                    assign(group, n);
                }
            }
        }
        Relationship::PotentialDuplicates => {
            let king_in_domain = |group: u32| {
                matches!(env.domain, Domain::AllKnownFiles)
                    || groups.kings.get(&group).is_some_and(|k| files.contains(*k))
            };
            let mut per_group: HashMap<u32, u64> = HashMap::new();
            let mut stmt = env
                .conn
                .prepare_cached("SELECT smaller_group_id, larger_group_id FROM potential_pairs")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let (a, b): (u32, u32) = (row.get(0)?, row.get(1)?);
                if king_in_domain(a) && king_in_domain(b) {
                    *per_group.entry(a).or_default() += 1;
                    *per_group.entry(b).or_default() += 1;
                }
            }
            for (group, n) in per_group {
                assign(group, n);
            }
        }
    }
    Ok(counts)
}

/// alternates group -> its duplicate groups
fn alternates_groups(env: &Env<'_>) -> Result<HashMap<u32, Vec<u32>>> {
    let mut out: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut stmt = env
        .conn
        .prepare_cached("SELECT alt_group_id, group_id FROM alt_group_members")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let (alt, group): (u32, u32) = (row.get(0)?, row.get(1)?);
        out.entry(alt).or_default().push(group);
    }
    Ok(out)
}
