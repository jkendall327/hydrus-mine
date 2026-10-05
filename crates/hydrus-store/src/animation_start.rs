//! Native Animation's initial position; MPV does not consume this preference.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::objects::{LegacyOptions, YamlValue};
use rusqlite::Connection;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct Preferences {
    /// Retain the imported fraction until explicit Options acceptance.
    pub fraction: f64,
}
impl Setting for Preferences {
    const KEY: &'static str = "animation_start";
}
impl Preferences {
    pub fn from_legacy(old: &LegacyOptions) -> Result<Self> {
        let fraction = match old.get("animation_start_position") {
            None => 0.0,
            Some(YamlValue::Float(value)) => *value,
            Some(YamlValue::Int(value)) => *value as f64,
            Some(YamlValue::Bool(value)) => f64::from(u8::from(*value)),
            Some(_) => {
                return Err(StoreError::Corrupt(
                    "invalid animation start position".into(),
                ));
            }
        };
        if !fraction.is_finite() {
            return Err(StoreError::Corrupt(
                "nonfinite animation start position".into(),
            ));
        }
        Ok(Self { fraction })
    }
    /// QSpinBox receives Python int(fraction *100), then clamps0..100.
    pub fn percent(&self) -> u8 {
        (self.fraction * 100.0).trunc().clamp(0.0, 100.0) as u8
    }
    pub fn set_percent(&mut self, percent: i64) {
        self.fraction = percent.clamp(0, 100) as f64 / 100.0;
    }
    /// The reference samples the previous widget count before replacing it.
    /// Impossible negative/overflow indices cannot be admitted by the reader.
    pub fn frame(&self, previous_frames: usize) -> Option<usize> {
        let frame = (previous_frames.saturating_sub(1) as f64 * self.fraction).trunc();
        (frame.is_finite() && frame >= 0.0 && frame < usize::MAX as f64).then_some(frame as usize)
    }
    pub fn save_changed(&self, conn: &Connection, before: &Self) -> Result<()> {
        let latest = load(conn)?;
        let normalized_before = Self {
            fraction: f64::from(before.percent()) / 100.0,
        };
        // The editor always accepts the displayed integer once, including on
        // unchanged Apply. Compare with that displayed original to distinguish
        // implicit normalization from an edit without re-truncating0.29 to0.28.
        if (self != &normalized_before || latest == *before) && self != &latest {
            settings::set(conn, self)?;
        }
        Ok(())
    }
}
pub fn load(conn: &Connection) -> Result<Preferences> {
    let saved: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Preferences::KEY],
        |row| row.get(0),
    )?;
    if saved {
        let preferences: Preferences = settings::get(conn)?;
        if preferences.fraction.is_finite() {
            return Ok(preferences);
        }
        return Err(StoreError::Corrupt(
            "nonfinite animation start position".into(),
        ));
    }
    let old = crate::legacy::old_options(conn)?;
    let old = LegacyOptions::parse(old.as_deref()).map_err(StoreError::Corrupt)?;
    Preferences::from_legacy(&old)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(clippy::float_cmp)]
    fn actual_yaml_upgrade_native_override_reopen_and_invalid_values() {
        let fixture = hydrus_testkit::fixture_json("animation_start.json");
        let yaml = fixture["legacy_yaml"].as_str().unwrap().to_owned();
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        store.write(move |ctx| {ctx.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('options',0,'',0,0,?)",[yaml])?;Ok(())}).unwrap();
        assert_eq!(store.read(load).unwrap().fraction, 0.619);
        store
            .write(|ctx| settings::set(ctx.conn(), &Preferences { fraction: 0.72 }))
            .unwrap();
        assert_eq!(store.read(load).unwrap().percent(), 72);
        assert_eq!(
            crate::Store::open(directory.path())
                .unwrap()
                .read(load)
                .unwrap()
                .percent(),
            72
        );
        store
            .write(|ctx| {
                ctx.conn().execute(
                    "UPDATE settings SET value='{}' WHERE key=?",
                    [Preferences::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store.read(load).unwrap().percent(),
            0,
            "present native defaults still override retained YAML"
        );
        store
            .write(|ctx| {
                ctx.conn().execute(
                    r#"UPDATE settings SET value='{"fraction":"invalid"}' WHERE key=?"#,
                    [Preferences::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(
            store.read(load).is_err(),
            "malformed native policy cannot fall back to an old imported value"
        );
        for yaml in [
            "animation_start_position: .nan",
            "animation_start_position: nonsense",
        ] {
            assert!(Preferences::from_legacy(&LegacyOptions::parse(Some(yaml)).unwrap()).is_err());
        }
        for case in fixture["states"].as_array().unwrap() {
            let settings = Preferences {
                fraction: case["fraction"].as_f64().unwrap(),
            };
            assert_eq!(
                settings.frame(case["previous_count"].as_u64().unwrap() as usize),
                Some(case["index"].as_u64().unwrap() as usize),
                "{case}"
            );
        }
        assert_eq!(Preferences { fraction: -0.5 }.frame(4), None);
        assert_eq!(Preferences { fraction: 1.0 }.frame(0), Some(0));
        assert_eq!(
            Preferences {
                fraction: f64::INFINITY
            }
            .frame(4),
            None
        );
        assert_eq!(Preferences { fraction: f64::MAX }.frame(4), None);
    }
}
