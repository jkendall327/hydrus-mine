//! Bandwidth rules and usage: the reference's `HydrusNetworking.BandwidthRules`
//! and `BandwidthTracker`, and the rules-per-network-context part of
//! `ClientNetworkingBandwidth.NetworkBandwidthManager`.
//!
//! A rule caps requests or bytes over a span of seconds (or the calendar
//! month). Usage is counted in UTC calendar buckets (months, days, hours,
//! minutes, seconds) and a span is measured from the bucket size that suits
//! it, so a "per day" rule slides by the hour. Everything takes the time
//! (`now`, whole seconds) as an argument, as the reference's tests patch its
//! clock, and is checked against the reference by
//! `crates/hydrus-core/tests/bandwidth.rs`.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::network::{
    CONTEXT_DOMAIN, CONTEXT_DOWNLOADER_PAGE, CONTEXT_GLOBAL, CONTEXT_HYDRUS, CONTEXT_SUBSCRIPTION,
    CONTEXT_WATCHER_PAGE, NetworkContext,
};

/// What a rule counts (`HC.BANDWIDTH_TYPE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BandwidthType {
    Data = 0,
    Requests = 1,
}

impl BandwidthType {
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Data),
            1 => Some(Self::Requests),
            _ => None,
        }
    }
}

/// At most `max_allowed` of `kind` in the past `time_delta` seconds (`None`:
/// this calendar month).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Rule {
    pub kind: BandwidthType,
    pub time_delta: Option<u64>,
    pub max_allowed: u64,
}

impl Rule {
    pub const fn new(kind: BandwidthType, time_delta: Option<u64>, max_allowed: u64) -> Self {
        Self {
            kind,
            time_delta,
            max_allowed,
        }
    }
}

/// A context's rules (a set: the same rule twice is one rule).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    rules: Vec<Rule>,
}

impl Rules {
    pub fn new(rules: impl IntoIterator<Item = Rule>) -> Self {
        let mut out = Self::default();
        for rule in rules {
            out.add(rule);
        }
        out
    }

