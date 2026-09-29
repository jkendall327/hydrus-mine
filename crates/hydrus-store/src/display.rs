//! Tag display: how siblings and parents turn stored tags into displayed tags.
//!
//! For each tag service the display graph is built from the sibling and
//! parent relations of that service's *applicable* services (see
//! `docs/rust/STORE.md`). Graphs are small and read constantly, so they live
//! in memory as immutable snapshots, and are rebuilt when relations change.
//!
//! Terms:
//! - a *chain* is a set of sibling tags that all display as one *ideal* tag;
//! - *ancestors* of an ideal are every tag reachable through parent links
//!   (after rewriting parent pairs through the sibling ideals);
//! - the *display tags* of a stored tag are its ideal plus the ideal's
//!   ancestors.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use rusqlite::Connection;

use hydrus_core::{ContentStatus, ServiceId, TagId};

use crate::error::Result;
use crate::services::ServiceRegistry;

/// Siblings or parents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationKind {
    Siblings = 0,
    Parents = 1,
}

/// Incrementally resolves sibling pairs into chains, first pair wins.
///
/// Mirrors the reference `TagSiblingsStructure`: a pair is ignored if its bad
/// tag already points somewhere, if it points to itself, or if it would close
/// a cycle.
#[derive(Debug, Default)]
struct SiblingsBuilder {
    bad_to_good: HashMap<TagId, TagId>,
    bad_to_ideal: HashMap<TagId, TagId>,
    ideal_to_bads: HashMap<TagId, HashSet<TagId>>,
}

impl SiblingsBuilder {
    fn add(&mut self, bad: TagId, good: TagId) {
        if bad == good || self.bad_to_good.contains_key(&bad) {
            return;
        }
        let joining = self.bad_to_ideal.get(&good).copied();
        let extending = self.ideal_to_bads.contains_key(&bad);
        if extending && joining == Some(bad) {
            // the chain we'd join ends at our bad tag: a cycle
            return;
        }
        let ideal = joining.unwrap_or(good);
        self.bad_to_good.insert(bad, good);
        self.bad_to_ideal.insert(bad, ideal);
        let mut bads = self.ideal_to_bads.remove(&bad).unwrap_or_default();
        for b in &bads {
            self.bad_to_ideal.insert(*b, ideal);
        }
        bads.insert(bad);
        self.ideal_to_bads.entry(ideal).or_default().extend(bads);
    }
}

/// Incrementally closes parent pairs transitively.
///
/// Mirrors the reference `TagParentsStructure`: self-pairs and pairs that
/// would create a cycle are ignored.
#[derive(Debug, Default)]
struct ParentsBuilder {
    ancestors: HashMap<TagId, HashSet<TagId>>,
    descendants: HashMap<TagId, HashSet<TagId>>,
}

impl ParentsBuilder {
    fn add(&mut self, child: TagId, parent: TagId) {
        if child == parent {
            return;
        }
        if self
            .ancestors
            .get(&parent)
            .is_some_and(|a| a.contains(&child))
        {
            return; // loop
        }
        if self
            .ancestors
            .get(&child)
            .is_some_and(|a| a.contains(&parent))
        {
            return; // already implied
        }
        let mut new_ancestors: HashSet<TagId> =
            self.ancestors.get(&parent).cloned().unwrap_or_default();
        new_ancestors.insert(parent);
        let mut new_descendants: HashSet<TagId> =
            self.descendants.get(&child).cloned().unwrap_or_default();
        new_descendants.insert(child);
        for a in &new_ancestors {
            self.descendants
                .entry(*a)
                .or_default()
                .extend(new_descendants.iter().copied());
        }
        for d in &new_descendants {
            self.ancestors
                .entry(*d)
                .or_default()
                .extend(new_ancestors.iter().copied());
        }
    }
}

/// The resolved display rules of one tag service.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DisplayGraph {
    bad_to_ideal: HashMap<TagId, TagId>,
    /// ideal -> non-ideal chain members, sorted
    ideal_to_bads: HashMap<TagId, Vec<TagId>>,
    /// ideal -> every ideal ancestor, sorted
    ancestors: HashMap<TagId, Vec<TagId>>,
    /// ideal -> every ideal descendant, sorted
    descendants: HashMap<TagId, Vec<TagId>>,
}

