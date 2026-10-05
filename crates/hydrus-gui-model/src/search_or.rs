//! A read-autocomplete OR draft is separate from the active search.
pub mod advanced;
use hydrus_core::sort::human_sort_key;
use hydrus_search::{Predicate, TextContext, predicate_text};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Construction {
    terms: Option<Vec<Predicate>>,
}
impl Construction {
    pub fn terms(&self) -> Option<&[Predicate]> {
        self.terms.as_deref()
    }
    pub fn predicate(&self) -> Option<Predicate> {
        self.terms.clone().map(Predicate::Or)
    }
    /// Shift accumulates unique terms; normal activation commits the draft.
    /// Activating its own one-term result unwraps it, as the reference does.
    pub fn broadcast(
        &mut self,
        mut chosen: Vec<Predicate>,
        shift: bool,
        text: &TextContext,
    ) -> Vec<Predicate> {
        let own = self.predicate();
        let choosing_own = own.as_ref().is_some_and(|own| chosen.contains(own));
        if shift {
            if let Some(own) = own {
                chosen.retain(|predicate| predicate != &own);
            }
            let terms = self.terms.get_or_insert_with(Vec::new);
            for predicate in chosen {
                if !terms.contains(&predicate) {
                    terms.push(predicate);
                }
            }
            terms.sort_by_cached_key(|predicate| human_sort_key(&predicate_text(predicate, text)));
            Vec::new()
        } else {
            let Some(mut terms) = self.terms.take() else {
                return chosen;
            };
            if choosing_own {
                if terms.len() == 1 {
                    chosen.retain(|predicate| Some(predicate) != own.as_ref());
                    chosen.extend(terms);
                }
                chosen
            } else {
                for predicate in chosen {
                    if !terms.contains(&predicate) {
                        terms.push(predicate);
                    }
                }
                terms.sort_by_cached_key(|predicate| {
                    human_sort_key(&predicate_text(predicate, text))
                });
                vec![Predicate::Or(terms)]
            }
        }
    }
    pub fn cancel(&mut self) {
        self.terms = None;
    }
    pub fn rewind(&mut self) {
        if let Some(terms) = &mut self.terms {
            if terms.len() <= 1 {
                self.cancel();
            } else {
                terms.pop();
            }
        }
    }
}