    pub fn add(&mut self, rule: Rule) {
        if !self.rules.contains(&rule) {
            self.rules.push(rule);
        }
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Whether a download under way may carry on: only data rules over 15
    /// seconds or less (speed limits) can pause it.
    pub fn can_continue_download(&self, tracker: &mut Tracker, now: i64) -> bool {
        const THRESHOLD: u64 = 15;
        self.rules.iter().all(|rule| {
            let ignore = rule.kind == BandwidthType::Requests
                || rule.time_delta.is_none_or(|d| d > THRESHOLD);
            ignore || tracker.usage(rule.kind, rule.time_delta, now) < rule.max_allowed
        })
    }

    /// Whether there is room for a decent whack of work (`expected_requests`
    /// and `expected_bytes` more), ignoring rules over `threshold` seconds or
    /// less.
    pub fn can_do_work(
        &self,
        tracker: &mut Tracker,
        expected_requests: u64,
        expected_bytes: u64,
        threshold: u64,
        now: i64,
    ) -> bool {
        self.rules.iter().all(|rule| {
            if rule.time_delta.is_some_and(|d| d <= threshold) {
                return true;
            }
            let expected = match rule.kind {
                BandwidthType::Requests => expected_requests,
                BandwidthType::Data => expected_bytes,
            };
            let max_allowed = i128::from(rule.max_allowed) - i128::from(expected);
            i128::from(tracker.usage(rule.kind, rule.time_delta, now)) < max_allowed
        })
    }

    /// Whether a new request may start: data rules over 5 seconds or less
    /// (the current download speed) don't stop it.
    pub fn can_start_request(&self, tracker: &mut Tracker, now: i64) -> bool {
        const THRESHOLD: u64 = 5;
        self.rules.iter().all(|rule| {
            let ignore =
                rule.kind == BandwidthType::Data && rule.time_delta.is_some_and(|d| d <= THRESHOLD);
            ignore || tracker.usage(rule.kind, rule.time_delta, now) < rule.max_allowed
        })
    }

    /// Seconds until every rule now used up has room again (0 if none is).
    pub fn waiting_estimate(&self, tracker: &mut Tracker, now: i64) -> u64 {
        let mut longest = 0;
        for rule in &self.rules {
            if tracker.usage(rule.kind, rule.time_delta, now) >= rule.max_allowed {
                let wait =
                    tracker.waiting_estimate(rule.kind, rule.time_delta, rule.max_allowed, now);
                longest = longest.max(wait);
            }
        }
        longest
    }
}

/// The reference's default rules (`ClientDefaults.
/// SetDefaultBandwidthManagerRules`), for a new install.
pub fn default_rules() -> Vec<(NetworkContext, Rules)> {
    use BandwidthType::{Data, Requests};
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * MB;
    let day = Some(86_400);
    vec![
        (
            NetworkContext::global(),
            // stop accidental spam; check your inbox lad
            Rules::new([
                Rule::new(Requests, Some(1), 5),
                Rule::new(Data, day, 16 * GB),
            ]),
        ),
        (
            NetworkContext::default_of_kind(CONTEXT_DOMAIN),
            // don't ever hammer a domain; don't go nuts on a site in a day
            Rules::new([
                Rule::new(Requests, Some(1), 1),
                Rule::new(Data, day, 8 * GB),
            ]),
        ),
        (
            NetworkContext::default_of_kind(CONTEXT_HYDRUS),
            Rules::new([Rule::new(Data, day, 2 * GB)]),
        ),
        (
            NetworkContext::default_of_kind(CONTEXT_DOWNLOADER_PAGE),
            Rules::new([Rule::new(Data, Some(300), 1024 * MB)]),
        ),
        (
            NetworkContext::default_of_kind(CONTEXT_SUBSCRIPTION),
            // catch up on a swell of many new things in chunks every day
            Rules::new([
                Rule::new(Requests, day, 1000),
                Rule::new(Data, day, 1024 * MB),
            ]),
        ),
        // watchers have time pressure: only the global and domain limits
        (
            NetworkContext::default_of_kind(CONTEXT_WATCHER_PAGE),
            Rules::default(),
        ),
    ]
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (year, month, day) to days since 1970-01-01.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The start of `now`'s UTC calendar month.
fn month_start(now: i64) -> i64 {
    let (y, m, _) = civil_from_days(now.div_euclid(86_400));
    days_from_civil(y, m, 1) * 86_400
}

/// The start of the UTC calendar month after `now`'s.
fn next_month_start(now: i64) -> i64 {
    let (y, m, _) = civil_from_days(now.div_euclid(86_400));
    let (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(y, m, 1) * 86_400
}

/// Usage counts per bucket (bucket start time to amount).
type Counter = BTreeMap<i64, u64>;

/// One kind's counters, from months down to seconds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Counters {
    months: Counter,
    days: Counter,
    hours: Counter,
    minutes: Counter,
    seconds: Counter,
}

/// How much a context has used, in calendar buckets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tracker {
    bytes: Counters,
    requests: Counters,
    /// When old buckets are next dropped.
    #[serde(skip, default)]
    next_maintenance: i64,
}

impl Tracker {
    /// Spans shorter than these are measured in the next bucket size down.
    const MAX_SECONDS_TIME_DELTA: i64 = 240;
    const MAX_MINUTES_TIME_DELTA: i64 = 180 * 60;
    const MAX_HOURS_TIME_DELTA: i64 = 72 * 3600;
    const MAX_DAYS_TIME_DELTA: i64 = 31 * 86_400;
    const CACHE_MAINTENANCE_TIME_DELTA: i64 = 120;

    /// A new tracker (or one just loaded), at `now`.
    pub fn new(now: i64) -> Self {
        Self {
            bytes: Counters::default(),
            requests: Counters::default(),
            next_maintenance: now + Self::CACHE_MAINTENANCE_TIME_DELTA,
        }
    }

    /// A stored tracker (its counters in the reference's order: months,
    /// days, hours, minutes and seconds of bytes, then of requests), loaded
    /// at `now`.
    pub fn from_counters(counters: [Vec<(i64, u64)>; 10], now: i64) -> Self {
        let mut t = Self::new(now);
        let [mb, db, hb, minb, sb, mr, dr, hr, minr, sr] = counters;
        let c = |v: Vec<(i64, u64)>| {
            let mut out = Counter::new();
            for (k, n) in v {
                *out.entry(k).or_default() += n;
            }
            out
        };
        t.bytes = Counters {
            months: c(mb),
            days: c(db),
            hours: c(hb),
            minutes: c(minb),
            seconds: c(sb),
        };
        t.requests = Counters {
            months: c(mr),
            days: c(dr),
            hours: c(hr),
            minutes: c(minr),
            seconds: c(sr),
        };
        t
    }

