//! Entering predicates into a search, as the reference's list of a
//! search's predicates takes them (`ListBoxTagsActiveSearchPredicates.
//! _EnterPredicates`): one already there is removed; one that isn't comes
//! in, and those it is mutually exclusive with go
//! ([`Predicate::is_mutually_exclusive`]); then the list sorts itself.

use hydrus_core::ServiceType;
use hydrus_core::sort::human_sort_key;

use crate::predicate::{Predicate, ServiceRef};
use crate::text::{TextContext, predicate_text};

/// Enter `entered` into `search`, as the reference's list does.
pub fn enter_predicates(search: &mut Vec<Predicate>, entered: &[Predicate], text: &TextContext) {
    let is_incdec = |service: &ServiceRef| is_incdec(service, text);
    let plain = plain(text);
    // (an OR's predicates in order, as the reference keeps them, so the
    // same OR in another order is the same)
    for predicate in search.iter_mut() {
        sort_or(predicate, &plain);
    }
    let entered: Vec<Predicate> = entered
        .iter()
        .map(|p| {
            let mut p = p.clone();
            sort_or(&mut p, &plain);
            p
        })
        .collect();
    let mut to_add: Vec<Predicate> = Vec::new();
    let mut to_remove: Vec<Predicate> = Vec::new();
    for predicate in &entered {
        if search.contains(predicate) {
            to_remove.push(predicate.clone());
        } else {
            if !to_add.contains(predicate) {
                to_add.push(predicate.clone());
            }
            to_remove.extend(
                search
                    .iter()
                    .filter(|p| p.is_mutually_exclusive(predicate, &is_incdec))
                    .cloned(),
            );
        }
    }
    search.extend(to_add);
    search.retain(|p| !to_remove.contains(p));
    // (sorted as the reference's list sorts them, `ListBoxItemPredicate.
    // __lt__`: by [`sort_text`], in human order)
    search.sort_by_cached_key(|p| human_sort_key(&sort_text(p, &plain)));
}

/// How the reference writes predicates for copying (and sorting): tags as
/// stored, not as the user has them shown.
fn plain(text: &TextContext) -> TextContext {
    TextContext {
        presentation: None,
        ..text.clone()
    }
}

/// The text the reference's list sorts a predicate by (its first
/// `GetCopyableTexts`), written with `text` (as [`plain`] makes it): a
/// namespace as "namespace:*" and a wildcard as its pattern (both without
/// any "-"), an OR as the first of its predicates as it orders them, and
/// the rest as written.
fn sort_text(predicate: &Predicate, text: &TextContext) -> String {
    match predicate {
        Predicate::Namespace { namespace, .. } => format!("{namespace}:*"),
        Predicate::Wildcard { pattern, .. } => pattern.as_str().to_owned(),
        Predicate::Or(predicates) => predicates
            .iter()
            .map(|p| predicate_text(p, text))
            .min_by_key(|t| human_sort_key(t))
            .unwrap_or_default(),
        _ => predicate_text(predicate, text),
    }
}

/// An OR's predicates in the reference's order: by their text, in human
/// order (as its `Predicate` sorts them when made).
fn sort_or(predicate: &mut Predicate, text: &TextContext) {
    if let Predicate::Or(predicates) = predicate {
        predicates.sort_by_cached_key(|p| human_sort_key(&predicate_text(p, text)));
    }
}

/// Whether a rating predicate's service is an inc/dec one: by key, or by
/// name (exactly, else ignoring case) among the rating services.
pub fn is_incdec(service: &ServiceRef, text: &TextContext) -> bool {
    let ratings = || {
        text.services
            .iter()
            .filter(|s| s.service_type.is_rating_service())
    };
    let found = match service {
        ServiceRef::Key(key) => text.services.iter().find(|s| &s.key == key),
        ServiceRef::Name(name) => ratings()
            .find(|s| &s.name == name)
            .or_else(|| ratings().find(|s| s.name.to_lowercase() == name.to_lowercase())),
    };
    found.is_some_and(|s| s.service_type == ServiceType::LocalRatingIncDec)
}

#[cfg(test)]
mod tests {
    use hydrus_core::ServiceKey;

    use super::*;
    use crate::text::NamedService;

    fn context() -> TextContext {
        let service = |name: &str, service_type| NamedService {
            key: ServiceKey::new(name.as_bytes().to_vec()),
            name: name.to_owned(),
            service_type,
            stars: None,
        };
        TextContext {
            services: vec![
                service("Stars", ServiceType::LocalRatingNumerical),
                service("Counter", ServiceType::LocalRatingIncDec),
            ],
            ..TextContext::default()
        }
    }

    #[test]
    fn ratings_by_key_sort_by_their_service_s_name() {
        use crate::predicate::{RatingTest, SystemPredicate};
        let context = context();
        let rated = |name: &str| {
            Predicate::System(SystemPredicate::Rating {
                service: ServiceRef::Key(ServiceKey::new(name.as_bytes().to_vec())),
                test: RatingTest::Rated,
            })
        };
        let mut search = Vec::new();
        enter_predicates(&mut search, &[rated("Stars")], &context);
        enter_predicates(&mut search, &[rated("Counter")], &context);
        assert_eq!(search, [rated("Counter"), rated("Stars")]);
    }

    #[test]
    fn counters_are_known_by_key_and_by_name_in_any_case() {
        let context = context();
        let by_key = |name: &str| ServiceRef::Key(ServiceKey::new(name.as_bytes().to_vec()));
        assert!(is_incdec(&by_key("Counter"), &context));
        assert!(!is_incdec(&by_key("Stars"), &context));
        // (names typed are lowercased)
        assert!(is_incdec(&ServiceRef::Name("counter".into()), &context));
        assert!(!is_incdec(&ServiceRef::Name("stars".into()), &context));
        assert!(!is_incdec(&ServiceRef::Name("nothing".into()), &context));
    }
}
