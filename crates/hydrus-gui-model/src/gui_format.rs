//! Shared presentation over explicit, owned settings; no mutable GUI globals.
use hydrus_store::{
    Store,
    settings::{self, GuiFormatting},
};
use jiff::{
    Timestamp,
    tz::{Offset, TimeZone},
};

pub fn preferences(store: &Store) -> GuiFormatting {
    store.read(settings::get).unwrap_or_default()
}
pub fn bytes(settings: &GuiFormatting, size: u64) -> String {
    hydrus_core::numbers::human_bytes_with_figures(size, settings.figures)
}
pub fn timestamp(settings: &GuiFormatting, value: Option<i64>, now: i64) -> String {
    // Python's local conversion applies the current offset to every date,
    // including target dates in the opposite daylight-saving season.
    timestamp_with_offset(settings, value, now, jiff::Zoned::now().offset())
}

pub fn timestamp_with_offset(
    settings: &GuiFormatting,
    value: Option<i64>,
    now: i64,
    offset: Offset,
) -> String {
    if !settings.iso {
        return value.map_or_else(
            || "at an unknown time".into(),
            |value| hydrus_core::time::timestamp_to_pretty_time_delta(value, now, " ago"),
        );
    }
    let Some(value) = value else {
        return "unknown time".into();
    };
    match Timestamp::from_second(value).map(|t| t.to_zoned(TimeZone::fixed(offset))) {
        Ok(time) if time.year() > 0 => {
            format!("{}-{}", time.year(), time.strftime("%m-%d %H:%M:%S"))
        }
        _ => format!("unparseable time {value}"),
    }
}
