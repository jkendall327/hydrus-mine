//! Date conversions share timezone rules with the reference string converter.
//! Parsing stays in process: common explicit dates and English relative dates
//! use Jiff, without invoking Python or depending on the host locale.

use jiff::{
    Span, Timestamp, Zoned,
    civil::DateTime,
    fmt::strtime,
    tz::{Offset, TimeZone},
};

use super::strings::DateTimezone;

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn datetime(text: &str, phrase: &str) -> Result<(DateTime, Option<Offset>), String> {
    let mut parsed = strtime::parse(phrase, text).map_err(err)?;
    // Python defaults omitted date fields to 1900-01-01, time to midnight.
    if parsed.year().is_none() {
        parsed.set_year(Some(1900)).map_err(err)?;
    }
    if parsed.day_of_year().is_none()
        && !phrase.contains("%G")
        && !phrase.contains("%U")
        && !phrase.contains("%W")
    {
        if parsed.month().is_none() {
            parsed.set_month(Some(1)).map_err(err)?;
        }
        if parsed.day().is_none() {
            parsed.set_day(Some(1)).map_err(err)?;
        }
    }
    if parsed.hour().is_none() {
        parsed.set_hour(Some(0)).map_err(err)?;
    }
    if parsed.minute().is_none() {
        parsed.set_minute(Some(0)).map_err(err)?;
    }
    if parsed.second().is_none() {
        parsed.set_second(Some(0)).map_err(err)?;
    }
    // strptime accepts a weekday that disagrees with the date.
    parsed.set_weekday(None);
    Ok((parsed.to_datetime().map_err(err)?, parsed.offset()))
}

pub(super) fn decode(
    text: &str,
    phrase: &str,
    timezone: DateTimezone,
    offset: i64,
) -> Result<String, String> {
    let (dt, parsed_offset) = datetime(text, phrase)?;
    let timestamp = match timezone {
        DateTimezone::Local => match parsed_offset {
            Some(offset) => offset.to_timestamp(dt).map_err(err)?,
            None => dt.to_zoned(TimeZone::system()).map_err(err)?.timestamp(),
        },
        DateTimezone::Utc | DateTimezone::Offset => {
            // The reference reconstructs a UTC datetime, discarding fractions
            // and any timezone parsed by strptime before subtracting the offset.
            let dt = dt.with().subsec_nanosecond(0).build().map_err(err)?;
            let timestamp = Offset::UTC.to_timestamp(dt).map_err(err)?;
            if timezone == DateTimezone::Offset {
                timestamp
                    .checked_sub(Span::new().seconds(offset))
                    .map_err(err)?
            } else {
                timestamp
            }
        }
    };
    Ok(timestamp.as_second().to_string())
}

pub(super) fn encode(text: &str, phrase: &str, timezone: DateTimezone) -> Result<String, String> {
    let digits: String = text.trim().chars().filter(|c| *c != '_').collect();
    let seconds: i64 = digits
        .parse()
        .map_err(|_| format!("\"{text}\" was not an integer!"))?;
    let timestamp = Timestamp::from_second(seconds).map_err(err)?;
    // TimestampToDateTime uses the *current* local offset, yielding a naive
    // datetime: Python consequently renders both %z and %Z as empty strings.
    let offset = if timezone == DateTimezone::Local {
        Zoned::now().offset()
    } else {
        Offset::UTC
    };
    let dt = offset.to_datetime(timestamp);
    let phrase = encode_phrase(phrase);
    strtime::format(&phrase, dt).map_err(err)
}

fn encode_phrase(phrase: &str) -> String {
    let mut result = String::new();
    let mut chars = phrase.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('z' | 'Z') => {}
            Some('f') => result.push_str("000000"),
            Some('c') => result.push_str("%a %b %e %H:%M:%S %Y"),
            Some('x') => result.push_str("%m/%d/%y"),
            Some('X') => result.push_str("%H:%M:%S"),
            Some(c) => {
                result.push('%');
                result.push(c);
            }
            None => result.push('%'),
        }
    }
    result
}

pub(super) fn parse(text: &str) -> Result<String, String> {
    parse_at(text, Zoned::now()).map(|t| t.to_string())
}