    /// A stored tracker, loaded at `now` (old buckets are next dropped two
    /// minutes from then, as when the reference loads one).
    #[must_use]
    pub fn loaded_at(mut self, now: i64) -> Self {
        self.next_maintenance = now + Self::CACHE_MAINTENANCE_TIME_DELTA;
        self
    }

    /// The counters, in [`Tracker::from_counters`]'s order.
    pub fn to_counters(&self) -> [Vec<(i64, u64)>; 10] {
        let v = |c: &Counter| c.iter().map(|(&k, &n)| (k, n)).collect();
        [
            v(&self.bytes.months),
            v(&self.bytes.days),
            v(&self.bytes.hours),
            v(&self.bytes.minutes),
            v(&self.bytes.seconds),
            v(&self.requests.months),
            v(&self.requests.days),
            v(&self.requests.hours),
            v(&self.requests.minutes),
            v(&self.requests.seconds),
        ]
    }

    fn counters(&self, kind: BandwidthType) -> &Counters {
        match kind {
            BandwidthType::Data => &self.bytes,
            BandwidthType::Requests => &self.requests,
        }
    }

    /// The bucket size a span is measured in (`_GetWindowAndCounter`): the
    /// width of a bucket less a second, and its counter.
    fn window_and_counter(&self, kind: BandwidthType, time_delta: u64) -> (i64, &Counter) {
        let c = self.counters(kind);
        let d = i64::try_from(time_delta).unwrap_or(i64::MAX);
        if d < Self::MAX_SECONDS_TIME_DELTA {
            (0, &c.seconds)
        } else if d < Self::MAX_MINUTES_TIME_DELTA {
            (59, &c.minutes)
        } else if d < Self::MAX_HOURS_TIME_DELTA {
            (3599, &c.hours)
        } else {
            (86_399, &c.days)
        }
    }

    fn raw_usage(&self, kind: BandwidthType, time_delta: Option<u64>, now: i64) -> u64 {
        let Some(time_delta) = time_delta else {
            let c = self.counters(kind);
            return c.months.get(&month_start(now)).copied().unwrap_or(0);
        };
        let (window, counter) = self.window_and_counter(kind, time_delta);
        if time_delta == 1 {
            // only the current second: the bucket width is a second too
            return counter.get(&now).copied().unwrap_or(0);
        }
        // a bucket counts if any of it falls in the span
        let since = now
            .saturating_sub(i64::try_from(time_delta).unwrap_or(i64::MAX))
            .saturating_sub(window);
        // (never the future: a clock once went back decades)
        counter.range(since..=now).map(|(_, &n)| n).sum()
    }

    /// Drop buckets too old to matter, every couple of minutes.
    fn maintain(&mut self, now: i64) {
        if now <= self.next_maintenance {
            return;
        }
        let keep = |c: &mut Counter, oldest: i64| c.retain(|&t, _| t >= oldest);
        for c in [&mut self.bytes, &mut self.requests] {
            keep(&mut c.days, now - Self::MAX_DAYS_TIME_DELTA);
            keep(&mut c.hours, now - Self::MAX_HOURS_TIME_DELTA);
            keep(&mut c.minutes, now - Self::MAX_MINUTES_TIME_DELTA);
            keep(&mut c.seconds, now - Self::MAX_SECONDS_TIME_DELTA);
        }
        self.next_maintenance = now + Self::CACHE_MAINTENANCE_TIME_DELTA;
    }

    /// How much of `kind` was used in the past `time_delta` seconds (`None`:
    /// this calendar month).
    pub fn usage(&mut self, kind: BandwidthType, time_delta: Option<u64>, now: i64) -> u64 {
        if time_delta == Some(0) {
            return 0;
        }
        let usage = self.raw_usage(kind, time_delta, now);
        self.maintain(now);
        usage
    }

    /// Everything used this calendar month and before (the months kept).
    pub fn all_usage(&self, kind: BandwidthType) -> u64 {
        self.counters(kind).months.values().sum()
    }

