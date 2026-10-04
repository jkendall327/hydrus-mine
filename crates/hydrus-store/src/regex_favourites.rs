//! Shared favourite regex phrases, including preserved reference YAML options.
use crate::{
    Result, StoreError, legacy,
    settings::{self, Setting},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A phrase and description pair offered by regex controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegexFavourites(pub Vec<(String, String)>);

impl Default for RegexFavourites {
    fn default() -> Self {
        Self(vec![
            (r"[1-9]+\d*(?=.{4}$)".into(), "…0074.jpg -> 74".into()),
            (
                r"[^/]+(?=\s-)".into(),
                r"E:\my collection\author name - v4c1p0074.jpg -> author name".into(),
            ),
        ])
    }
}

impl Setting for RegexFavourites {
    const KEY: &'static str = "regex_favourites";
}

/// Read saved native favourites, falling back to imported reference preferences.
pub fn load(conn: &Connection) -> Result<RegexFavourites> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?)",
        [RegexFavourites::KEY],
        |row| row.get(0),
    )?;
    if exists {
        return settings::get(conn);
    }
    let old = legacy::old_options(conn)?;
    let Some(old) = old else {
        return Ok(RegexFavourites::default());
    };
    let old =
        hydrus_legacy::objects::LegacyOptions::parse(Some(&old)).map_err(StoreError::Corrupt)?;
    let Some(rows) = old
        .get("regex_favourites")
        .and_then(hydrus_legacy::objects::YamlValue::as_seq)
    else {
        return Ok(RegexFavourites::default());
    };
    let rows = rows
        .iter()
        .map(|row| {
            let values = row
                .as_seq()
                .ok_or_else(|| StoreError::Corrupt("Invalid regex favourite pair.".into()))?;
            match values {
                [phrase, description] => Ok((
                    phrase
                        .as_str()
                        .ok_or_else(|| StoreError::Corrupt("Invalid regex phrase.".into()))?
                        .to_owned(),
                    description
                        .as_str()
                        .ok_or_else(|| StoreError::Corrupt("Invalid regex description.".into()))?
                        .to_owned(),
                )),
                _ => Err(StoreError::Corrupt("Invalid regex favourite pair.".into())),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(RegexFavourites(rows))
}
