//! A detached service editor: staging, reference delete guards and final questions.
use crate::list_selection::ListSelection;
use hydrus_core::{ServiceId, ServiceKey, ServiceType, casefold::casefold};
use hydrus_store::{
    Store,
    error::Result,
    services::{
        LikeRatingConfig, NumericalRatingConfig, RatingDisplay, Service, ServiceKind,
        StarAppearance,
    },
    services_management,
};

/// Reference manage-service list columns.
pub const COLUMNS: [&str; 3] = ["name", "type", "deletable?"];
/// Confirmation before staging deletion.
pub const DELETE_QUESTION: &str = "Delete the selected services?";
/// Editable local service kinds in the add menu.
pub const ADD_TYPES: [ServiceType; 5] = [
    ServiceType::LocalTag,
    ServiceType::LocalFileDomain,
    ServiceType::LocalRatingLike,
    ServiceType::LocalRatingNumerical,
    ServiceType::LocalRatingIncDec,
];

/// A stable editor row independent of its persisted database id.
#[derive(Debug, Clone)]
pub struct Row {
    pub token: usize,
    pub service: Service,
}
impl Row {
    /// Reference display tuple for this row.
    pub fn cells(&self) -> [String; 3] {
        [
            self.service.name.clone(),
            self.service.service_type().name().into(),
            match &self.service.kind {
                ServiceKind::LocalFiles => "must be empty of files, must have at least one",
                ServiceKind::LocalTags => "must have at least one",
                kind if services_management::editable(kind) => "yes",
                _ => "",
            }
            .into(),
        ]
    }
}
/// Staged service list. Dropping it performs no writes.
#[derive(Debug, Clone)]
pub struct Editor {
    pub rows: Vec<Row>,
    pub selection: ListSelection<usize>,
    /// Column and direction currently shown.
    pub sort_column: usize,
    pub ascending: bool,
    original: Vec<Service>,
    next_token: usize,
}
impl Editor {
    /// Take a detached copy of the current registry.
    pub fn new(store: &Store) -> Result<Self> {
        let original = store.read(|conn| {
            Ok(hydrus_store::services::ServiceRegistry::load(conn)?
                .all()
                .map(|s| (**s).clone())
                .collect::<Vec<_>>())
        })?;
        let rows = original
            .iter()
            .enumerate()
            .map(|(token, service)| Row {
                token,
                service: service.clone(),
            })
            .collect();
        let mut editor = Self {
            rows,
            selection: ListSelection::default(),
            next_token: original.len(),
            original,
            sort_column: 1,
            ascending: true,
        };
        editor.sort(1, true);
        Ok(editor)
    }
    /// Stable tokens in the displayed order.
    pub fn order(&self) -> Vec<usize> {
        self.rows.iter().map(|r| r.token).collect()
    }
    /// Sort the reference list columns while retaining selection.
    pub fn sort(&mut self, column: usize, ascending: bool) {
        self.sort_column = column.min(2);
        self.ascending = ascending;
        self.rows.sort_by(|a, b| {
            let order = if column == 2 {
                services_management::editable(&a.service.kind)
                    .cmp(&services_management::editable(&b.service.kind))
            } else {
                a.cells()[column.min(1)]
                    .cmp(&b.cells()[column.min(1)])
                    .then_with(|| a.service.name.cmp(&b.service.name))
            };
            if ascending { order } else { order.reverse() }
        });
    }
    /// First selected service, as the reference edits the first selected row.
    pub fn selected(&self) -> Option<&Row> {
        self.rows
            .iter()
            .find(|r| self.selection.is_selected(r.token))
    }
    /// Stage a new local service; its key is supplied by the caller's random source.
    pub fn add(
        &mut self,
        key: ServiceKey,
        kind: ServiceKind,
    ) -> std::result::Result<usize, String> {
        if !services_management::editable(&kind) {
            return Err("Only local file, tag and rating services can be added here.".into());
        }
        let token = self.next_token;
        self.next_token += 1;
        self.rows.push(Row {
            token,
            service: Service {
                id: ServiceId(0),
                key,
                name: "new service".into(),
                kind,
            },
        });
        self.selection.select_only(Some(token));
        self.rename(token, "new service")?;
        Ok(token)
    }
    /// Stage a unique casefolded name, using the reference's numbered suffixes.
    pub fn rename(&mut self, token: usize, name: &str) -> std::result::Result<(), String> {
        if name.is_empty() {
            return Err("Please enter a name!".into());
        }
        let unique = crate::favourites::non_dupe_name(name, &|candidate| {
            self.rows
                .iter()
                .any(|r| r.token != token && casefold(&r.service.name) == casefold(candidate))
        });
        let row = self
            .rows
            .iter_mut()
            .find(|r| r.token == token)
            .ok_or("No selected service.")?;
        row.service.name = unique;
        self.sort(self.sort_column, self.ascending);
        Ok(())
    }
    /// Validate selection deletion without changing the staged rows.
    pub fn delete_question(&self, store: &Store) -> std::result::Result<Option<String>, String> {
        let deletable = self
            .rows
            .iter()
            .filter(|r| {
                self.selection.is_selected(r.token)
                    && services_management::editable(&r.service.kind)
            })
            .collect::<Vec<_>>();
        if deletable.is_empty() {
            return Ok(None);
        }
        for service_type in [ServiceType::LocalFileDomain, ServiceType::LocalTag] {
            if self
                .rows
                .iter()
                .filter(|r| r.service.service_type() == service_type)
                .count()
                == deletable
                    .iter()
                    .filter(|r| r.service.service_type() == service_type)
                    .count()
            {
                return Err(format!(
                    "Unfortunately, you must have at least one service of the type \"{}\". You cannot delete them all.",
                    service_type.name()
                ));
            }
        }
        for row in deletable {
            if matches!(row.service.kind, ServiceKind::LocalFiles) && row.service.id != ServiceId(0)
            {
                let id = row.service.id;
                let files: i64 = store
                    .read(|conn| {
                        Ok(conn.query_row(
                            "SELECT count(*) FROM file_domain_current WHERE service_id=?",
                            [id],
                            |r| r.get(0),
                        )?)
                    })
                    .map_err(|e| e.to_string())?;
                if files > 0 {
                    return Err(format!(
                        "The service {} needs to be empty before it can be deleted, but it seems to have {} files in it! Please delete or migrate all the files from it and then try again.",
                        row.service.name,
                        hydrus_core::numbers::human_int(u64::try_from(files).unwrap_or(0))
                    ));
                }
            }
        }
        Ok(Some(DELETE_QUESTION.into()))
    }
    /// Confirm the already checked selection deletion.
    pub fn delete_selected(&mut self) {
        self.rows.retain(|r| {
            !self.selection.is_selected(r.token) || !services_management::editable(&r.service.kind)
        });
        self.selection = ListSelection::default();
    }
    /// Additional reference confirmation when Apply will remove persisted services.
    pub fn apply_question(&self) -> Option<String> {
        let deleted = self
            .original
            .iter()
            .filter(|s| !self.rows.iter().any(|r| r.service.key == s.key))
            .collect::<Vec<_>>();
        if deleted.is_empty() {
            return None;
        }
        let mut text = format!(
            "You are about to delete the following services:\n\n{}\n\nAre you absolutely sure this is correct?",
            deleted
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        if deleted.iter().any(|s| s.kind.has_mappings()) {
            text.push_str("\n\nIf the tag service you are deleting is very large, this operation may take a very very long time. You client will lock up until it is done.");
        }
        Some(text)
    }
    /// Commit the entire staged list, rejecting stale editor state atomically.
    pub fn apply(&self, store: &Store) -> Result<()> {
        services_management::apply(
            store,
            self.original.clone(),
            self.rows.iter().map(|r| r.service.clone()).collect(),
        )
    }
}
/// Reference defaults for an added local service.
pub fn default_kind(service_type: ServiceType) -> Option<ServiceKind> {
    Some(match service_type {
        ServiceType::LocalTag => ServiceKind::LocalTags,
        ServiceType::LocalFileDomain => ServiceKind::LocalFiles,
        ServiceType::LocalRatingLike => ServiceKind::RatingLike(LikeRatingConfig {
            display: RatingDisplay {
                colours: hydrus_store::services::RatingColours {
                    dislike: hydrus_store::services::PenBrush {
                        pen: hydrus_store::services::Rgb([0, 0, 0]),
                        brush: hydrus_store::services::Rgb([200, 80, 120]),
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            appearance: StarAppearance::Shape(hydrus_store::services::StarShape::CIRCLE),
        }),
        ServiceType::LocalRatingNumerical => ServiceKind::RatingNumerical(NumericalRatingConfig {
            display: RatingDisplay::default(),
            appearance: StarAppearance::Shape(hydrus_store::services::StarShape::CIRCLE),
            num_stars: 5,
            allow_zero: true,
            custom_pad: 4,
            show_fraction_beside_stars: 0,
        }),
        ServiceType::LocalRatingIncDec => ServiceKind::RatingIncDec(RatingDisplay::default()),
        _ => return None,
    })
}

/// The reference forces a one-star numerical service to allow zero, avoiding
/// an unusable one-choice rating scale when the user clears the checkbox.
pub fn normalize_numerical(config: &mut NumericalRatingConfig) {
    if config.num_stars == 1 {
        config.allow_zero = true;
    }
}
