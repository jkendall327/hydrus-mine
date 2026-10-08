#!/usr/bin/env python3
"""Record the reference's "datestring to timestamp (easy)" on a corpus of the
date forms downloaders and users meet.

The easy conversion is `ClientTime.ParseDate`, which is `dateparser.parse`
(then its English fallback). This runs the real conversion on each string at
a fixed UTC clock (dateparser's `RELATIVE_BASE` is held at 2026-10-04
12:30:00 and the process runs in UTC) and records the timestamp or the
error. Forms are grouped so the port's coverage can be stated by group.

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

out = []
for group, texts in GROUPS.items():
    for text in texts:
        converter = ClientStrings.StringConverter(conversions=[(14, None)])
        try:
            result = converter.Convert(text)
            error = False
        except Exception as exc:
            result = str(exc)
            error = True
        out.append({'group': group, 'text': text, 'result': result, 'error': error})

path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'fixtures', 'dateparser_corpus.json')
with open(path, 'w') as f:
    json.dump({'now': NOW.isoformat(), 'timezone': 'UTC', 'cases': out}, f, indent=1, ensure_ascii=False)
    f.write('\n')
print(f'wrote {path}: {len(out)} cases, {sum(1 for c in out if c["error"])} errors')