impl DisplayGraph {
    /// Build from relation pairs already in priority order.
    pub fn build(
        sibling_pairs: impl IntoIterator<Item = (TagId, TagId)>,
        parent_pairs: impl IntoIterator<Item = (TagId, TagId)>,
    ) -> Self {
        let mut siblings = SiblingsBuilder::default();
        for (bad, good) in sibling_pairs {
            siblings.add(bad, good);
        }
        let ideal = |t: TagId| siblings.bad_to_ideal.get(&t).copied().unwrap_or(t);
        let mut parents = ParentsBuilder::default();
        for (child, parent) in parent_pairs {
            parents.add(ideal(child), ideal(parent));
        }
        let sorted = |set: HashSet<TagId>| {
            let mut v: Vec<TagId> = set.into_iter().collect();
            v.sort_unstable();
            v
        };
        Self {
            ideal_to_bads: siblings
                .ideal_to_bads
                .into_iter()
                .map(|(k, v)| (k, sorted(v)))
                .collect(),
            bad_to_ideal: siblings.bad_to_ideal,
            ancestors: parents
                .ancestors
                .into_iter()
                .map(|(k, v)| (k, sorted(v)))
                .collect(),
            descendants: parents
                .descendants
                .into_iter()
                .map(|(k, v)| (k, sorted(v)))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bad_to_ideal.is_empty() && self.ancestors.is_empty()
    }

    /// The tag `tag` displays as, ignoring parents.
    pub fn ideal(&self, tag: TagId) -> TagId {
        self.bad_to_ideal.get(&tag).copied().unwrap_or(tag)
    }

    /// Every tag in `tag`'s sibling chain, including the ideal.
    pub fn chain(&self, tag: TagId) -> Vec<TagId> {
        let ideal = self.ideal(tag);
        let mut out = vec![ideal];
        if let Some(bads) = self.ideal_to_bads.get(&ideal) {
            out.extend_from_slice(bads);
        }
        out
    }

    /// Ancestors of `tag`'s ideal.
    pub fn ancestors(&self, tag: TagId) -> &[TagId] {
        self.ancestors
            .get(&self.ideal(tag))
            .map_or(&[], Vec::as_slice)
    }

    /// Descendants of `tag`'s ideal.
    pub fn descendants(&self, tag: TagId) -> &[TagId] {
        self.descendants
            .get(&self.ideal(tag))
            .map_or(&[], Vec::as_slice)
    }

    /// What a stored tag displays as: its ideal and the ideal's ancestors.
    pub fn display_tags(&self, stored: TagId) -> impl Iterator<Item = TagId> + '_ {
        let ideal = self.ideal(stored);
        std::iter::once(ideal).chain(self.ancestors(ideal).iter().copied())
    }

    /// Every stored tag that displays as `display` (itself or through a child).
    ///
    /// `display` is first resolved to its ideal, so searching for a bad
    /// sibling finds the same files as searching for the ideal.
    pub fn stored_tags_for(&self, display: TagId) -> Vec<TagId> {
        let ideal = self.ideal(display);
        let mut out = self.chain(ideal);
        for d in self.descendants(ideal) {
            out.extend(self.chain(*d));
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Every tag the graph mentions.
    pub fn all_tags(&self) -> BTreeSet<TagId> {
        let mut out: BTreeSet<TagId> = self.bad_to_ideal.keys().copied().collect();
        out.extend(self.bad_to_ideal.values().copied());
        out.extend(self.ancestors.keys().copied());
        out.extend(self.descendants.keys().copied());
        out
    }
}

/// Display graphs for every tag service.
#[derive(Debug, Default, Clone)]
pub struct DisplayGraphs {
    graphs: HashMap<ServiceId, Arc<DisplayGraph>>,
}

impl DisplayGraphs {
    /// Build every tag service's graph from the database.
    pub fn load(conn: &Connection, services: &ServiceRegistry) -> Result<Self> {
        let application = load_application(conn, services)?;
        let mut graphs = HashMap::new();
        for service in services.tag_services() {
            let graph = build_for(conn, service.id, &application)?;
            graphs.insert(service.id, Arc::new(graph));
        }
        Ok(Self { graphs })
    }

    /// The graph for a tag service (empty if it has no relations).
    pub fn get(&self, service: ServiceId) -> Arc<DisplayGraph> {
        self.graphs.get(&service).cloned().unwrap_or_default()
    }
}

/// Which services' relations apply to each tag service's display, in order.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Application {
    pub siblings: HashMap<ServiceId, Vec<ServiceId>>,
    pub parents: HashMap<ServiceId, Vec<ServiceId>>,
}

impl Application {
    fn for_kind(&self, kind: RelationKind) -> &HashMap<ServiceId, Vec<ServiceId>> {
        match kind {
            RelationKind::Siblings => &self.siblings,
            RelationKind::Parents => &self.parents,
        }
    }

    /// Services whose `kind` relations apply to `display_service`. A service
    /// with no explicit rule uses only its own relations.
    pub fn sources(&self, kind: RelationKind, display_service: ServiceId) -> Vec<ServiceId> {
        self.for_kind(kind)
            .get(&display_service)
            .cloned()
            .unwrap_or_else(|| vec![display_service])
    }
}

pub fn load_application(conn: &Connection, services: &ServiceRegistry) -> Result<Application> {
    let mut app = Application::default();
    let mut stmt = conn.prepare(
        "SELECT display_service_id, kind, source_service_id FROM tag_display_application
         ORDER BY display_service_id, kind, position",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, ServiceId>(0)?,
            r.get::<_, u8>(1)?,
            r.get::<_, ServiceId>(2)?,
        ))
    })?;
    for row in rows {
        let (display, kind, source) = row?;
        // relations of deleted services no longer apply
        if services.get(source).is_err() {
            continue;
        }
        let map = if kind == RelationKind::Siblings as u8 {
            &mut app.siblings
        } else {
            &mut app.parents
        };
        map.entry(display).or_default().push(source);
    }
    Ok(app)
}