    /// Seconds until usage over the span is below `max_allowed` again.
    pub fn waiting_estimate(
        &self,
        kind: BandwidthType,
        time_delta: Option<u64>,
        max_allowed: u64,
        now: i64,
    ) -> u64 {
        let Some(time_delta) = time_delta else {
            return u64::try_from(next_month_start(now) - now).unwrap_or(0);
        };
        let (window, counter) = self.window_and_counter(kind, time_delta);
        let span = i64::try_from(time_delta).unwrap_or(i64::MAX) + window;
        let mut usage = 0;
        // the most recent first: wait until enough of it ages out
        for (&timestamp, &n) in counter.iter().rev() {
            let age = now - timestamp;
            if age > span {
                break;
            }
            usage += n;
            if usage >= max_allowed {
                return u64::try_from(span - age).unwrap_or(0);
            }
        }
        0
    }

    fn report(&mut self, kind: BandwidthType, n: u64, now: i64) {
        let day = now - now.rem_euclid(86_400);
        let hour = now - now.rem_euclid(3600);
        let minute = now - now.rem_euclid(60);
        let month = month_start(now);
        let c = match kind {
            BandwidthType::Data => &mut self.bytes,
            BandwidthType::Requests => &mut self.requests,
        };
        *c.months.entry(month).or_default() += n;
        *c.days.entry(day).or_default() += n;
        *c.hours.entry(hour).or_default() += n;
        *c.minutes.entry(minute).or_default() += n;
        *c.seconds.entry(now).or_default() += n;
        self.maintain(now);
    }

    pub fn report_data(&mut self, bytes: u64, now: i64) {
        self.report(BandwidthType::Data, bytes, now);
    }

    pub fn report_requests(&mut self, requests: u64, now: i64) {
        self.report(BandwidthType::Requests, requests, now);
    }
}

/// Rules per network context and each context's usage: the bandwidth half of
/// the reference's `NetworkBandwidthManager`.
#[derive(Debug, Clone)]
pub struct Manager {
    rules: HashMap<NetworkContext, Rules>,
    trackers: HashMap<NetworkContext, Tracker>,
    /// Contexts whose usage changed since it was last saved.
    dirty: HashSet<NetworkContext>,
    /// Per gallery token kind: second-level domain to when one was last
    /// taken.
    gallery_tokens: HashMap<GalleryTokenKind, HashMap<String, i64>>,
}

/// Which gallery pages wait their turn together (the reference's gallery
/// token names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GalleryTokenKind {
    /// Gallery downloaders ("download page").
    DownloadPage,
    Subscription,
    Watcher,
}

impl Default for Manager {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Manager {
    /// A manager with these rules; each kind's default is there even if
    /// `rules` doesn't set it (as no rules).
    pub fn new(rules: Vec<(NetworkContext, Rules)>) -> Self {
        let mut map: HashMap<NetworkContext, Rules> = [
            CONTEXT_GLOBAL,
            CONTEXT_HYDRUS,
            CONTEXT_DOMAIN,
            CONTEXT_DOWNLOADER_PAGE,
            CONTEXT_SUBSCRIPTION,
            CONTEXT_WATCHER_PAGE,
        ]
        .into_iter()
        .map(|kind| (NetworkContext::default_of_kind(kind), Rules::default()))
        .collect();
        for (context, r) in rules {
            map.insert(context, r);
        }
        Self {
            rules: map,
            trackers: HashMap::new(),
            dirty: HashSet::new(),
            gallery_tokens: HashMap::new(),
        }
    }

    /// Replace every context's rules (as [`Manager::new`] sets them),
    /// keeping the usage so far.
    pub fn set_all_rules(&mut self, rules: Vec<(NetworkContext, Rules)>) {
        self.rules = Self::new(rules).rules;
    }

    /// Carry on from stored usage.
    pub fn set_trackers(&mut self, trackers: impl IntoIterator<Item = (NetworkContext, Tracker)>) {
        self.trackers = trackers.into_iter().collect();
        self.dirty.clear();
    }

    /// Forget selected usage trackers while retaining their configured rules.
    pub fn delete_history(&mut self, contexts: &[NetworkContext]) {
        for context in contexts {
            self.trackers.remove(context);
            self.dirty.remove(context);
        }
    }

    /// Current context trackers, including ephemeral page contexts, for live review.
    pub fn all_trackers(&self) -> Vec<(NetworkContext, Tracker)> {
        let mut trackers: Vec<_> = self
            .trackers
            .iter()
            .map(|(c, t)| (c.clone(), t.clone()))
            .collect();
        trackers.sort_by(|a, b| a.0.cmp(&b.0));
        trackers
    }

