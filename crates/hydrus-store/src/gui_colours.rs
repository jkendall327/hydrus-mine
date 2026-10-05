//! Legacy GUI coloursets override thirteen painted roles, never the stylesheet.
use crate::{error::Result, services::Rgb, settings};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub const ROLE_COUNT: usize = 13;
pub const SET_NAMES: [&str; 2] = ["default", "darkmode"];
pub const ROW_LABELS: [&str; 7] = [
    "thumbnail background (local: normal/selected, not local: normal/selected): ",
    "thumbnail border (local: normal/selected, not local: normal/selected): ",
    "thumbnail grid background: ",
    "autocomplete background: ",
    "media viewer background: ",
    "media viewer text: ",
    "tags box background: ",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub override_stylesheet: bool,
    pub current: usize,
    pub sets: [[Rgb; ROLE_COUNT]; 2],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            override_stylesheet: false,
            current: 0,
            sets: [
                [
                    [255, 255, 255],
                    [217, 242, 255],
                    [32, 32, 36],
                    [64, 64, 72],
                    [223, 227, 230],
                    [1, 17, 26],
                    [248, 208, 204],
                    [227, 66, 52],
                    [255, 255, 255],
                    [235, 248, 255],
                    [255, 255, 255],
                    [0, 0, 0],
                    [255, 255, 255],
                ]
                .map(Rgb),
                [
                    [64, 64, 72],
                    [112, 128, 144],
                    [64, 13, 2],
                    [171, 39, 79],
                    [145, 163, 176],
                    [223, 227, 230],
                    [248, 208, 204],
                    [227, 66, 52],
                    [52, 52, 52],
                    [83, 98, 103],
                    [52, 52, 52],
                    [112, 128, 144],
                    [35, 38, 41],
                ]
                .map(Rgb),
            ],
        }
    }
}
impl settings::Setting for Settings {
    const KEY: &'static str = "gui_coloursets";
}
impl Settings {
    pub fn active(&self) -> &[Rgb; ROLE_COUNT] {
        &self.sets[self.current.min(1)]
    }
    pub fn flip(&mut self) {
        self.current = usize::from(self.current == 0);
    }
    /// A concurrently toggled Help colourset and untouched RGB roles survive.
    pub fn save_changed(&self, conn: &Connection, before: &Self) -> Result<()> {
        if self == before {
            return Ok(());
        }
        let mut latest = load(conn)?;
        if self.override_stylesheet != before.override_stylesheet {
            latest.override_stylesheet = self.override_stylesheet;
        }
        if self.current != before.current {
            latest.current = self.current.min(1);
        }
        for set in 0..2 {
            for role in 0..ROLE_COUNT {
                if self.sets[set][role] != before.sets[set][role] {
                    latest.sets[set][role] = self.sets[set][role];
                }
            }
        }
        settings::set(conn, &latest)
    }
}

impl Settings {
    pub fn from_legacy(options: &hydrus_legacy::objects::ClientOptions) -> Self {
        let mut value = Self::default();
        if let Some(enabled) = options.booleans.get("override_stylesheet_colours") {
            value.override_stylesheet = *enabled;
        }
        value.current = usize::from(
            options
                .strings
                .get("current_colourset")
                .is_some_and(|name| name == "darkmode"),
        );
        for (set, name) in SET_NAMES.into_iter().enumerate() {
            if let Some(colours) = options.colours.get(name) {
                for (role, rgb) in value.sets[set].iter_mut().enumerate() {
                    if let Some(saved) = colours.get(&i64::try_from(role).unwrap_or(0)) {
                        *rgb = Rgb(*saved);
                    }
                }
            }
        }
        value
    }
}
/// Native values win; older imported stores retain both original coloursets.
pub fn load(conn: &Connection) -> Result<Settings> {
    use crate::settings::Setting as _;
    use hydrus_legacy::{
        objects::ClientOptions,
        serialisable::{SerialisableObject, SerialisableType},
    };
    let saved: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Settings::KEY],
        |row| row.get(0),
    )?;
    if saved {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Settings::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|error| crate::error::StoreError::Corrupt(error.to_string()))?;
    let options = ClientOptions::from_object(&object)
        .map_err(|error| crate::error::StoreError::Corrupt(error.to_string()))?;
    Ok(Settings::from_legacy(&options))
}
