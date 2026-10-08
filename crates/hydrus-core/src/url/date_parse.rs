//! The "datestring to timestamp (easy)" conversion's English grammar. By the
//! owner's decision (2026-10-08) it matches a reference install without the
//! `dateparser` library, where `ClientTime.ParseDate` is `dateutil.parser.parse`,
//! plus the English relative expressions `dateparser` reads. Proved against a
//! corpus recorded from the reference (`oracle/record_dateparser_corpus.py`).
//!
//! The dateutil part: ISO 8601 dates and times with `T`, fractions, `Z` and
//! offsets; RFC 2822 and HTTP dates; numeric dates (month first, day first when
//! the month would be invalid, two-digit years within fifty years of now);
//! month names, ordinals and weekdays; 12- and 24-hour times; UTC, GMT and Z
//! (other zone names are ignored, `UTC+9` reads as POSIX does, west of UTC).
//! Missing parts come from `now`, as dateutil's default does. No Unix
//! timestamps, "noon", or other languages.
//!
//! The relative part (dateparser's): "now", "yesterday", "2 hours ago", "in 3
//! weeks", "a day ago", "last week", "yesterday at 5pm", with the units
//! abbreviated or in words.

use jiff::{
    Span, Timestamp, Zoned,
    civil::{Date, DateTime, Time},
    tz::Offset,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Num(String),
    Word(String),
    Sym(char),
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Token::Num(chars[start..i].iter().collect()));
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphabetic() {
                i += 1;
            }
            out.push(Token::Word(
                chars[start..i].iter().collect::<String>().to_lowercase(),
            ));
        } else {
            let fraction_comma = c == ','
                && i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if fraction_comma {
                out.push(Token::Sym('.'));
            } else if !c.is_whitespace() && c != ',' {
                out.push(Token::Sym(c));
            }
            i += 1;
        }
    }
    out
}

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

fn month_of(word: &str) -> Option<i8> {
    if word == "sept" {
        return Some(9);
    }
    if word.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|m| *m == word || (word.len() == 3 && m.starts_with(word)))
        .and_then(|i| i8::try_from(i + 1).ok())
}

fn weekday_of(word: &str) -> Option<usize> {
    if word.len() < 3 {
        return None;
    }
    WEEKDAYS
        .iter()
        .position(|d| *d == word || (word.len() == 3 && d.starts_with(word)))
}

/// Seconds east of UTC for a zone name. Like `dateutil`, only UTC, GMT and
/// "Z" are known; other names (EST, CEST) are ignored.
fn zone_offset(word: &str) -> Option<i32> {
    matches!(word, "z" | "utc" | "gmt" | "ut").then_some(0)
}

