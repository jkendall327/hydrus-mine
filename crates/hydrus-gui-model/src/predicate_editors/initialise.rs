//! Reconstruct panel controls from typed predicates, resolving services by key.
use super::{
    Comparison, Context, Field, Kind, NamespaceFilter, NumberOp, NumberTest, Panel, PixelUnit,
    RatioOp, Relationship, RelativeOp, SizeUnit, TagDisplayType, TagNumberOp, ViewCanvas,
    ViewCanvases, ViewingStat,
};
use hydrus_core::ServiceType;
use hydrus_core::search::number::RatingOp;
use hydrus_core::search::predicate::{
    FileHashes, Predicate, RatingLogic, RatingTest, ServiceRef, ServiceSelection, SystemPredicate,
};
use hydrus_core::search::time::TimeTest;

impl Panel {
    fn initial_choice(&mut self, at: usize, selected: usize) {
        if let Some(Field::Choice { chosen, options }) = self.fields.get_mut(at)
            && selected < options.len()
        {
            *chosen = selected;
        }
    }
    fn initial_label(&mut self, at: usize, text: &str) {
        if let Some(Field::Choice { chosen, options }) = self.fields.get_mut(at)
            && let Some(index) = options.iter().position(|o| o == text)
        {
            *chosen = index;
        }
    }
    fn initial_number(&mut self, at: usize, value: u64) {
        if let Some(Field::Number {
            value: v, min, max, ..
        }) = self.fields.get_mut(at)
        {
            *v = i64::try_from(value).unwrap_or(i64::MAX).clamp(*min, *max);
        }
    }
    fn initial_text(&mut self, at: usize, text: &str) {
        if let Some(Field::Text { text: t, .. } | Field::Lines { text: t, .. }) =
            self.fields.get_mut(at)
        {
            text.clone_into(t);
        }
    }
    fn initial_ticks<T>(&mut self, at: usize, values: &[T], mut selected: impl FnMut(&T) -> bool) {
        if let Some(Field::Ticks { ticked, .. }) = self.fields.get_mut(at) {
            for (on, value) in ticked.iter_mut().zip(values) {
                *on = selected(value);
            }
        }
    }
    fn initial_span(&mut self, at: usize, mut value: u64, scales: &[u64]) {
        for (i, scale) in scales.iter().enumerate() {
            self.initial_number(at + i, value / scale);
            value %= scale;
        }
    }
    fn initial_test(&mut self, at: usize, test: NumberTest) {
        self.initial_label(at, number_operator(test.op));
        self.initial_number(at + 1, test.value);
        match test.op {
            NumberOp::ApproxAbsolute { tolerance } => self.initial_number(at + 2, tolerance),
            NumberOp::ApproxPercent { percent } => self.initial_number(at + 3, u64::from(percent)),
            _ => {}
        }
    }
    fn initial_namespace(&mut self, namespace: &NamespaceFilter) {
        let index = match namespace {
            NamespaceFilter::Any => 0,
            NamespaceFilter::Unnamespaced => 1,
            NamespaceFilter::Namespace(_) => 2,
        };
        self.initial_choice(1, index);
        if let NamespaceFilter::Namespace(text) = namespace {
            self.initial_text(2, text);
        }
    }
    fn initial_selection(&mut self, at: usize, value: &ServiceSelection, context: &Context) {
        let local = [
            ServiceType::LocalRatingLike,
            ServiceType::LocalRatingNumerical,
            ServiceType::LocalRatingIncDec,
        ];
        match value {
            ServiceSelection::Types(types) => {
                self.initial_choice(at, 0);
                self.initial_ticks(at + 1, &local, |t| types.contains(t));
            }
            ServiceSelection::Keys(keys) => {
                self.initial_choice(at, 1);
                let services = context
                    .rating_services
                    .iter()
                    .filter(|s| local.contains(&s.service_type))
                    .collect::<Vec<_>>();
                self.initial_ticks(at + 2, &services, |s| keys.contains(&s.key));
            }
            ServiceSelection::Names(names) => {
                self.initial_choice(at, 1);
                let services = context
                    .rating_services
                    .iter()
                    .filter(|s| local.contains(&s.service_type))
                    .collect::<Vec<_>>();
                self.initial_ticks(at + 2, &services, |s| {
                    names.contains(&s.name.to_lowercase())
                });
            }
        }
    }
    /// Called only after the typed family matches the panel.
    pub(super) fn initialise_predicate(&mut self, predicate: &Predicate, context: &Context) {
        let Predicate::System(p) = predicate else {
            return;
        };
        use SystemPredicate as S;
        match p {
            S::Number { test, .. } => {
                if self.kind == Kind::Duration {
                    self.initial_label(1, number_operator(test.op));
                    self.initial_span(2, test.value, &[3_600_000, 60_000, 1000, 1]);
                    match test.op {
                        NumberOp::ApproxAbsolute { tolerance } => {
                            self.initial_span(6, tolerance, &[60_000, 1000, 1]);
                        }
                        NumberOp::ApproxPercent { percent } => {
                            self.initial_number(9, u64::from(percent));
                        }
                        _ => {}
                    }
                } else {
                    self.initial_test(if self.kind == Kind::Framerate { 2 } else { 1 }, *test);
                }
            }
            S::Ratio { op, width, height } => {
                self.initial_label(
                    1,
                    match op {
                        RatioOp::WiderThan => "wider than",
                        RatioOp::TallerThan => "taller than",
                        RatioOp::Approx => "≈",
                        RatioOp::NotEqual => "≠",
                        RatioOp::Equal => "=",
                    },
                );
                self.initial_number(2, *width);
                self.initial_number(4, *height);
            }
            S::NumPixels { op, count, unit } => {
                self.initial_label(1, comparison(*op));
                self.initial_number(2, *count);
                self.initial_choice(
                    3,
                    match unit {
                        PixelUnit::Pixels => 0,
                        PixelUnit::Kilopixels => 1,
                        PixelUnit::Megapixels => 2,
                    },
                );
            }
            S::FileSize { op, size, unit } => {
                self.initial_label(1, comparison(*op));
                self.initial_number(2, *size);
                self.initial_choice(
                    3,
                    match unit {
                        SizeUnit::Bytes => 0,
                        SizeUnit::Kilobytes => 1,
                        SizeUnit::Megabytes => 2,
                        SizeUnit::Gigabytes => 3,
                        SizeUnit::Terabytes => 4,
                    },
                );
            }
            S::FileRelationshipCount {
                op,
                count,
                relationship,
            } => {
                self.initial_label(1, comparison(*op));
                self.initial_number(2, *count);
                self.initial_choice(
                    3,
                    match relationship {
                        Relationship::Duplicates => 0,
                        Relationship::Alternates => 1,
                        Relationship::FalsePositives => 2,
                        Relationship::PotentialDuplicates => 3,
                    },
                );
            }
            S::FileService {
                service,
                status,
                is_in,
            } => {
                self.initial_choice(1, usize::from(!is_in));
                self.initial_choice(
                    2,
                    match status {
                        hydrus_core::ContentStatus::Current => 0,
                        hydrus_core::ContentStatus::Deleted => 1,
                        hydrus_core::ContentStatus::Pending => 2,
                        hydrus_core::ContentStatus::Petitioned => 3,
                    },
                );
                if let Some(index) = service_index(service, &context.file_services) {
                    self.initial_choice(3, index);
                }
            }
            S::FileViewingStats {
                stat,
                canvases,
                op,
                value,
            } => {
                if let ViewCanvases::Specific(canvases) = canvases {
                    self.initial_ticks(
                        1,
                        &[
                            ViewCanvas::MediaViewer,
                            ViewCanvas::Preview,
                            ViewCanvas::ClientApi,
                        ],
                        |c| canvases.contains(c),
                    );
                }
                self.initial_label(2, comparison(*op));
                if *stat == ViewingStat::Views {
                    self.initial_number(3, *value);
                } else {
                    let ms = if *stat == ViewingStat::ViewTime {
                        value.saturating_mul(1000)
                    } else {
                        *value
                    };
                    self.initial_span(3, ms, &[86_400_000, 3_600_000, 60_000, 1000, 1]);
                }
            }
            S::Limit(value) => self.initial_number(1, *value),
            S::NoteName { name, has } => {
                self.initial_choice(1, usize::from(!has));
                self.initial_text(2, name);
            }
            S::NumTags {
                namespace,
                op,
                count,
            } => {
                self.initial_namespace(namespace);
                self.initial_label(3, comparison(*op));
                self.initial_number(4, *count);
            }
            S::TagAsNumber {
                namespace,
                op,
                value,
            } => {
                self.initial_namespace(namespace);
                self.initial_label(
                    3,
                    match op {
                        TagNumberOp::Less => "<",
                        TagNumberOp::Approx => "≈",
                        TagNumberOp::Greater => ">",
                    },
                );
                if let Some(Field::Number {
                    value: v, min, max, ..
                }) = self.fields.get_mut(4)
                {
                    *v = (*value).clamp(*min, *max);
                }
            }
            S::TagAdvanced {
                service,
                display,
                statuses,
                tag,
                inclusive,
            } => {
                self.initial_choice(1, usize::from(!inclusive));
                self.initial_choice(
                    2,
                    service
                        .as_ref()
                        .and_then(|s| service_index(s, &context.tag_services))
                        .map_or(0, |i| i + 1),
                );
                self.initial_choice(3, usize::from(*display == TagDisplayType::Storage));
                self.initial_ticks(
                    4,
                    &[
                        hydrus_core::ContentStatus::Current,
                        hydrus_core::ContentStatus::Pending,
                        hydrus_core::ContentStatus::Deleted,
                        hydrus_core::ContentStatus::Petitioned,
                    ],
                    |s| statuses.contains(s),
                );
                self.initial_text(5, tag.as_str());
            }
            S::Time {
                test: TimeTest::Relative { op, age },
                ..
            } => {
                self.initial_choice(
                    1,
                    match op {
                        RelativeOp::Greater => 0,
                        RelativeOp::Less => 1,
                        RelativeOp::Approx | RelativeOp::NotEqual => 2,
                    },
                );
                for (i, value) in [age.years, age.months, age.days, age.hours]
                    .into_iter()
                    .enumerate()
                {
                    self.initial_number(2 + i, u64::from(value));
                }
            }
            S::Time {
                test: TimeTest::Absolute { op, at },
                ..
            } => {
                self.initial_choice(
                    1,
                    match op {
                        Comparison::Less => 0,
                        Comparison::Greater => 1,
                        Comparison::Approx => 3,
                        _ => 2,
                    },
                );
                self.initial_text(
                    2,
                    &format!("{:04}-{:02}-{:02}", at.year(), at.month(), at.day()),
                );
                self.initial_text(3, &format!("{:02}:{:02}", at.hour(), at.minute()));
            }
            S::KnownUrl { rule, has } => {
                self.initial_choice(1, usize::from(!has));
                match rule {
                    hydrus_core::search::predicate::UrlRule::ExactMatch(s)
                    | hydrus_core::search::predicate::UrlRule::Domain(s)
                    | hydrus_core::search::predicate::UrlRule::Regex(s) => self.initial_text(3, s),
                    hydrus_core::search::predicate::UrlRule::UrlClass(name) => {
                        if let Some(index) = context
                            .url_classes
                            .iter()
                            .position(|s| s.eq_ignore_ascii_case(name))
                        {
                            self.initial_choice(3, index);
                        }
                    }
                }
            }
            S::Filetype {
                filetypes,
                inclusive,
            } => {
                self.initial_choice(1, usize::from(!inclusive));
                let selected = filetypes.specific_mimes();
                if let Some(Field::Tree { groups }) = self.fields.get_mut(2) {
                    for (group, (_, mimes)) in groups.iter_mut().zip(super::special::FILETYPE_TREE)
                    {
                        for (on, mime) in group.ticked.iter_mut().zip(mimes) {
                            *on = selected.contains(mime);
                        }
                    }
                }
            }
            S::Hash { hashes, inclusive } => {
                self.initial_choice(1, usize::from(!inclusive));
                let (kind, lines) = match hashes {
                    FileHashes::Md5(h) => {
                        ("md5", h.iter().map(ToString::to_string).collect::<Vec<_>>())
                    }
                    FileHashes::Sha1(h) => (
                        "sha1",
                        h.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    ),
                    FileHashes::Sha256(h) => (
                        "sha256",
                        h.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    ),
                    FileHashes::Sha512(h) => (
                        "sha512",
                        h.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    ),
                };
                self.initial_label(5, kind);
                self.initial_text(2, &lines.join("\n"));
            }
            S::RatingAdvanced {
                logic,
                services,
                rated,
            } => {
                self.initial_choice(
                    0,
                    match logic {
                        RatingLogic::All => 0,
                        RatingLogic::Any => 1,
                        RatingLogic::Only { .. } => 2,
                    },
                );
                self.initial_selection(1, services, context);
                if let RatingLogic::Only { amongst } = logic {
                    self.initial_selection(5, amongst, context);
                }
                self.initial_choice(8, usize::from(!rated));
            }
            S::Rating { test, .. } => self.initial_rating(*test, context),
            S::SimilarToFiles {
                files,
                max_distance,
            } => {
                self.initial_text(
                    2,
                    &files
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                self.initial_number(4, *max_distance);
            }
            S::SimilarToData {
                pixel_hashes,
                perceptual_hashes,
                max_distance,
            } => {
                self.initial_text(
                    4,
                    &pixel_hashes
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                self.initial_text(
                    5,
                    &perceptual_hashes
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                self.initial_number(7, *max_distance);
            }
            _ => {}
        }
    }
    fn initial_rating(&mut self, test: RatingTest, context: &Context) {
        let choice = match (self.kind, test) {
            (Kind::RatingLike(_), RatingTest::Count { .. }) => 2,
            (
                Kind::RatingIncDec(_),
                RatingTest::Count {
                    op: RatingOp::Greater,
                    value: 0,
                },
            ) => 0,
            (
                Kind::RatingIncDec(_),
                RatingTest::Count {
                    op: RatingOp::Less,
                    value: 1,
                },
            ) => 1,
            (_, test) => match test {
                RatingTest::Rated => 0,
                RatingTest::NotRated => 1,
                RatingTest::Stars { op, .. } | RatingTest::Count { op, .. } => rating_operator(op),
                _ => {
                    if matches!(self.kind, Kind::RatingLike(_)) {
                        2
                    } else {
                        4
                    }
                }
            },
        };
        self.initial_choice(1, choice);
        match (self.kind, test) {
            (Kind::RatingLike(_), RatingTest::Liked) => self.initial_choice(2, 1),
            (Kind::RatingLike(_), RatingTest::Disliked) => self.initial_choice(2, 2),
            (Kind::RatingLike(_), RatingTest::Count { value: 0, .. }) => self.initial_choice(2, 2),
            (Kind::RatingLike(_), RatingTest::Count { value: 1, .. }) => self.initial_choice(2, 1),
            (Kind::RatingNumerical(index), RatingTest::Stars { stars, .. }) => {
                if let Some(service) = context.rating_services.get(index) {
                    self.initial_choice(
                        2,
                        usize::try_from(stars).unwrap_or(0) + usize::from(service.stars.1),
                    );
                }
            }
            (Kind::RatingIncDec(_), RatingTest::Count { value, .. }) => {
                self.initial_number(2, value);
            }
            _ => {}
        }
    }
}
fn number_operator(op: NumberOp) -> &'static str {
    match op {
        NumberOp::Less => "<",
        NumberOp::LessOrEqual => "≤",
        NumberOp::Equal => "=",
        NumberOp::NotEqual => "≠",
        NumberOp::GreaterOrEqual => "≥",
        NumberOp::Greater => ">",
        NumberOp::ApproxAbsolute { .. } => "≈",
        NumberOp::ApproxPercent { .. } => "≈%",
    }
}
fn comparison(op: Comparison) -> &'static str {
    match op {
        Comparison::Less => "<",
        Comparison::Greater => ">",
        Comparison::Approx => "≈",
        Comparison::NotEqual => "≠",
        Comparison::Equal => "=",
    }
}
fn rating_operator(op: RatingOp) -> usize {
    match op {
        RatingOp::Greater | RatingOp::GreaterOrEqual => 2,
        RatingOp::Less | RatingOp::LessOrEqual => 3,
        RatingOp::Approx => 5,
        RatingOp::Equal => 4,
    }
}
fn service_index(
    service: &ServiceRef,
    services: &[(hydrus_core::ServiceKey, String)],
) -> Option<usize> {
    services.iter().position(|(key, name)| match service {
        ServiceRef::Key(s) => s == key,
        ServiceRef::Name(s) => s.eq_ignore_ascii_case(name),
    })
}
