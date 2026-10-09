//! The pair count of the auto-resolution rule editor's search (the reference
//! embeds the same `EditPotentialDuplicatesSearchContextPanel` there as on the
//! duplicates page): hydrus-gui-model's fragmentary count off the UI thread,
//! shown beside the search's fields with the play/pause and refresh buttons
//! and the cog, whose options are the page's.
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hydrus_core::duplicates::DuplicatesSearch;
use hydrus_gui_model::duplicates_count::{
    BLOCK_GUIDELINE, COG_FILE_SEARCH_OPTIMISATION, COG_STARTS_PAUSED, COG_STOPS_TO_ESTIMATE, Gate,
    Handle, StoreSource,
};
use hydrus_store::Store;
use hydrus_store::duplicates::cache::PairRow;
use hydrus_store::settings::{self, PotentialPairsCountOptions};
use slint::{ComponentHandle as _, ModelRc, Timer, TimerMode, VecModel};

use crate::AutoResolutionRuleWindow;
use crate::duplicates_filtering_sidebar::{model_options, stored_options};

/// A rule editor's count: the worker, the search it counts and the timer
/// that shows how far it has got.
pub(crate) struct RuleCount {
    store: Arc<Store>,
    handle: Rc<Handle<PairRow>>,
    search: Arc<Mutex<DuplicatesSearch>>,
    _timer: Timer,
}

fn show_options(window: &AutoResolutionRuleWindow, options: PotentialPairsCountOptions) {
    window.set_count_cog(ModelRc::new(VecModel::from(vec![
        COG_STARTS_PAUSED.0.into(),
        COG_STOPS_TO_ESTIMATE.0.into(),
        COG_FILE_SEARCH_OPTIMISATION.0.into(),
    ])));
    window.set_count_ticks(ModelRc::new(VecModel::from(vec![
        options.starts_paused,
        options.stops_to_estimate,
        options.file_search_optimisation,
    ])));
}

impl RuleCount {
    /// Start counting `search`, shown in `window`.
    pub(crate) fn start(
        window: &AutoResolutionRuleWindow,
        store: &Arc<Store>,
        search: DuplicatesSearch,
    ) -> Rc<Self> {
        let options = stored_options(store);
        show_options(window, options);
        let search = Arc::new(Mutex::new(search));
        let gate = Gate::here();
        let handle = Rc::new(Handle::start(
            gate.as_ref().map_or(BLOCK_GUIDELINE, |g| g.guideline()),
            options.starts_paused,
            // (a test gate searches the pairs in the order given)
            gate.is_none(),
            model_options(options),
            StoreSource::new(store.clone(), Arc::clone(&search)),
            gate,
        ));
        let timer = Timer::default();
        let weak = window.as_weak();
        let shown = Rc::clone(&handle);
        timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let shot = shown.snapshot();
            if window.get_count() != shot.label.as_str() {
                window.set_count(shot.label.into());
            }
            if window.get_count_paused() != shot.paused {
                window.set_count_paused(shot.paused);
            }
        });
        Rc::new(Self {
            store: store.clone(),
            handle,
            search,
            _timer: timer,
        })
    }

    /// The rule's search was edited: count it afresh when it differs.
    pub(crate) fn search_changed(&self, search: &DuplicatesSearch) {
        let mut counted = self.search.lock().unwrap_or_else(PoisonError::into_inner);
        if *counted == *search {
            return;
        }
        let domain_changed = counted.search_1.location != search.search_1.location;
        counted.clone_from(search);
        drop(counted);
        self.handle.search_changed(domain_changed);
    }

    /// A press of the play/pause or refresh button, or a cog item.
    pub(crate) fn action(&self, window: &AutoResolutionRuleWindow, what: &str, n: i32) {
        match what {
            "pause count" => self.handle.pause_play(),
            "refresh count" => self.handle.refresh(),
            "count option" => {
                let mut options = stored_options(&self.store);
                match n {
                    0 => options.starts_paused = !options.starts_paused,
                    1 => options.stops_to_estimate = !options.stops_to_estimate,
                    2 => options.file_search_optimisation = !options.file_search_optimisation,
                    _ => return,
                }
                if let Err(error) = self
                    .store
                    .write(move |ctx| settings::set(ctx.conn(), &options))
                {
                    eprintln!("could not save the count's options: {error}");
                }
                show_options(window, options);
                self.handle.set_options(model_options(options));
            }
            _ => {}
        }
    }
}