/// Relation pairs of one service that are in effect: current minus
/// petitioned, then pending, each in id order.
fn effective_pairs(
    conn: &Connection,
    kind: RelationKind,
    service: ServiceId,
) -> Result<Vec<(TagId, TagId)>> {
    let (table, a, b) = match kind {
        RelationKind::Siblings => ("tag_siblings", "bad_tag_id", "good_tag_id"),
        RelationKind::Parents => ("tag_parents", "child_tag_id", "parent_tag_id"),
    };
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {a}, {b} FROM {table} WHERE service_id = ?1 AND status = ?2 ORDER BY {a}, {b}"
    ))?;
    let mut fetch = |status: ContentStatus| -> Result<Vec<(TagId, TagId)>> {
        Ok(stmt
            .query_map(rusqlite::params![service, status.code()], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?)
    };
    let petitioned: HashSet<(TagId, TagId)> =
        fetch(ContentStatus::Petitioned)?.into_iter().collect();
    let mut out: Vec<(TagId, TagId)> = fetch(ContentStatus::Current)?
        .into_iter()
        .filter(|p| !petitioned.contains(p))
        .collect();
    out.extend(fetch(ContentStatus::Pending)?);
    Ok(out)
}

fn build_for(
    conn: &Connection,
    display_service: ServiceId,
    application: &Application,
) -> Result<DisplayGraph> {
    let mut siblings = Vec::new();
    for source in application.sources(RelationKind::Siblings, display_service) {
        siblings.extend(effective_pairs(conn, RelationKind::Siblings, source)?);
    }
    let mut parents = Vec::new();
    for source in application.sources(RelationKind::Parents, display_service) {
        parents.extend(effective_pairs(conn, RelationKind::Parents, source)?);
    }
    Ok(DisplayGraph::build(siblings, parents))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn t(n: u32) -> TagId {
        TagId(n)
    }

    #[test]
    fn sibling_chains_resolve_to_the_end() {
        // 3 -> 2 -> 1
        let g = DisplayGraph::build([(t(2), t(1)), (t(3), t(2))], []);
        assert_eq!(g.ideal(t(3)), t(1));
        assert_eq!(g.chain(t(3)), [t(1), t(2), t(3)]);
        // chains built in the other order resolve the same
        let g2 = DisplayGraph::build([(t(3), t(2)), (t(2), t(1))], []);
        assert_eq!(g, g2);
    }

    #[test]
    fn first_sibling_pair_wins_and_cycles_are_dropped() {
        let g = DisplayGraph::build([(t(1), t(2)), (t(1), t(3)), (t(2), t(1))], []);
        assert_eq!(g.ideal(t(1)), t(2));
        assert_eq!(g.ideal(t(2)), t(2));
        assert_eq!(g.ideal(t(3)), t(3));
    }

    #[test]
    fn parents_close_transitively_through_ideals() {
        // samus(10) -> character:samus aran(11); 11 -> series:metroid(12) -> studio(13)
        let g = DisplayGraph::build([(t(10), t(11))], [(t(10), t(12)), (t(12), t(13))]);
        assert_eq!(g.ancestors(t(11)), [t(12), t(13)]);
        assert_eq!(g.descendants(t(13)), [t(11), t(12)]);
        assert_eq!(
            g.display_tags(t(10)).collect::<Vec<_>>(),
            [t(11), t(12), t(13)]
        );
        assert_eq!(g.stored_tags_for(t(13)), [t(10), t(11), t(12), t(13)]);
        // searching a bad sibling searches its ideal
        assert_eq!(g.stored_tags_for(t(10)), [t(10), t(11)]);
    }

    #[test]
    fn parent_loops_are_dropped() {
        let g = DisplayGraph::build([], [(t(1), t(2)), (t(2), t(3)), (t(3), t(1))]);
        assert_eq!(g.ancestors(t(1)), [t(2), t(3)]);
        assert!(g.ancestors(t(3)).is_empty());
    }

    proptest! {
        #[test]
        fn graph_invariants(
            siblings in proptest::collection::vec((0u32..20, 0u32..20), 0..30),
            parents in proptest::collection::vec((0u32..20, 0u32..20), 0..30),
        ) {
            let g = DisplayGraph::build(
                siblings.iter().map(|&(a, b)| (t(a), t(b))),
                parents.iter().map(|&(a, b)| (t(a), t(b))),
            );
            for n in 0..20 {
                let tag = t(n);
                let ideal = g.ideal(tag);
                // ideals are fixed points
                prop_assert_eq!(g.ideal(ideal), ideal);
                // a tag is in its own chain
                prop_assert!(g.chain(tag).contains(&tag));
                // no tag is its own ancestor
                prop_assert!(!g.ancestors(tag).contains(&ideal));
                // ancestors and descendants are mirror images
                for a in g.ancestors(tag) {
                    prop_assert!(g.descendants(*a).contains(&ideal));
                }
                // everything a stored tag displays as finds it again
                for d in g.display_tags(tag) {
                    prop_assert!(g.stored_tags_for(d).contains(&tag));
                }
            }
        }
    }
}
