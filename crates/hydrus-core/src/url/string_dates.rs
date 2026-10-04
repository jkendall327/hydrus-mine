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

fn python_parse_phrase(
    text: &str,
    phrase: &str,
    system_zone: &TimeZone,
) -> Result<(String, String), String> {
    let mut pattern = String::from("^");
    let mut translated = String::new();
    let mut directives = phrase.chars();
    let mut special = false;
    while let Some(c) = directives.next() {
        if c != '%' {
            pattern.push_str(&regex::escape(&c.to_string()));
            translated.push(c);
            continue;
        }
        let directive = directives
            .next()
            .ok_or_else(|| "stray % in format".to_owned())?;
        if !"aAwdbBmyYHIpMSfzZjUWcxXGuV%".contains(directive) {
            return Err(format!(
                "'{directive}' is a bad directive in format '{phrase}'"
            ));
        }
        match directive {
            'f' => {
                special = true;
                pattern.push_str("(?P<fraction>[0-9]+)");
                translated.push_str("%f");
            }
            'Z' => {
                special = true;
                pattern.push_str("(?P<zone>[A-Za-z0-9_+\\-]+)");
            }
            '%' => {
                pattern.push('%');
                translated.push_str("%%");
            }
            'Y' | 'G' => {
                pattern.push_str("[0-9]{4}");
                translated.push('%');
                translated.push(directive);
            }
            'y' => {
                pattern.push_str("[0-9]{2}");
                translated.push_str("%y");
            }
            'm' | 'd' | 'H' | 'I' | 'M' | 'S' => {
                pattern.push_str("[0-9]{1,2}");
                translated.push('%');
                translated.push(directive);
            }
            'c' => {
                pattern.push_str("(?s:.*?)");
                translated.push_str("%a %b %e %H:%M:%S %Y");
            }
            'x' => {
                pattern.push_str("(?s:.*?)");
                translated.push_str("%m/%d/%y");
            }
            'X' => {
                pattern.push_str("(?s:.*?)");
                translated.push_str("%H:%M:%S");
            }
            _ => {
                pattern.push_str("(?s:.*?)");
                translated.push('%');
                translated.push(directive);
            }
        }
    }
    if !special {
        return Ok((text.to_owned(), translated));
    }
    pattern.push('$');
    let regex = regex::Regex::new(&pattern).map_err(err)?;
    let captures = regex
        .captures(text)
        .ok_or_else(|| format!("time data '{text}' does not match format '{phrase}'"))?;
    if captures
        .name("fraction")
        .is_some_and(|v| v.as_str().len() > 6)
    {
        return Err("fractional seconds must contain 1 to 6 digits".into());
    }
    let mut text = text.to_owned();
    if let Some(zone) = captures.name("zone") {
        let current = Timestamp::now().to_zoned(system_zone.clone());
        let mut names = vec!["UTC".to_owned(), "GMT".to_owned()];
        for month in [1, 7] {
            if let Ok(local) = jiff::civil::date(current.year(), month, 1)
                .at(0, 0, 0, 0)
                .to_zoned(current.time_zone().clone())
            {
                if let Ok(name) = strtime::format("%Z", &local) {
                    names.push(name);
                }
            }
        }
        if !names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(zone.as_str()))
        {
            return Err(format!(
                "time data '{text}' does not match format '{phrase}'"
            ));
        }
        // Python strptime recognises these names but returns a naive datetime.
        text.replace_range(zone.range(), "");
    }
    Ok((text, translated))
}

fn datetime(
    text: &str,
    phrase: &str,
    system_zone: &TimeZone,
) -> Result<(DateTime, Option<Offset>), String> {
    let (text, translated) = python_parse_phrase(text, phrase, system_zone)?;
    let mut parsed = strtime::parse(&translated, &text).map_err(err)?;
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
    let dt = parsed.to_datetime().map_err(err)?;
    if dt.year() < 1 {
        return Err("year is out of range".into());
    }
    Ok((dt, parsed.offset()))
}

pub(super) fn decode(
    text: &str,
    phrase: &str,
    timezone: DateTimezone,
    offset: i64,
) -> Result<String, String> {
    decode_in_zone(text, phrase, timezone, offset, &TimeZone::system())
}

fn decode_in_zone(
    text: &str,
    phrase: &str,
    timezone: DateTimezone,
    offset: i64,
    system_zone: &TimeZone,
) -> Result<String, String> {
    let (dt, parsed_offset) = datetime(text, phrase, system_zone)?;
    let timestamp = match timezone {
        DateTimezone::Local => match parsed_offset {
            Some(offset) => offset.to_timestamp(dt).map_err(err)?,
            None => dt.to_zoned(system_zone.clone()).map_err(err)?.timestamp(),
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
    let trimmed = text.trim();
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    if !unsigned
        .split('_')
        .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err(format!("\"{text}\" was not an integer!"));
    }
    let digits: String = trimmed.chars().filter(|c| *c != '_').collect();
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
    if dt.year() < 1 {
        return Err("date value out of range".into());
    }
    let phrase = encode_phrase(phrase);
    strtime::BrokenDownTime::from(dt)
        .to_string_with_config(&strtime::Config::new().lenient(true), &phrase)
        .map_err(err)
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
            Some('Q') => result.push_str("%%Q"),
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
        if let Ok((dt, offset)) = datetime(text, phrase, now.time_zone()) {
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
    fn local_names_and_dst_boundaries_match_recorded_reference_platform() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../oracle/fixtures/string_dates.json"
        ))
        .unwrap();
        for case in fixture["local_contexts"].as_array().unwrap() {
            let timezone = TimeZone::get(case["system_timezone"].as_str().unwrap()).unwrap();
            let result = decode_in_zone(
                case["text"].as_str().unwrap(),
                case["phrase"].as_str().unwrap(),
                DateTimezone::Local,
                0,
                &timezone,
            );
            if case["error"] == true {
                assert!(result.is_err(), "{case}");
            } else {
                assert_eq!(result.unwrap(), case["result"].as_str().unwrap(), "{case}");
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