    /// Every context's rules (defaults included).
    pub fn all_rules(&self) -> Vec<(NetworkContext, Rules)> {
        let mut all: Vec<_> = self
            .rules
            .iter()
            .map(|(c, r)| (c.clone(), r.clone()))
            .collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all
    }

    /// The rules that apply to `context`: its own, else its kind's default.
    pub fn rules_for(&self, context: &NetworkContext) -> &Rules {
        self.rules
            .get(context)
            .or_else(|| {
                self.rules
                    .get(&NetworkContext::default_of_kind(context.kind))
            })
            .unwrap_or(&EMPTY)
    }

    /// Set a context's rules; no rules on a specific context means it
    /// follows its kind's default again.
    pub fn set_rules(&mut self, context: NetworkContext, rules: Rules) {
        if rules.is_empty() && !context.is_default() && context.kind != CONTEXT_GLOBAL {
            self.rules.remove(&context);
        } else {
            self.rules.insert(context, rules);
        }
    }

    fn tracker(&mut self, context: &NetworkContext, now: i64) -> &mut Tracker {
        self.trackers
            .entry(context.clone())
            .or_insert_with(|| Tracker::new(now))
    }

    /// `context`'s usage so far (an empty tracker if it has none).
    pub fn usage(
        &mut self,
        context: &NetworkContext,
        kind: BandwidthType,
        time_delta: Option<u64>,
        now: i64,
    ) -> u64 {
        match self.trackers.get_mut(context) {
            Some(t) => t.usage(kind, time_delta, now),
            None => 0,
        }
    }

    fn check(
        &mut self,
        contexts: &[NetworkContext],
        now: i64,
        test: impl Fn(&Rules, &mut Tracker) -> bool,
    ) -> bool {
        for context in contexts {
            let rules = self.rules_for(context).clone();
            let tracker = self.tracker(context, now);
            if !test(&rules, tracker) {
                return false;
            }
        }
        true
    }

    pub fn can_start_request(&mut self, contexts: &[NetworkContext], now: i64) -> bool {
        self.check(contexts, now, |r, t| r.can_start_request(t, now))
    }

    pub fn can_continue_download(&mut self, contexts: &[NetworkContext], now: i64) -> bool {
        self.check(contexts, now, |r, t| r.can_continue_download(t, now))
    }

    /// Whether there is room for a request and a megabyte, ignoring rules
    /// over `threshold` seconds or less.
    pub fn can_do_work(&mut self, contexts: &[NetworkContext], threshold: u64, now: i64) -> bool {
        self.check(contexts, now, |r, t| {
            r.can_do_work(t, 1, 1_048_576, threshold, now)
        })
    }

    /// Start a request if every context's rules allow it, counting it.
    pub fn try_to_start_request(&mut self, contexts: &[NetworkContext], now: i64) -> bool {
        if !self.can_start_request(contexts, now) {
            return false;
        }
        self.report_request(contexts, now);
        true
    }

    /// Count a request (one that started without asking, too).
    pub fn report_request(&mut self, contexts: &[NetworkContext], now: i64) {
        for context in contexts {
            self.tracker(context, now).report_requests(1, now);
            if !context.is_ephemeral() {
                self.dirty.insert(context.clone());
            }
        }
    }

    pub fn report_data(&mut self, contexts: &[NetworkContext], bytes: u64, now: i64) {
        for context in contexts {
            self.tracker(context, now).report_data(bytes, now);
            if !context.is_ephemeral() {
                self.dirty.insert(context.clone());
            }
        }
    }

    /// The longest wait among `contexts`, and whose it is (the first of the
    /// longest; the global context if there are none).
    pub fn waiting_estimate_and_context(
        &mut self,
        contexts: &[NetworkContext],
        now: i64,
    ) -> (u64, NetworkContext) {
        let mut best: Option<(u64, NetworkContext)> = None;
        for context in contexts {
            let rules = self.rules_for(context).clone();
            let estimate = rules.waiting_estimate(self.tracker(context, now), now);
            if best.as_ref().is_none_or(|(b, _)| estimate > *b) {
                best = Some((estimate, context.clone()));
            }
        }
        best.unwrap_or_else(|| (0, NetworkContext::global()))
    }

