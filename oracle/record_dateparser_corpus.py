#!/usr/bin/env python3
"""Record the reference's "datestring to timestamp (easy)" on a corpus of the
date forms downloaders, HTTP headers and people meet.

The easy conversion is `ClientTime.ParseDate`: `dateparser.parse` when the
library is installed, else `dateutil.parser.parse` (then with `ignoretz`).
By the owner's decision (2026-10-08) hydrus-rs matches a dateparser-less
install, plus English relative dates as dateparser reads them. So:

* the `relative` group (now, yesterday, "2 hours ago", "in 3 weeks", ...) is
  recorded with dateparser, its `RELATIVE_BASE` held at 2026-10-04 12:30:00;
* every other group is recorded with `ClientTime.DATEPARSER_OK = False`, so the
  dateutil path runs, with dateutil's `default` held at that day's midnight
  (what it fills missing parts from) and the process in UTC.

Each case records the mode, the timestamp or the error.

Usage: ~/pyenv/bin/python oracle/record_dateparser_corpus.py
       (writes fixtures/dateparser_corpus.json)
"""
import datetime
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
os.environ['TZ'] = 'UTC'
time.tzset()

from hydrus.client import ClientStrings, ClientTime

NOW = datetime.datetime(2026, 10, 4, 12, 30, 0)
original_parse = ClientTime.dateparser.parse


def fixed_parse(text, **kwargs):
    kwargs.setdefault('settings', {})['RELATIVE_BASE'] = NOW
    return original_parse(text, **kwargs)


ClientTime.dateparser.parse = fixed_parse

import dateutil.parser
original_dateutil = dateutil.parser.parse


def fixed_dateutil(text, *args, **kwargs):
    kwargs.setdefault('default', NOW.replace(hour=0, minute=0, second=0, microsecond=0))
    return original_dateutil(text, *args, **kwargs)


ClientTime.dateutil.parser.parse = fixed_dateutil

GROUPS = {
    'iso': [
        '2024-02-29', '2024-02-29T10:20:30', '2024-02-29T10:20:30Z', '2024-02-29T10:20:30+00:00',
        '2024-02-29T10:20:30-05:00', '2024-02-29T10:20:30.123Z', '2024-02-29T10:20:30.123456+02:00',
        '2024-02-29 10:20:30', '2024-02-29 10:20', '2024-02-29 10:20:30 +0200', '2024-02-29 10:20:30+0200',
        '2024-02-29 10:20:30 UTC', '2024-02-29 10:20:30 GMT', '2024-02-29 10:20:30 EST', '2024-02-29 10:20:30 PST',
        '20240229', '20240229T102030', '2024-02', '2024', '2024/02/29', '2024.02.29', '2024-2-9', '2024-W09-4',
    ],
    'numeric': [
        '7/18/2023', '7/18/2023 8:32:00AM', '7/18/2023 8:32:00 AM', '7/18/2023 8:32 PM', '7/18/2023 20:32',
        '07/18/23', '7-18-2023', '7.18.2023', '18/7/2023', '1/2/2023', '12/31/1999', '02/29/2024', '02/30/2024',
        '13/13/2023', '1/2/03', '31.12.2023', '31-12-2023 23:59',
    ],
    'month_names': [
        '4 March 2020', '4 Mar 2020', 'March 4, 2020', 'Mar 4, 2020', 'March 4 2020', 'March 4th, 2020',
        '1st January 2020', '2nd Feb 2021', '3rd of March 2022', 'January 12, 2012 10:00 PM',
        'Jan 12, 2012 10:00 PM', 'January 12, 2012 22:00', 'Sept 5 2020', 'September 5, 2020', 'sep 5 2020',
        '5 SEPTEMBER 2020', 'march 2020', 'March 2020', 'Mar 2020', 'March 4', 'Mar 4',
        'Wednesday, March 4, 2020', 'Wed Mar 4 2020', 'Wed, 04 Mar 2020 12:00:00 GMT',
        'Wed, 02 Oct 2002 13:00:00 +0200', 'Wed, 02 Oct 2002 13:00:00 -0500', 'Thu Jan  1 00:00:00 1970',
        'Tuesday', 'monday', 'Sunday', 'Saturday', 'friday',
    ],
    'times': [
        '12:30', '12:30:45', '8:32 AM', '8:32 PM', '8:32pm', '12:00 AM', '12:00 PM', '00:00', '23:59:59',
        'noon', 'midnight',
    ],
    'relative': [
        'now', 'today', 'yesterday', 'tomorrow', '1 second ago', '30 seconds ago', '1 minute ago',
        '2 minutes ago', '1 hour ago', '2 hours ago', '3 days ago', '1 day ago', '1 week ago', '2 weeks ago',
        '1 month ago', '3 months ago', '1 year ago', '5 years ago', 'a day ago', 'an hour ago', 'a week ago',
        'a month ago', 'a year ago', 'in 2 days', 'in 1 hour', 'in 3 weeks', 'in a day', 'in an hour',
        '2 hours and 30 minutes ago', '1 year, 2 months ago', 'last week', 'last month', 'last year',
        'next week', 'next month', 'next year', 'last monday', 'two days ago', 'one hour ago',
        '2d ago', '5 min ago', '1h ago', '10s ago', '1w ago', '36 hours ago', '2 days 3 hours ago',
        '1m ago', '3 mins ago', '2 hrs ago', '1y ago', '2 yrs ago', '4 mo ago', '2 secs ago', '1 mon ago',
        'yesterday at 5pm', 'today at 5pm', 'tomorrow at noon', 'yesterday 17:00',
    ],
    'timestamps': [
        '1700000000', '1700000000.5', '1700000000000', '0', '-1',
    ],
    'timezones': [
        '2024-06-01 12:00 EDT', '2024-06-01 12:00 CEST', '2024-06-01 12:00 JST', '2024-06-01 12:00 +09:00',
        '2024-06-01 12:00 UTC+9', '2024-06-01 12:00 GMT+2', '2024-06-01 12:00 Europe/Paris',
    ],
    'other_languages': [
        'hier', 'il y a 3 jours', '4 mars 2020', '4. März 2020', '4 de marzo de 2020', '2020年3月4日',
        '2020년 3월 4일', '4 марта 2020', 'vor 2 Stunden', 'hace 2 horas',
    ],
    'junk': [
        '', ' ', 'not a date', 'abc', '2024-13-45', '99/99/9999', 'the 4th of never', '1 2 3',
        'posted on March 4, 2020 by someone', 'Uploaded: 2020-03-04 12:00',
    ],
}

