//! Ordered registered call references and specific-over-general launch routing.
use crate::{
    Mime,
    external_calls::{ActualCall, Callable, Manager, Pipeline},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Names are retained for display; stable keys resolve calls independently of renames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallRef {
    pub key: [u8; 32],
    pub name: String,
}
impl From<&Callable> for CallRef {
    fn from(call: &Callable) -> Self {
        Self {
            key: call.key,
            name: call.name.clone(),
        }
    }
}
/// Options' ordered URL calls and MIME-specific file calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routing {
    pub urls: Vec<CallRef>,
    pub files: BTreeMap<Mime, Vec<CallRef>>,
}
impl Default for Routing {
    fn default() -> Self {
        Self {
            urls: Vec::new(),
            files: BTreeMap::from([(Mime::GeneralFile, Vec::new())]),
        }
    }
}
impl Manager {
    /// Registered OS defaults are regenerated in the detached routing draft.
    pub fn ensure_os(&mut self, pipeline: Pipeline) -> CallRef {
        if let Some(call) = self.calls.iter().find(|call| {
            matches!(
                (&call.call, pipeline),
                (ActualCall::DefaultFile, Pipeline::File) | (ActualCall::DefaultUrl, Pipeline::Url)
            )
        }) {
            return call.into();
        }
        let (name, actual) = match pipeline {
            Pipeline::File => ("Default OS File Launch", ActualCall::DefaultFile),
            Pipeline::Url => ("Default OS URL Launch", ActualCall::DefaultUrl),
        };
        let mut call = Callable::new(name);
        call.pipeline = pipeline;
        call.call = actual;
        let mut suffix = 1;
        while self.calls.iter().any(|existing| existing.name == call.name) {
            call.name = format!("{name} ({suffix})");
            suffix += 1;
        }
        let reference = (&call).into();
        self.calls.push(call);
        reference
    }
    /// Keys are authoritative: update names, reject wrong pipelines, deduplicate.
    pub fn wash(&self, pipeline: Pipeline, values: &[CallRef]) -> Vec<CallRef> {
        let mut result: Vec<CallRef> = Vec::new();
        for value in values {
            if let Some(call) = self
                .calls
                .iter()
                .find(|call| call.key == value.key && call.pipeline == pipeline)
                && !result.iter().any(|existing| existing.key == call.key)
            {
                result.push(call.into());
            }
        }
        result
    }
}
impl Routing {
    /// Empty queues receive an OS reference before missing/wrong identities are washed.
    pub fn wash(&mut self, manager: &mut Manager) {
        if self.urls.is_empty() {
            self.urls.push(manager.ensure_os(Pipeline::Url));
        }
        self.urls = manager.wash(Pipeline::Url, &self.urls);
        self.files.entry(Mime::GeneralFile).or_default();
        for values in self.files.values_mut() {
            if values.is_empty() {
                values.push(manager.ensure_os(Pipeline::File));
            }
            *values = manager.wash(Pipeline::File, values);
        }
    }
    /// A present empty specific row suppresses inherited custom calls.
    pub fn file_calls(&self, mime: Mime) -> &[CallRef] {
        if matches!(
            mime,
            Mime::ApplicationHydrusClientCollection | Mime::ApplicationUnknown
        ) {
            return &[];
        }
        self.files
            .get(&mime)
            .or_else(|| {
                mime.general_class().and_then(|general| {
                    self.files
                        .get(&general)
                        .or_else(|| self.files.get(&Mime::GeneralFile))
                })
            })
            .map_or(&[], Vec::as_slice)
    }
}