    /// Take a gallery token for `second_level_domain` if `delay` seconds
    /// have passed since the last of its kind; else when the next is due.
    pub fn try_to_consume_gallery_token(
        &mut self,
        second_level_domain: &str,
        kind: GalleryTokenKind,
        delay: i64,
        now: i64,
    ) -> Result<(), i64> {
        let last = self.gallery_tokens.entry(kind).or_default();
        let next = last.get(second_level_domain).copied().unwrap_or(0) + delay;
        if now > next {
            last.insert(second_level_domain.to_owned(), now);
            Ok(())
        } else {
            Err(next)
        }
    }

    /// The usage changed since the last save (for keeping), and forget that
    /// it changed.
    pub fn take_dirty(&mut self) -> Vec<(NetworkContext, Tracker)> {
        let dirty = std::mem::take(&mut self.dirty);
        dirty
            .into_iter()
            .filter_map(|c| self.trackers.get(&c).map(|t| (c, t.clone())))
            .collect()
    }
}

static EMPTY: Rules = Rules { rules: Vec::new() };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_buckets_are_utc() {
        // 2024-02-29T13:45:30Z
        let t = 1_709_214_330;
        assert_eq!(month_start(t), 1_706_745_600); // 2024-02-01
        assert_eq!(next_month_start(t), 1_709_251_200); // 2024-03-01
        // 2023-12-31T23:59:59Z
        assert_eq!(next_month_start(1_704_067_199), 1_704_067_200);
        assert_eq!(month_start(0), 0);
    }

    #[test]
    fn a_one_per_second_rule_waits_for_the_next_second() {
        let rules = Rules::new([Rule::new(BandwidthType::Requests, Some(1), 1)]);
        let mut t = Tracker::new(100);
        assert!(rules.can_start_request(&mut t, 100));
        t.report_requests(1, 100);
        assert!(!rules.can_start_request(&mut t, 100));
        assert_eq!(rules.waiting_estimate(&mut t, 100), 1);
        assert!(rules.can_start_request(&mut t, 101));
    }

    #[test]
    fn a_daily_limit_holds_until_usage_ages_out() {
        let rules = Rules::new([Rule::new(BandwidthType::Requests, Some(86_400), 2)]);
        let start = 1_700_000_000;
        let mut t = Tracker::new(start);
        t.report_requests(2, start);
        assert!(!rules.can_start_request(&mut t, start + 3600));
        let wait = rules.waiting_estimate(&mut t, start + 3600);
        // measured in hours: the bucket the usage fell in ages out
        assert!(wait > 0 && wait <= 86_400, "{wait}");
        assert!(rules.can_start_request(&mut t, start + 3600 + i64::try_from(wait).unwrap() + 1));
    }

    #[test]
    fn specific_rules_replace_the_default() {
        let mut m = Manager::new(default_rules());
        let site = NetworkContext::domain("example.com");
        assert_eq!(
            m.rules_for(&site),
            m.rules_for(&NetworkContext::default_of_kind(2))
        );
        m.set_rules(
            site.clone(),
            Rules::new([Rule::new(BandwidthType::Requests, Some(4), 1)]),
        );
        let contexts = [NetworkContext::global(), site.clone()];
        assert!(m.try_to_start_request(&contexts, 10));
        assert!(!m.try_to_start_request(&contexts, 12));
        assert_eq!(
            m.waiting_estimate_and_context(&contexts, 12),
            (2, site.clone())
        );
        // (the span includes both ends, as the reference counts it)
        assert!(!m.try_to_start_request(&contexts, 14));
        assert!(m.try_to_start_request(&contexts, 15));
        // no rules on a specific context: back to the default
        m.set_rules(site.clone(), Rules::default());
        assert!(m.rules_for(&site).rules().len() == 2);
    }

    #[test]
    fn gallery_tokens_space_out_each_site() {
        let mut m = Manager::default();
        let k = GalleryTokenKind::Subscription;
        assert_eq!(
            m.try_to_consume_gallery_token("a.example", k, 5, 100),
            Ok(())
        );
        assert_eq!(
            m.try_to_consume_gallery_token("a.example", k, 5, 105),
            Err(105)
        );
        assert_eq!(
            m.try_to_consume_gallery_token("b.example", k, 5, 105),
            Ok(())
        );
        assert_eq!(
            m.try_to_consume_gallery_token("a.example", GalleryTokenKind::Watcher, 5, 105),
            Ok(())
        );
        assert_eq!(
            m.try_to_consume_gallery_token("a.example", k, 5, 106),
            Ok(())
        );
    }
}
