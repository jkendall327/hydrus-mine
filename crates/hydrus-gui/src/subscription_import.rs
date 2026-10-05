//! Owned, ordered subscription imports. Files are loaded only when their turn
//! arrives, so a missing-history prompt cannot lose the rest of a selection.
use hydrus_downloader_exchange::{
    Error, MAX_BYTES, subscription_import as codec, subscriptions::Subscription,
};
use std::{collections::VecDeque, fs::File, io::Read as _, path::PathBuf};

#[derive(Debug, Clone)]
pub(crate) enum Event {
    Subscription(Box<Subscription>),
    Notice(String, String),
    File(PathBuf, bool),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Queue {
    events: VecDeque<Event>,
    shown_serialisation_error: bool,
}
impl Queue {
    /// The original menu's format-specific picker, or immediate clipboard text.
    pub(crate) fn from_mode(mode: i32) -> Self {
        let mut queue = Self::default();
        if mode == 3 {
            match crate::from_clipboard().and_then(|text| {
                codec::decode_text_at(&text, now()).map_err(|error| error.to_string())
            }) {
                Ok(package) => queue.package(package),
                Err(error) => queue
                    .events
                    .push_back(Event::Notice("Problem importing!".into(), error)),
            }
        } else {
            let png = mode == 5;
            let title = if png {
                "select the png or pngs with the encoded data"
            } else {
                "select the json or jsons with the serialised data"
            };
            queue.events.extend(
                crate::pick_exchange_files(title, if png { "png" } else { "json" })
                    .into_iter()
                    .map(|path| Event::File(path, png)),
            );
        }
        queue
    }

    fn package(&mut self, package: codec::Package) {
        let warning = package.warning();
        let count = package.subscriptions.len();
        let mut events = VecDeque::new();
        events.extend(
            package
                .subscriptions
                .into_iter()
                .map(|subscription| Event::Subscription(Box::new(subscription))),
        );
        if let Some(warning) = warning {
            events.push_back(Event::Notice("Warning".into(), warning));
        }
        if count > 0 {
            events.push_back(Event::Notice(
                "Information".into(),
                format!(
                    "{} objects added!",
                    hydrus_core::numbers::human_int(count as u64)
                ),
            ));
        }
        events.append(&mut self.events);
        self.events = events;
    }

    /// Read the next package, preserving any accepted prefix on later failures.
    pub(crate) fn next(&mut self) -> Option<Event> {
        while let Some(event) = self.events.pop_front() {
            let Event::File(path, png) = event else {
                return Some(event);
            };
            let bytes = (|| -> Result<Vec<u8>, String> {
                let mut bytes = Vec::new();
                File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take((MAX_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > MAX_BYTES {
                    return Err(Error::Limit.to_string());
                }
                Ok(bytes)
            })();
            let bytes = match bytes {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.events.clear();
                    return Some(Event::Notice(
                        if png {
                            "Problem importing!"
                        } else {
                            "Problem loading!"
                        }
                        .into(),
                        error,
                    ));
                }
            };
            let package = if png {
                codec::decode_png_at(&bytes, now())
            } else {
                std::str::from_utf8(&bytes)
                    .map_err(|e| Error::Invalid(e.to_string()))
                    .and_then(|text| codec::decode_text_at(text, now()))
            };
            match package {
                Ok(package) => self.package(package),
                Err(Error::Unsupported(error)) => {
                    if !self.shown_serialisation_error {
                        self.shown_serialisation_error = true;
                        return Some(Event::Notice("Problem importing!".into(), error));
                    }
                }
                Err(error) => {
                    self.events.clear();
                    return Some(Event::Notice(
                        "Problem importing!".into(),
                        error.to_string(),
                    ));
                }
            }
        }
        None
    }
}
fn now() -> i64 {
    hydrus_core::TimestampMs::now().millis() / 1000
}