EXTRA = {
    'iso': ['2024-02-29T10:20:30+0200', '2024-02-29T10:20:30 +02:00', '2024-02-29 10:20:30.5', '2024-02-29 10:20:30,5',
            '2024-02-29T25:00:00', '2024-02-30', '2024-W09-4', '2024-060'],
    'http': [
        'Thu, 20 May 2010 07:00:23 GMT', 'Wed, 21 Oct 2015 07:28:00 GMT', 'Sun, 06 Nov 1994 08:49:37 GMT',
        'Sunday, 06-Nov-94 08:49:37 GMT', 'Sun Nov  6 08:49:37 1994', 'Fri, 31 Dec 1999 23:59:59 +0000',
        'Tue, 15 Nov 1994 08:12:31 -0800', '120', 'Mon, 01 Jan 2024 00:00:00 UTC',
    ],
    'month_names': ['4 Mar 20', 'March 4, 20', 'Dec 25', '25 December', 'Fri Jan 5 2024 10:00:00 GMT+0100',
                    'January 12, 2012 10:00 PM EST', '1970-01-02 00:00:00 UTC'],
    'numeric': ['1/2/2023 13:45:00', '2023-1-2', '12.31.1999', '31/12/1999', '010203', '1999'],
    'times': ['5pm', '5 pm', '17h', '10:20:30.123'],
    'relative': ['1 day 2 hours ago', '2 days, 3 hours ago', '1 hour 30 minutes ago', '90 minutes ago', 'in 1 day 2 hours',
                 'in 2 weeks', 'in 1 month', 'in 1 year', '2 months ago', '11 months ago', '13 months ago',
                 '1 month 1 day ago', '29 days ago', '365 days ago', '0 seconds ago', '1 min ago'],
}
for group, extra in EXTRA.items():
    GROUPS.setdefault(group, [])
    GROUPS[group] += [t for t in extra if t not in GROUPS[group]]

# the relative forms (and the day words) go through dateparser; the rest do not
DAY_WORDS = {'now', 'today', 'yesterday', 'tomorrow'}

out = []
for group, texts in GROUPS.items():
    for text in texts:
        use_dateparser = group == 'relative' or text in DAY_WORDS
        ClientTime.DATEPARSER_OK = use_dateparser
        converter = ClientStrings.StringConverter(conversions=[(14, None)])
        try:
            result = converter.Convert(text)
            error = False
        except Exception as exc:
            result = str(exc)
            error = True
        out.append({'group': group, 'text': text, 'mode': 'dateparser' if use_dateparser else 'dateutil',
                    'result': result, 'error': error})
ClientTime.DATEPARSER_OK = True

# `Last-Modified` without dateparser (ClientNetworkingJobs._GenerateModifiedDate):
# the fixed strptime form, "GMT" dropped, the rest read as local time
from hydrus.core import HydrusTime

last_modified = []
for text in ['Thu, 20 May 2010 07:00:23 GMT', 'Wed, 21 Oct 2015 07:28:00 GMT', 'Thu, 01 Jan 1970 00:00:00 GMT',
             'Fri, 02 Jan 1970 00:00:00 GMT', 'Thu, 08 Jan 1970 00:00:01 GMT', 'Sunday, 06-Nov-94 08:49:37 GMT',
             'Sun Nov  6 08:49:37 1994', '2010-05-20T07:00:23Z', 'Thu, 20 May 2010 07:00:23 +0200', 'garbage',
             'Thu, 20 May 2010 07:00:23']:
    try:
        string = text[:-4] if text.endswith(' GMT') else text
        dt = datetime.datetime.strptime(string, '%a, %d %b %Y %H:%M:%S')
        stamp = HydrusTime.DateTimeToTimestamp(dt)
        result = stamp if ClientTime.TimestampIsSensible(stamp) else None
    except Exception:
        result = None
    last_modified.append({'text': text, 'result': result})

path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'fixtures', 'dateparser_corpus.json')
with open(path, 'w') as f:
    json.dump({'now': NOW.isoformat(), 'timezone': 'UTC', 'cases': out, 'last_modified': last_modified}, f, indent=1, ensure_ascii=False)
    f.write('\n')
print(f'wrote {path}: {len(out)} cases, {sum(1 for c in out if c["error"])} errors')
