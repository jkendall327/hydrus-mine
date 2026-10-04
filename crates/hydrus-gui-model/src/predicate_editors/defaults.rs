//! Immediate custom predicate defaults stored as canonical typed predicates.
//!
//! Panel identity follows the reference's UI comparability, not widget indices,
//! service positions, or the current operator/value. Opening an existing typed
//! predicate takes precedence; reset only changes future editors.
use super::{Context, Editor, Kind, Panel};
use hydrus_core::search::predicate::{Predicate, SystemPredicate, UrlRule, ViewingStat};
use hydrus_core::search::time::TimeTest;

pub use hydrus_store::settings::CustomPredicateDefaults as CustomDefaults;

/// Panel-family behavior layered on the store's canonical predicate records.
pub trait CustomDefaultsExt {
    fn save(&mut self, predicates: Vec<Predicate>);
    fn reset(&mut self, kind: Kind);
    fn uses(&self, kind: Kind) -> bool;
}
/// The actual reference's IsUIEditable relationship for two valued predicates.
/// Ratings deliberately compare across services; viewtime units share a family.
pub fn comparable(a: &Predicate, b: &Predicate) -> bool {
    match (a, b) {
        (Predicate::System(a), Predicate::System(b))
            if a.reference_type() == b.reference_type() =>
        {
            match (a, b) {
                (SystemPredicate::Time { test: a, .. }, SystemPredicate::Time { test: b, .. }) => {
                    matches!(
                        (a, b),
                        (TimeTest::Relative { .. }, TimeTest::Relative { .. })
                            | (TimeTest::Absolute { .. }, TimeTest::Absolute { .. })
                    )
                }
                (
                    SystemPredicate::FileViewingStats { stat: a, .. },
                    SystemPredicate::FileViewingStats { stat: b, .. },
                ) => (*a == ViewingStat::Views) == (*b == ViewingStat::Views),
                (
                    SystemPredicate::KnownUrl { rule: a, .. },
                    SystemPredicate::KnownUrl { rule: b, .. },
                ) => std::mem::discriminant(a) == std::mem::discriminant(b),
                _ => true,
            }
        }
        (Predicate::Namespace { .. }, Predicate::Namespace { .. }) => true,
        _ => false,
    }
}
impl CustomDefaultsExt for CustomDefaults {
    /// Replace each comparable family, preserving unrelated defaults.
    /// An empty panel result, as some rating controls produce, changes nothing.
    fn save(&mut self, predicates: Vec<Predicate>) {
        self.predicates
            .retain(|kept| !predicates.iter().any(|new| comparable(kept, new)));
        self.predicates.extend(predicates);
    }
    /// Forget this panel's family, leaving the current panel fields untouched.
    fn reset(&mut self, kind: Kind) {
        self.predicates.retain(|p| !kind.accepts(p));
    }
    /// Whether the reference star menu offers reset for this panel's family.
    fn uses(&self, kind: Kind) -> bool {
        self.predicates.iter().any(|p| kind.accepts(p))
    }
}
impl Kind {
    /// Whether this valued predicate can initialise this panel in the reference.
    pub fn accepts(self, predicate: &Predicate) -> bool {
        use super::NumericProperty as N;
        use Kind as K;
        use SystemPredicate as S;
        let Predicate::System(p) = predicate else {
            return false;
        };
        match (self, p) {
            (
                K::Width,
                S::Number {
                    property: N::Width, ..
                },
            )
            | (
                K::Height,
                S::Number {
                    property: N::Height,
                    ..
                },
            )
            | (
                K::Duration,
                S::Number {
                    property: N::Duration,
                    ..
                },
            )
            | (
                K::Framerate,
                S::Number {
                    property: N::Framerate,
                    ..
                },
            )
            | (
                K::NumFrames,
                S::Number {
                    property: N::NumFrames,
                    ..
                },
            )
            | (
                K::NumNotes,
                S::Number {
                    property: N::NumNotes,
                    ..
                },
            )
            | (
                K::NumWords,
                S::Number {
                    property: N::NumWords,
                    ..
                },
            )
            | (
                K::NumUrls,
                S::Number {
                    property: N::NumUrls,
                    ..
                },
            ) => true,
            (
                K::TimeDelta(kind),
                S::Time {
                    kind: other,
                    test: TimeTest::Relative { .. },
                },
            )
            | (
                K::TimeDate(kind),
                S::Time {
                    kind: other,
                    test: TimeTest::Absolute { .. },
                },
            ) => kind == *other,
            (
                K::Views,
                S::FileViewingStats {
                    stat: ViewingStat::Views,
                    ..
                },
            )
            | (
                K::Viewtime,
                S::FileViewingStats {
                    stat: ViewingStat::ViewTime | ViewingStat::ViewTimeMilliseconds,
                    ..
                },
            ) => true,
            (
                K::UrlExact,
                S::KnownUrl {
                    rule: UrlRule::ExactMatch(_),
                    ..
                },
            )
            | (
                K::UrlDomain,
                S::KnownUrl {
                    rule: UrlRule::Domain(_),
                    ..
                },
            )
            | (
                K::UrlRegex,
                S::KnownUrl {
                    rule: UrlRule::Regex(_),
                    ..
                },
            )
            | (
                K::UrlClass,
                S::KnownUrl {
                    rule: UrlRule::UrlClass(_),
                    ..
                },
            ) => true,
            (K::Ratio, S::Ratio { .. })
            | (K::NumPixels, S::NumPixels { .. })
            | (K::Size, S::FileSize { .. })
            | (K::DuplicateRelationships, S::FileRelationshipCount { .. })
            | (K::FileService, S::FileService { .. })
            | (K::Limit, S::Limit(_))
            | (K::NoteName, S::NoteName { .. })
            | (K::NumTags, S::NumTags { .. })
            | (K::TagAsNumber, S::TagAsNumber { .. })
            | (K::TagAdvanced, S::TagAdvanced { .. })
            | (K::Filetype, S::Filetype { .. })
            | (K::Hash, S::Hash { .. })
            | (K::RatingAdvanced, S::RatingAdvanced { .. })
            | (K::RatingLike(_) | K::RatingNumerical(_) | K::RatingIncDec(_), S::Rating { .. })
            | (K::SimilarToData, S::SimilarToData { .. })
            | (K::SimilarToFiles, S::SimilarToFiles { .. }) => true,
            _ => false,
        }
    }
}
impl Panel {
    /// Explicit compatible input wins over custom defaults. The reference's
    /// per-service rating constructors do not consult custom defaults.
    pub fn initialise(
        &mut self,
        supplied: Option<&Predicate>,
        defaults: &CustomDefaults,
        context: &Context,
    ) {
        let value = supplied.filter(|p| self.kind.accepts(p)).or_else(|| {
            if matches!(
                self.kind,
                Kind::RatingLike(_) | Kind::RatingNumerical(_) | Kind::RatingIncDec(_)
            ) {
                None
            } else {
                defaults.predicates.iter().find(|p| self.kind.accepts(p))
            }
        });
        if let Some(value) = value {
            self.initialise_predicate(value, context);
        }
    }
}
impl Editor {
    /// Initialise every page before it is displayed, preserving built-in buttons.
    pub fn apply_defaults(&mut self, defaults: &CustomDefaults, context: &Context) {
        for page in &mut self.pages {
            for panel in &mut page.panels {
                panel.initialise(None, defaults, context);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicate_editors::{Blank, RatingService};
    use hydrus_core::search::time::CivilDateTime;
    use hydrus_core::{ServiceKey, ServiceType};
    use serde_json::Value;
    fn decode(value: &Value) -> Predicate {
        let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
        let object =
            hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&value.to_string())
                .unwrap();
        let mut predicate =
            hydrus_legacy::objects::predicates::predicate_with_scales(&object, &|key| {
                recording["services"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| {
                        s["type"].as_u64()
                            == Some(u64::from(ServiceType::LocalRatingNumerical.code()))
                            && s["key"].as_str() == Some(key.to_hex().as_str())
                    })
                    .map(|s| {
                        (
                            s["num_stars"].as_u64().unwrap(),
                            s["allow_zero"].as_bool().unwrap(),
                        )
                    })
            })
            .unwrap();
        // Python like ratings are integer 0/1; native controls use their typed
        // semantic names. Numerical float ratings retain their actual scale.
        if let Predicate::System(SystemPredicate::Rating {
            service: hydrus_core::search::predicate::ServiceRef::Key(key),
            test,
        }) = &mut predicate
            && recording["services"].as_array().unwrap().iter().any(|s| {
                s["key"].as_str() == Some(key.to_hex().as_str())
                    && s["type"].as_u64() == Some(u64::from(ServiceType::LocalRatingLike.code()))
            })
            && let hydrus_core::search::predicate::RatingTest::Count {
                op: hydrus_core::search::number::RatingOp::Equal,
                value: 0 | 1,
            } = *test
        {
            *test = if matches!(
                *test,
                hydrus_core::search::predicate::RatingTest::Count { value: 1, .. }
            ) {
                hydrus_core::search::predicate::RatingTest::Liked
            } else {
                hydrus_core::search::predicate::RatingTest::Disliked
            };
        }
        predicate
    }
    fn values(value: &Value) -> Vec<Predicate> {
        value.as_array().unwrap().iter().map(decode).collect()
    }
    fn context(recording: &Value) -> Context {
        let services = recording["services"].as_array().unwrap();
        let named = |wanted: &[ServiceType]| {
            let mut result = services
                .iter()
                .filter(|s| {
                    wanted.contains(
                        &ServiceType::from_code(s["type"].as_i64().unwrap() as u8).unwrap(),
                    )
                })
                .map(|s| {
                    (
                        ServiceKey::from_hex(s["key"].as_str().unwrap()).unwrap(),
                        s["name"].as_str().unwrap().to_owned(),
                    )
                })
                .collect::<Vec<_>>();
            result.sort_by_key(|(key, name)| {
                let code = services
                    .iter()
                    .find(|s| s["key"].as_str() == Some(key.to_hex().as_str()))
                    .unwrap()["type"]
                    .as_u64()
                    .unwrap() as u8;
                (
                    wanted.iter().position(|t| t.code() == code).unwrap(),
                    name.to_lowercase(),
                )
            });
            result
        };
        let mut ratings = services
            .iter()
            .filter(|s| {
                [
                    ServiceType::LocalRatingLike,
                    ServiceType::LocalRatingNumerical,
                    ServiceType::LocalRatingIncDec,
                ]
                .contains(&ServiceType::from_code(s["type"].as_u64().unwrap() as u8).unwrap())
            })
            .map(|s| RatingService {
                key: ServiceKey::from_hex(s["key"].as_str().unwrap()).unwrap(),
                name: s["name"].as_str().unwrap().to_owned(),
                service_type: ServiceType::from_code(s["type"].as_u64().unwrap() as u8).unwrap(),
                stars: (
                    s["num_stars"].as_u64().unwrap(),
                    s["allow_zero"].as_bool().unwrap(),
                ),
            })
            .collect::<Vec<_>>();
        ratings.sort_by(|a, b| a.name.cmp(&b.name));
        let date = recording["today"]
            .as_str()
            .unwrap()
            .split('-')
            .map(|s| s.parse::<u16>().unwrap())
            .collect::<Vec<_>>();
        Context {
            file_services: named(&[
                ServiceType::LocalFileDomain,
                ServiceType::LocalFileUpdateDomain,
                ServiceType::LocalFileTrashDomain,
                ServiceType::HydrusLocalFileStorage,
                ServiceType::CombinedLocalFileDomains,
                ServiceType::CombinedDeletedFile,
                ServiceType::FileRepository,
            ]),
            tag_services: named(&[
                ServiceType::LocalTag,
                ServiceType::TagRepository,
                ServiceType::CombinedTag,
            ]),
            url_classes: recording["url_classes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect(),
            rating_services: ratings,
            today: CivilDateTime::new(date[0], date[1] as u8, date[2] as u8, 0, 0).unwrap(),
        }
    }
    #[test]
    fn all_recorded_panel_families_restore_typed_defaults_and_explicit_inputs() {
        let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
        let context = context(&recording);
        assert_eq!(recording["cases"].as_array().unwrap().len(), 40);
        for case in recording["cases"].as_array().unwrap() {
            let blank = Blank::from_text(case["blank"].as_str().unwrap()).unwrap();
            let editor = Editor::new(blank, &context);
            let panel = editor
                .pages
                .iter()
                .flat_map(|p| &p.panels)
                .find(|p| p.kind.class_name() == case["class"].as_str().unwrap())
                .unwrap();
            let saved = values(&case["saved"]);
            let mut defaults = CustomDefaults::default();
            defaults.save(saved.clone());
            assert!(defaults.uses(panel.kind));
            let mut fresh = panel.clone();
            fresh.initialise(None, &defaults, &context);
            assert_eq!(
                fresh.predicates(&context).unwrap(),
                values(&case["fresh_after_save"]["serialised"]),
                "{}",
                case["class"]
            );
            let mut explicit = panel.clone();
            explicit.initialise(Some(&decode(&case["explicit_input"])), &defaults, &context);
            assert_eq!(
                explicit.predicates(&context).unwrap(),
                values(&case["explicit"]["serialised"]),
                "{}",
                case["class"]
            );
            let fields = explicit.fields.clone();
            defaults.reset(panel.kind);
            assert_eq!(
                defaults.uses(panel.kind),
                case["uses_after_reset"].as_bool().unwrap()
            );
            assert_eq!(explicit.fields, fields);
            let mut future = panel.clone();
            future.initialise(None, &defaults, &context);
            assert_eq!(future, panel.clone());
            assert_eq!(
                future.predicates(&context).unwrap(),
                values(&case["fresh_after_reset"]["serialised"]),
                "reset {}",
                case["class"]
            );
            let roundtrip: CustomDefaults = serde_json::from_str(
                &serde_json::to_string(&CustomDefaults {
                    predicates: saved.clone(),
                })
                .unwrap(),
            )
            .unwrap();
            assert_eq!(roundtrip.predicates, saved);
        }
    }
    #[test]
    fn comparability_replays_all_reference_subtypes_and_rating_services() {
        let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
        let cases = recording["cases"].as_array().unwrap();
        let decoded = cases
            .iter()
            .map(|c| decode(&c["saved"][0]))
            .collect::<Vec<_>>();
        for (i, a) in cases.iter().enumerate() {
            for (j, b) in cases.iter().enumerate() {
                assert_eq!(
                    comparable(&decoded[i], &decoded[j]),
                    recording["comparability"][i][j].as_bool().unwrap(),
                    "{} vs {}",
                    a["class"],
                    b["class"]
                );
            }
        }
        let mut defaults = CustomDefaults::default();
        for case in cases {
            defaults.save(values(&case["saved"]));
        }
        // Each of the three reference rating defaults replaces the previous
        // service's value, while advanced rating remains an independent type.
        assert_eq!(defaults.predicates.len(), cases.len() - 2);
        let ratings = defaults
            .predicates
            .iter()
            .filter(|p| matches!(p, Predicate::System(SystemPredicate::Rating { .. })))
            .count();
        assert_eq!(ratings, 1);
    }
    #[test]
    fn namespace_shortcuts_and_empty_results_follow_the_actual_star_menu() {
        let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
        let context = context(&recording);
        let edge = &recording["namespace_edge"];
        let mut defaults = CustomDefaults::default();
        let saved = values(&edge["saved"]["serialised"]);
        defaults.save(saved.clone());
        assert_eq!(
            defaults.uses(Kind::NumTags),
            edge["uses_custom"].as_bool().unwrap()
        );
        let mut editor = Editor::new(Blank::NumTags, &context);
        editor.apply_defaults(&defaults, &context);
        assert_eq!(
            editor.pages[0].panels[0].predicates(&context).unwrap(),
            values(&edge["fresh"]["serialised"])
        );
        defaults.reset(Kind::NumTags);
        assert_eq!(defaults.predicates, saved);
        defaults.save(Vec::new());
        assert_eq!(defaults.predicates, saved);
    }
    #[test]
    fn service_keys_and_fractional_viewtime_survive_reordered_contexts_and_reopening() {
        let recording = hydrus_testkit::fixture_json("predicate_custom_defaults.json");
        let mut context = context(&recording);
        let mut defaults = CustomDefaults::default();
        let wanted = [
            "PanelPredicateSystemFileService",
            "PanelPredicateSystemFileViewingStatsViewtime",
            "PredicateSystemRatingAdvanced",
        ];
        for case in recording["cases"].as_array().unwrap() {
            if wanted.contains(&case["class"].as_str().unwrap()) {
                defaults.save(values(&case["saved"]));
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        let kept = defaults.clone();
        store
            .write(move |writer| hydrus_store::settings::set(writer.conn(), &kept))
            .unwrap();
        drop(store);
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        let defaults = store
            .read(hydrus_store::settings::get::<CustomDefaults>)
            .unwrap();
        context.file_services.reverse();
        context.rating_services.reverse();
        for blank in [Blank::FileService, Blank::FileViewingStats, Blank::Rating] {
            let mut editor = Editor::new(blank, &context);
            editor.apply_defaults(&defaults, &context);
            for panel in editor.pages.iter().flat_map(|p| &p.panels) {
                if wanted.contains(&panel.kind.class_name()) {
                    let expected = defaults
                        .predicates
                        .iter()
                        .find(|p| panel.kind.accepts(p))
                        .unwrap();
                    assert_eq!(panel.predicates(&context).unwrap(), vec![expected.clone()]);
                }
            }
        }
        let times = defaults
            .predicates
            .iter()
            .find(|p| {
                matches!(
                    p,
                    Predicate::System(SystemPredicate::FileViewingStats {
                        stat: ViewingStat::ViewTimeMilliseconds,
                        ..
                    })
                )
            })
            .unwrap();
        assert!(matches!(
            times,
            Predicate::System(SystemPredicate::FileViewingStats { value: 600_037, .. })
        ));
    }
}
