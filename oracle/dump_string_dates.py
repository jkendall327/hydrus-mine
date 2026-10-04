#!/usr/bin/env python3
"""Record date string conversions, including invalid and timezone boundaries.

Reference execution is captured directly at a fixed UTC clock. The GUI recorder
record_string_date_editor.py separately captures live Qt controls and previews.
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
CASES = [
    (10, ['%Y-%m-%d', 0, 0], '2024-02-29'),
    (10, ['%Y-%m-%d %H:%M:%S', 2, 19800], '1970-01-01 05:30:00'),
    (10, ['%Y-%m-%d %H:%M:%S', 2, -18000], '1969-12-31 19:00:00'),
    (10, ['%Y-%m-%dT%H:%M:%S%z', 0, 0], '2024-01-01T12:00:00+0530'),
    (10, ['%Y-%m-%dT%H:%M:%S%z', 1, 0], '2024-01-01T12:00:00+0530'),
    (10, ['%Y-%m-%d %H:%M:%S.%f', 0, 0], '1969-12-31 23:59:59.900000'),
    (10, ['%Y-%m-%d %H:%M:%S.%f', 1, 0], '1969-12-31 23:59:59.900000'),
    (10, ['%H:%M', 0, 0], '12:34'),
    (10, ['%Y %j', 0, 0], '2024 060'),
    (10, ['%Y-%m-%d', 0, 0], '2023-02-29'),
    (10, ['%Y-%m-%d', 0, 0], '2024-01-01extra'),
    (10, ['%Y-%m-%d', 0, 0], 'not a date'),
    (10, ['%Y-%m-%d %H:%M:%S.%f', 0, 0], '2024-01-01 12:00:00.123456'),
    (10, ['%Y-%m-%d %H:%M:%S.%f', 0, 0], '2024-01-01 12:00:00.1234567'),
    (10, ['%Y-%m-%d %H:%M:%S.%f', 0, 0], '2024-01-01 12:00:00.000000000'),
    (10, ['%Y-%m-%d %Z', 0, 0], '2024-01-01 UTC'),
    (10, ['%Y-%m-%d %Z', 0, 0], '2024-01-01 gmt'),
    (10, ['%Y-%m-%d %Z', 0, 0], '2024-01-01 EST'),
    (10, ['%Y-%m-%d %Z', 0, 0], '2024-01-01 nowhere'),
    (10, ['%F', 0, 0], '2024-01-01'),
    (10, ['%c', 0, 0], 'Thu Jan  1 00:00:00 1970'),
    (10, ['%x %X', 0, 0], '01/01/70 00:00:00'),
    (12, ['%Y-%m-%d %H:%M:%S %f %z %Z', 0], '-1'),
    (12, ['%Y-%m-%d %H:%M:%S', 0], ' +1_700_000_000 '),
    (12, ['%c / %x / %X / %%', 0], '0'),
    (12, ['%Y', 0], '1.5'),
    (12, ['%Q %%f %', 0], '0'),
    (12, ['%Y', 0], '1__2'),
    (12, ['%Y', 0], '+_1'),
    (12, ['%Y', 0], '-62167219200'),
    (10, ['%Y-%m-%d', 0, 0], '0000-01-01'),
    (12, ['%Y', 0], '9223372036854775807'),
]
for text in ['2024-02-29', '2024-01-01T12:00:00+05:30', '2024-01-01 12:00:00',
             '7/18/2023 8:32:00AM', '4 March 2020', 'January 12, 2012 10:00 PM',
             'Wed, 02 Oct 2002 13:00:00 +0200', '2 hours ago', '3 days ago',
             'yesterday', 'tomorrow', 'today', 'now', 'not a date']:
    CASES.append((14, None, text))
out = []
for code, data, text in CASES:
    converter = ClientStrings.StringConverter(conversions=[(code, tuple(data) if isinstance(data, list) else data)])
    try:
        result = converter.Convert(text)
        error = False
    except Exception as exc:
        result = str(exc)
        error = True
    out.append({'conversion': [code, data], 'text': text, 'result': result, 'error': error})
local_contexts = []
os.environ['TZ'] = 'America/New_York'
time.tzset()
for text, phrase in [('2024-11-03 01:30:00', '%Y-%m-%d %H:%M:%S'),
                     ('2024-03-10 02:30:00', '%Y-%m-%d %H:%M:%S'),
                     ('2024-01-01 EST', '%Y-%m-%d %Z'),
                     ('2024-07-01 EDT', '%Y-%m-%d %Z'),
                     ('2024-01-01 PST', '%Y-%m-%d %Z')]:
    converter = ClientStrings.StringConverter(conversions=[(10, (phrase, 1, 0))])
    try:
        result = converter.Convert(text)
        error = False
    except Exception as exc:
        result = str(exc)
        error = True
    local_contexts.append({'system_timezone': 'America/New_York', 'text': text, 'phrase': phrase, 'result': result, 'error': error})
os.environ['TZ'] = 'UTC'
time.tzset()
path = os.path.join(os.path.dirname(__file__), 'fixtures/string_dates.json')
with open(path, 'w') as stream:
    json.dump({'now': NOW.isoformat(), 'timezone': 'UTC', 'cases': out, 'local_contexts': local_contexts}, stream, indent=1)
    stream.write('\n')
print(f'wrote {path}: {len(out)} cases')