fn number_word(word: &str) -> Option<i64> {
    Some(match word {
        "a" | "an" | "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum Unit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

fn unit_of(word: &str) -> Option<Unit> {
    Some(match word {
        "s" | "sec" | "secs" | "second" | "seconds" => Unit::Second,
        "m" | "min" | "mins" | "minute" | "minutes" => Unit::Minute,
        "h" | "hr" | "hrs" | "hour" | "hours" => Unit::Hour,
        "d" | "day" | "days" => Unit::Day,
        "week" | "weeks" => Unit::Week,
        "mo" | "month" | "months" => Unit::Month,
        "y" | "yr" | "year" | "years" => Unit::Year,
        _ => return None,
    })
}

fn span(count: i64, unit: Unit) -> Span {
    match unit {
        Unit::Second => Span::new().seconds(count),
        Unit::Minute => Span::new().minutes(count),
        Unit::Hour => Span::new().hours(count),
        Unit::Day => Span::new().days(count),
        Unit::Week => Span::new().weeks(count),
        Unit::Month => Span::new().months(count),
        Unit::Year => Span::new().years(count),
    }
}

/// A clock time and its zone, as far as the text gave them.
#[derive(Default)]
struct Clock {
    time: Option<Time>,
    /// Seconds east of UTC, if the text named a zone.
    offset: Option<i32>,
}

fn num(token: Option<&Token>) -> Option<i64> {
    match token {
        Some(Token::Num(n)) if n.len() <= 18 => n.parse().ok(),
        _ => None,
    }
}

fn word(token: Option<&Token>) -> Option<&str> {
    match token {
        Some(Token::Word(w)) => Some(w),
        _ => None,
    }
}

/// Take the clock time and zone out of `tokens`.
fn take_clock(tokens: &mut Vec<Token>, day_words: bool) -> Result<Clock, ()> {
    let mut clock = Clock::default();
    // where the time was, which is where an offset after it begins
    let mut time_at = None;
    let mut i = 0;
    while i < tokens.len() {
        // h:mm[:ss[.fraction]] [am|pm]
        if let (Some(h), Some(Token::Sym(':')), Some(m)) = (
            num(tokens.get(i)),
            tokens.get(i + 1),
            num(tokens.get(i + 2)),
        ) && clock.time.is_none()
        {
            let mut end = i + 3;
            let mut s = 0;
            if let (Some(Token::Sym(':')), Some(sec)) = (tokens.get(end), num(tokens.get(end + 1)))
            {
                s = sec;
                end += 2;
                if let (Some(Token::Sym('.')), Some(Token::Num(_))) =
                    (tokens.get(end), tokens.get(end + 1))
                {
                    end += 2;
                }
            }
            let mut hour = h;
            if let Some(meridian @ ("am" | "pm")) = word(tokens.get(end)) {
                if !(1..=12).contains(&hour) {
                    return Err(());
                }
                if meridian == "pm" && hour != 12 {
                    hour += 12;
                } else if meridian == "am" && hour == 12 {
                    hour = 0;
                }
                end += 1;
            }
            let time = Time::new(
                i8::try_from(hour).map_err(|_| ())?,
                i8::try_from(m).map_err(|_| ())?,
                i8::try_from(s).map_err(|_| ())?,
                0,
            )
            .map_err(|_| ())?;
            clock.time = Some(time);
            tokens.drain(i..end);
            time_at = Some(i);
            continue;
        }
        // 5pm
        if let (Some(h), Some(meridian @ ("am" | "pm"))) =
            (num(tokens.get(i)), word(tokens.get(i + 1)))
            && clock.time.is_none()
            && (1..=12).contains(&h)
        {
            let hour = match (meridian, h) {
                ("pm", 12) | ("am", 1..=11) => h,
                ("pm", _) => h + 12,
                _ => 0,
            };
            clock.time =
                Some(Time::new(i8::try_from(hour).map_err(|_| ())?, 0, 0, 0).map_err(|_| ())?);
            tokens.drain(i..i + 2);
            time_at = Some(i);
            continue;
        }
        // 17h
        if let (Some(h), Some("h")) = (num(tokens.get(i)), word(tokens.get(i + 1)))
            && clock.time.is_none()
            && h < 24
        {
            clock.time =
                Some(Time::new(i8::try_from(h).map_err(|_| ())?, 0, 0, 0).map_err(|_| ())?);
            tokens.drain(i..i + 2);
            time_at = Some(i);
            continue;
        }
        i += 1;
    }
    for (name, hour) in [("noon", 12), ("midnight", 0)] {
        if !day_words {
            break;
        }
        if let Some(at) = tokens.iter().position(|t| *t == Token::Word(name.into())) {
            clock.time = Some(Time::new(hour, 0, 0, 0).map_err(|_| ())?);
            tokens.remove(at);
        }
    }
    // a zone: a name, with an optional offset (UTC+9), or an offset after a time
    let mut i = 0;
    while i < tokens.len() {
        if let Some(name) = word(tokens.get(i))
            && let Some(base) = zone_offset(name)
            && clock.offset.is_none()
            && (clock.time.is_some() || name != "z")
            && (i > 0 || tokens.len() == 1)
        {
            let mut offset = base;
            let mut end = i + 1;
            if matches!(name, "utc" | "gmt")
                && let Some(Token::Sym(sign @ ('+' | '-'))) = tokens.get(end)
                && let Some(n) = num(tokens.get(end + 1))
            {
                let (hours, minutes, used) = offset_digits(tokens, end + 1, n)?;
                let seconds = hours * 3600 + minutes * 60;
                // (as dateutil reads a POSIX-style "UTC+9": west of UTC)
                offset = if *sign == '-' { seconds } else { -seconds };
                end += 1 + used;
            }
            clock.offset = Some(offset);
            tokens.drain(i..end);
            continue;
        }
        if time_at == Some(i)
            && clock.offset.is_none()
            && let Some(Token::Sym(sign @ ('+' | '-'))) = tokens.get(i)
            && let Some(n) = num(tokens.get(i + 1))
            && matches!(tokens.get(i + 1), Some(Token::Num(d)) if d.len() == 2 || d.len() == 4)
        {
            let (hours, minutes, used) = offset_digits(tokens, i + 1, n)?;
            let seconds = hours * 3600 + minutes * 60;
            clock.offset = Some(if *sign == '-' { -seconds } else { seconds });
            tokens.drain(i..i + 1 + used);
            continue;
        }
        i += 1;
    }
    Ok(clock)
}

/// An offset's hours and minutes from the digits at `at` (`+09`, `+0900`,
/// `+09:00`, or just `+9`): (hours, minutes, tokens used).
fn offset_digits(tokens: &[Token], at: usize, n: i64) -> Result<(i32, i32, usize), ()> {
    let Some(Token::Num(digits)) = tokens.get(at) else {
        return Err(());
    };
    let (hours, mut minutes, mut used) = if digits.len() == 4 {
        (n / 100, n % 100, 1)
    } else {
        (n, 0, 1)
    };
    if digits.len() <= 2
        && let (Some(Token::Sym(':')), Some(m)) = (tokens.get(at + 1), num(tokens.get(at + 2)))
    {
        minutes = m;
        used += 2;
    }
    if hours > 14 || minutes > 59 {
        return Err(());
    }
    Ok((
        i32::try_from(hours).map_err(|_| ())?,
        i32::try_from(minutes).map_err(|_| ())?,
        used,
    ))
}

/// A year from its digits; two digits are the year within fifty years of
/// `now`'s (dateutil's rule).
fn year_of(n: i64, digits: usize, now_year: i16) -> Option<i16> {
    let year = if digits <= 2 {
        let now_year = i64::from(now_year);
        let mut year = now_year / 100 * 100 + n;
        if year >= now_year + 50 {
            year -= 100;
        } else if year < now_year - 50 {
            year += 100;
        }
        year
    } else {
        n
    };
    (digits == 2 || digits == 1 || digits == 4)
        .then_some(year)
        .and_then(|y| i16::try_from(y).ok())
}

/// The date the remaining `tokens` name (`now`'s date fills what is missing).
fn take_date(tokens: &[Token], now: &DateTime) -> Result<Option<Date>, ()> {
    let tokens: Vec<&Token> = tokens
        .iter()
        .filter(|t| {
            !matches!(t, Token::Word(w) if matches!(w.as_str(), "of" | "at" | "on" | "and" | "ad")
                || weekday_of(w).is_some()
                || matches!(w.as_str(), "st" | "nd" | "rd" | "th"))
        })
        .collect();
    if tokens.is_empty() {
        return Ok(None);
    }
    let digits = |t: &Token| match t {
        Token::Num(n) => Some(n.len()),
        _ => None,
    };
    let value = |t: &Token| match t {
        Token::Num(n) => n.parse::<i64>().ok(),
        _ => None,
    };
    let make = |year: i64, month: i64, day: i64| -> Result<Date, ()> {
        Date::new(
            i16::try_from(year).map_err(|_| ())?,
            i8::try_from(month).map_err(|_| ())?,
            i8::try_from(day).map_err(|_| ())?,
        )
        .map_err(|_| ())
    };
    // a month name: with a day and/or a year around it
    if let Some(at) = tokens
        .iter()
        .position(|t| matches!(t, Token::Word(w) if month_of(w).is_some()))
    {
        let Token::Word(name) = tokens[at] else {
            return Err(());
        };
        let month = i64::from(month_of(name).ok_or(())?);
        let mut day = None;
        let mut year = None;
        for t in tokens
            .iter()
            .enumerate()
            .filter_map(|(i, t)| (i != at).then_some(*t))
        {
            let (Some(n), Some(d)) = (value(t), digits(t)) else {
                if matches!(t, Token::Sym('.' | '-' | '/')) {
                    continue;
                }
                return Err(());
            };
            if d == 4 {
                if year.replace(n).is_some() {
                    return Err(());
                }
            } else if day.is_none() && (1..=31).contains(&n) {
                day = Some(n);
            } else if year.is_none() && d == 2 {
                year = Some(i64::from(year_of(n, 2, now.year()).ok_or(())?));
            } else {
                return Err(());
            }
        }
        let year = year.unwrap_or_else(|| i64::from(now.year()));
        let day = day.unwrap_or_else(|| i64::from(now.day()));
        // (a day past the month's end gives way to its last day)
        let first = make(year, month, 1)?;
        let day = day.min(i64::from(first.days_in_month()));
        return make(year, month, day).map(Some);
    }
    // numbers with separators, or a run of digits
    let numbers: Vec<i64> = tokens.iter().filter_map(|t| value(t)).collect();
    let widths: Vec<usize> = tokens.iter().filter_map(|t| digits(t)).collect();
    let separators = tokens.iter().filter(|t| matches!(t, Token::Sym(_))).count();
    if tokens.iter().any(|t| matches!(t, Token::Word(_))) {
        return Err(());
    }
    if let (&[run], 0) = (widths.as_slice(), separators) {
        return match run {
            8 => {
                let n = numbers[0];
                make(n / 10_000, n / 100 % 100, n % 100).map(Some)
            }
            4 => make(numbers[0], i64::from(now.month()), i64::from(now.day())).map(Some),
            _ => Err(()),
        };
    }
    if separators != widths.len().saturating_sub(1)
        || !tokens.iter().all(|t| match t {
            Token::Sym(c) => matches!(c, '-' | '/' | '.'),
            _ => true,
        })
    {
        return Err(());
    }
    match (numbers.as_slice(), widths.as_slice()) {
        // year first: 2024-02-29, 2024/02/29, 2024-02
        ([y, m, d], [4, 1..=2, 1..=2]) => make(*y, *m, *d).map(Some),
        ([y, m], [4, 1..=2]) => {
            let first = make(*y, *m, 1)?;
            make(
                *y,
                *m,
                i64::from(now.day()).min(i64::from(first.days_in_month())),
            )
            .map(Some)
        }
        // month first, or day first when that is the only valid reading
        ([a, b, c], [1..=2, 1..=2, w @ (1 | 2 | 4)]) => {
            let year = i64::from(year_of(*c, *w, now.year()).ok_or(())?);
            make(year, *a, *b)
                .or_else(|()| make(year, *b, *a))
                .map(Some)
        }
        _ => Err(()),
    }
}

/// A relative expression ("2 hours ago", "in a day", "last week"), as the
/// civil time it names from `now`; `None` if the text isn't one.
fn relative(tokens: &[Token], now: &DateTime) -> Result<Option<DateTime>, ()> {
    let words: Vec<&Token> = tokens
        .iter()
        .filter(|t| !matches!(t, Token::Word(w) if w == "and"))
        .collect();
    let ago = matches!(words.last(), Some(Token::Word(w)) if w == "ago");
    let future = matches!(words.first(), Some(Token::Word(w)) if w == "in");
    // last/next <unit>
    if let [Token::Word(which), Token::Word(unit)] = words.as_slice()
        && matches!(which.as_str(), "last" | "next")
        && let Some(unit) =
            unit_of(unit).filter(|u| !matches!(u, Unit::Second | Unit::Minute | Unit::Hour))
    {
        let sign = if which == "last" { -1 } else { 1 };
        return now.checked_add(span(sign, unit)).map(Some).map_err(|_| ());
    }
    if !ago && !future {
        return Ok(None);
    }
    let body = &words[usize::from(future)..words.len() - usize::from(ago)];
    if body.is_empty() {
        return Err(());
    }
    let mut total = [0_i64; 7];
    let mut i = 0;
    while i < body.len() {
        // "2d" is a number and a word; "two", "a" and "an" are words
        let (count, step) = match (body[i], body.get(i + 1)) {
            (Token::Num(n), _) => (n.parse::<i64>().map_err(|_| ())?, 1),
            (Token::Word(w), Some(Token::Word(_))) => (number_word(w).ok_or(())?, 1),
            _ => return Err(()),
        };
        let Some(Token::Word(unit)) = body.get(i + step) else {
            return Err(());
        };
        let slot = match unit_of(unit).ok_or(())? {
            Unit::Year => 0,
            Unit::Month => 1,
            Unit::Week => 2,
            Unit::Day => 3,
            Unit::Hour => 4,
            Unit::Minute => 5,
            Unit::Second => 6,
        };
        total[slot] = total[slot].checked_add(count).ok_or(())?;
        i += step + 1;
    }
    let total = Span::new()
        .try_years(total[0])
        .and_then(|s| s.try_months(total[1]))
        .and_then(|s| s.try_weeks(total[2]))
        .and_then(|s| s.try_days(total[3]))
        .and_then(|s| s.try_hours(total[4]))
        .and_then(|s| s.try_minutes(total[5]))
        .and_then(|s| s.try_seconds(total[6]))
        .map_err(|_| ())?;
    let moved = if ago {
        now.checked_sub(total)
    } else {
        now.checked_add(total)
    };
    moved.map(Some).map_err(|_| ())
}

/// The Unix timestamp the English text `text` names, at the clock `now`.
pub(super) fn parse_at(text: &str, now: &Zoned) -> Result<i64, String> {
    let failure = || "Sorry, could not parse that date!".to_owned();
    inner(text.trim(), now).map_err(|()| failure())
}

/// `text` without a trailing upper-case time zone name that dateutil doesn't
/// know (EST, CEST, JST), which it ignores once there is a time.
fn strip_unknown_zone(text: &str) -> String {
    let trimmed = text.trim_end();
    if let Some((rest, last)) = trimmed.rsplit_once(char::is_whitespace)
        && (2..=5).contains(&last.len())
        && last.chars().all(|c| c.is_ascii_uppercase())
        && !matches!(last, "AM" | "PM" | "UTC" | "GMT" | "UT" | "AD")
        && (rest.contains(':')
            || rest.to_ascii_lowercase().contains("am")
            || rest.to_ascii_lowercase().contains("pm"))
    {
        return rest.to_owned();
    }
    trimmed.to_owned()
}

fn inner(text: &str, now: &Zoned) -> Result<i64, ()> {
    let civil_now = now.datetime();
    let resolve = |dt: DateTime, offset: Option<i32>| -> Result<i64, ()> {
        match offset {
            Some(seconds) => Offset::from_seconds(seconds)
                .map_err(|_| ())?
                .to_timestamp(dt)
                .map(Timestamp::as_second)
                .map_err(|_| ()),
            None => dt
                .to_zoned(now.time_zone().clone())
                .map(|z| z.timestamp().as_second())
                .map_err(|_| ()),
        }
    };
    let stripped = strip_unknown_zone(text);
    let mut parts = tokens(&stripped);
    if parts.is_empty() {
        return Err(());
    }
    // compact ISO: 20240229T102030
    if let [Token::Num(date), Token::Word(t), Token::Num(time)] = parts.as_slice()
        && t == "t"
        && date.len() == 8
        && time.len() == 6
    {
        parts = tokens(&format!(
            "{}-{}-{} {}:{}:{}",
            &date[..4],
            &date[4..6],
            &date[6..],
            &time[..2],
            &time[2..4],
            &time[4..]
        ));
    }
    // the ISO date/time separator
    if let Some(at) = parts
        .iter()
        .position(|t| matches!(t, Token::Word(w) if w == "t"))
        && at > 0
        && matches!(parts.get(at + 1), Some(Token::Num(_)))
    {
        parts.remove(at);
    }
    // the day words, alone or with a time
    let day_word = parts.iter().find_map(|t| match t {
        Token::Word(w) if matches!(w.as_str(), "now" | "today" | "yesterday" | "tomorrow") => {
            Some(w.clone())
        }
        _ => None,
    });
    if let Some(day) = day_word {
        parts.retain(|t| !matches!(t, Token::Word(w) if *w == day || w == "at"));
        let clock = take_clock(&mut parts, true)?;
        if !parts.is_empty() {
            return Err(());
        }
        let shift = match day.as_str() {
            "yesterday" => -1,
            "tomorrow" => 1,
            _ => 0,
        };
        let base = civil_now
            .checked_add(Span::new().days(shift))
            .map_err(|_| ())?;
        let dt = match clock.time {
            Some(time) => base.date().to_datetime(time),
            None => base,
        };
        return resolve(dt, clock.offset);
    }
    if let Some(dt) = relative(&parts, &civil_now)? {
        return resolve(dt, None);
    }
    let clock = take_clock(&mut parts, false)?;
    let weekday = parts.iter().find_map(|t| match t {
        Token::Word(w) => weekday_of(w),
        _ => None,
    });
    let date = take_date(&parts, &civil_now)?;
    if date.is_none() && clock.time.is_none() && weekday.is_none() {
        return Err(());
    }
    let mut date = date.unwrap_or_else(|| civil_now.date());
    // a weekday moves the date forward to it
    if let Some(day) = weekday {
        let have = usize::try_from(date.weekday().to_monday_zero_offset()).map_err(|_| ())?;
        let forward = (day + 7 - have) % 7;
        date = date
            .checked_add(Span::new().days(i64::try_from(forward).map_err(|_| ())?))
            .map_err(|_| ())?;
    }
    let time = clock.time.unwrap_or_else(Time::midnight);
    resolve(date.to_datetime(time), clock.offset)
}
