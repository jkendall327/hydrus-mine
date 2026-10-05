//! Independent file-history query and chart controls, without Slint state.
use hydrus_core::search::context::{FileSearchContext, LocationContext};
use hydrus_search::{
    Clock, FileSort, Predicate, SystemPredicate, search_files_without_implicit_limit,
};
use hydrus_store::{
    Store, StoreError,
    content::DomainRoles,
    file_history::{self, History, Scope},
};
use std::sync::atomic::AtomicBool;
pub const COMPLEX_DOMAIN: &str =
    "Sorry, this domain is too complicated to calculate for now, try something simpler!";
pub fn load(
    store: &Store,
    search: &FileSearchContext,
    steps: i64,
    cancel: &AtomicBool,
) -> hydrus_store::Result<History> {
    if search.location.current().len() != 1
        || !search.location.deleted().is_empty()
        || search.location.is_all_known_files()
    {
        return Err(StoreError::Invalid(COMPLEX_DOMAIN.into()));
    }
    let snapshot = store.snapshot();
    let key = search
        .location
        .current()
        .first()
        .ok_or_else(|| StoreError::Invalid(COMPLEX_DOMAIN.into()))?;
    let service = snapshot
        .services
        .by_key(key)
        .map_err(|_| StoreError::Invalid(COMPLEX_DOMAIN.into()))?
        .id;
    let roles = DomainRoles::new(&snapshot.services)?;
    store.read(|conn| {
        let mut scope = Scope::entire(service);
        if !search.predicates.is_empty()
            && search.predicates != [Predicate::System(SystemPredicate::Everything)]
        {
            let clock = Clock::system();
            let current = search_files_without_implicit_limit(
                conn,
                &snapshot,
                search,
                FileSort::default(),
                &clock,
            )
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
            let deleted_context = FileSearchContext {
                location: LocationContext::new([], [key.clone()]),
                ..search.clone()
            };
            let deleted = search_files_without_implicit_limit(
                conn,
                &snapshot,
                &deleted_context,
                FileSort::default(),
                &clock,
            )
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
            scope.current = Some(current.into_iter().collect());
            scope.deleted = Some(deleted.into_iter().collect());
        }
        file_history::load(conn, &roles, &scope, steps, cancel)
    })
}
#[derive(Debug, Clone)]
pub struct Chart {
    pub history: History,
    pub visible: [bool; 4],
    pub x: (i64, i64),
    pub y: (i64, i64),
    pub custom_x: bool,
    pub custom_y: bool,
}
impl Default for Chart {
    fn default() -> Self {
        Self {
            history: History::default(),
            visible: [true; 4],
            x: (0, 0),
            y: (0, 1),
            custom_x: false,
            custom_y: false,
        }
    }
}
impl Chart {
    pub fn publish(&mut self, history: History) {
        self.history = history;
        if !self.custom_x {
            self.refit_x();
        }
        if !self.custom_y {
            self.refit_y();
        }
    }
    pub fn toggle(&mut self, index: usize) {
        if let Some(visible) = self.visible.get_mut(index) {
            *visible = !*visible;
            if !self.custom_y {
                self.refit_y();
            }
        }
    }
    pub fn refit_x(&mut self) {
        self.custom_x = false;
        let series = self.history.series();
        let first = series
            .iter()
            .zip(self.visible)
            .filter(|(_, show)| *show)
            .filter_map(|(rows, _)| rows.first().map(|r| r.0))
            .min();
        let last = series
            .iter()
            .zip(self.visible)
            .filter(|(_, show)| *show)
            .filter_map(|(rows, _)| rows.last().map(|r| r.0))
            .max();
        if let (Some(first), Some(last)) = (first, last) {
            self.x = (first, last);
        }
    }
    pub fn refit_y(&mut self) {
        self.custom_y = false;
        self.y = (
            0,
            self.history
                .series()
                .iter()
                .zip(self.visible)
                .filter(|(_, show)| *show)
                .flat_map(|(rows, _)| rows.iter().map(|r| r.1))
                .max()
                .unwrap_or(1)
                .max(1),
        );
    }
    pub fn range_x(&mut self, start: i64, end: i64) -> Result<(), String> {
        if start > end {
            return Err("start date must not be after end date".into());
        }
        self.custom_x = true;
        self.x = (start, end);
        Ok(())
    }
    pub fn range_y(&mut self, min: i64, max: i64) -> Result<(), String> {
        if min > max {
            return Err("minimum count must not exceed maximum count".into());
        }
        self.custom_y = true;
        self.y = (min, max);
        Ok(())
    }
    /// Flat runs are compressed for drawing while the exact sampled data remains
    /// available for axes and reference replay. Coordinates use a 600x320 box.
    pub fn paths(&self) -> [String; 4] {
        let series = self.history.series();
        std::array::from_fn(|i| {
            if !self.visible[i] {
                return String::new();
            }
            let mut compact: Vec<(i64, i64)> = Vec::new();
            for &point in series[i] {
                let n = compact.len();
                if n >= 2 && compact[n - 1].1 == point.1 && compact[n - 2].1 == point.1 {
                    compact[n - 1] = point;
                } else {
                    compact.push(point);
                }
            }
            let mut out = String::new();
            for (n, (x, y)) in compact.into_iter().enumerate() {
                let x = 600.0 * (x as f64 - self.x.0 as f64)
                    / (self.x.1 as f64 - self.x.0 as f64).max(1.0);
                let y = 320.0
                    - 320.0 * (y as f64 - self.y.0 as f64)
                        / (self.y.1 as f64 - self.y.0 as f64).max(1.0);
                out.push_str(&format!(
                    "{} {x:.3} {y:.3} ",
                    if n == 0 { "M" } else { "L" }
                ));
            }
            out
        })
    }
}
pub fn date(seconds: i64) -> String {
    jiff::Timestamp::from_second(seconds)
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}
pub fn parse_date(text: &str) -> Result<i64, String> {
    let date = text
        .parse::<jiff::civil::Date>()
        .map_err(|e| e.to_string())?;
    date.at(0, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::system())
        .map(|t| t.timestamp().as_second())
        .map_err(|e| e.to_string())
}