fn parse_at(text: &str, now: Zoned) -> Result<i64, String> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    let failure = || "Sorry, could not parse that date!".to_owned();
    let relative = match lower.as_str() {
        "now" => Some(now.clone()),
        "today" => Some(now.clone()),
        "yesterday" => Some(now.checked_sub(Span::new().days(1)).map_err(err)?),
        "tomorrow" => Some(now.checked_add(Span::new().days(1)).map_err(err)?),
        _ => None,
    };
    if let Some(relative) = relative {
        return Ok(relative.timestamp().as_second());
    }
    let words: Vec<_> = lower.split_whitespace().collect();
    if let [count, unit, "ago"] = words.as_slice() {
        let count: i64 = count.parse().map_err(|_| failure())?;
        let span = match unit.trim_end_matches('s') {
            "second" => Span::new().seconds(count),
            "minute" => Span::new().minutes(count),
            "hour" => Span::new().hours(count),
            "day" => Span::new().days(count),
            "week" => Span::new().weeks(count),
            "month" => Span::new().months(count),
            "year" => Span::new().years(count),
            _ => return Err(failure()),
        };
        return now
            .checked_sub(span)
            .map(|dt| dt.timestamp().as_second())
            .map_err(err);
    }
    if let Ok(timestamp) = text.parse::<Timestamp>() {
        return Ok(timestamp.as_second());
    }
    // dateparser's English fallback and dateutil cover these common downloader
    // forms. A supplied offset takes precedence over the user's local timezone.
    for phrase in [
        "%Y-%m-%dT%H:%M:%S.%f%z",
        "%Y-%m-%dT%H:%M:%S%z",
        "%Y-%m-%d %H:%M:%S%z",
        "%Y-%m-%d %H:%M:%S %z",
        "%Y-%m-%dT%H:%M:%S.%f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%m/%d/%Y %I:%M:%S%p",
        "%m/%d/%Y %I:%M%p",
        "%m/%d/%Y",
        "%d %B %Y %H:%M:%S",
        "%d %B %Y",
        "%d %b %Y",
        "%B %d, %Y %I:%M %p",
        "%B %d, %Y",
        "%b %d, %Y",
        "%a, %d %b %Y %H:%M:%S %z",
    ] {
        if let Ok((dt, offset)) = datetime(text, phrase) {
            return match offset {
                Some(offset) => offset.to_timestamp(dt).map(|t| t.as_second()).map_err(err),
                None => dt
                    .to_zoned(now.time_zone().clone())
                    .map(|dt| dt.timestamp().as_second())
                    .map_err(err),
            };
        }
    }
    Err(failure())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url::strings::{Conversion, StringConverter};

    #[test]
    fn date_conversions_match_direct_reference_execution() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../oracle/fixtures/string_dates.json"
        ))
        .unwrap();
        let now: Zoned = "2026-10-04T12:30:00+00:00[UTC]".parse().unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let code = case["conversion"][0].as_i64().unwrap();
            let data = &case["conversion"][1];
            let text = case["text"].as_str().unwrap();
            let expected = case["result"].as_str().unwrap();
            let conversion = match code {
                10 => Conversion::DateDecode {
                    phrase: data[0].as_str().unwrap().into(),
                    timezone: DateTimezone::from_code(data[1].as_i64().unwrap()).unwrap(),
                    offset: data[2].as_i64().unwrap(),
                },
                12 => Conversion::DateEncode {
                    phrase: data[0].as_str().unwrap().into(),
                    timezone: DateTimezone::from_code(data[1].as_i64().unwrap()).unwrap(),
                },
                14 => {
                    let actual = parse_at(text, now.clone());
                    if case["error"] == true {
                        assert_eq!(actual.unwrap_err(), "Sorry, could not parse that date!");
                    } else {
                        assert_eq!(actual.unwrap().to_string(), expected, "{text}");
                    }
                    continue;
                }
                _ => panic!("unknown fixture code"),
            };
            // The only naive local fixture uses UTC, so explicitly attach its
            // offset here instead of depending on the CI runner's timezone.
            let conversion = if text == "1969-12-31 23:59:59.900000" && data[1] == 1 {
                Conversion::DateDecode {
                    phrase: "%Y-%m-%d %H:%M:%S.%f%z".into(),
                    timezone: DateTimezone::Local,
                    offset: 0,
                }
            } else {
                conversion
            };
            let text = if text == "1969-12-31 23:59:59.900000" && data[1] == 1 {
                format!("{text}+0000")
            } else {
                text.to_owned()
            };
            let actual = StringConverter {
                conversions: vec![conversion],
                example: text.clone(),
            }
            .convert_upto(&text, None);
            if case["error"] == true {
                let actual = actual.unwrap_err();
                // Jiff's diagnostic reason differs from Python's; the conversion,
                // failing input and the fact it fails must match exactly.
                assert_eq!(actual.split("\": ").next(), expected.split("\": ").next());
            } else {
                assert_eq!(actual.unwrap(), expected, "{case}");
            }
        }
    }

    #[test]
    fn local_encode_uses_current_offset_and_naive_timezone_fields() {
        let current = Zoned::now().offset();
        let expected = strtime::format(
            "%Y-%m-%d %H:%M:%S",
            current.to_datetime(Timestamp::from_second(-1).unwrap()),
        )
        .unwrap();
        assert_eq!(
            encode("-1", "%Y-%m-%d %H:%M:%S%z%Z", DateTimezone::Local).unwrap(),
            expected
        );
    }
}
