#!/usr/bin/env python3
"""Record the reference's "timestamp to datestring" in local time, away from UTC.

`TimestampToDateTime` adds the machine's *current* UTC offset (not the offset
then) to the timestamp, so a winter and a summer timestamp are shifted by the
same amount. The reference's StringConverter is run under several system time
zones; each case records the offset in force at the time (what Python's
`datetime.now().astimezone()` reports) and the result.

Usage: python3 oracle/record_date_encode_local.py
       (writes fixtures/date_encode_local.json)
"""
import datetime
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from hydrus.client import ClientStrings

ZONES = ['America/New_York', 'Asia/Kolkata', 'Australia/Lord_Howe', 'Pacific/Auckland', 'UTC']
STAMPS = ['1700000000', '1720000000', '-1', '0', '1000000000']
PHRASES = ['%Y-%m-%d %H:%M:%S', '%Y-%m-%d %H:%M:%S%z%Z', '%a %d %b %Y %I:%M %p']

cases = []

for zone in ZONES:

    os.environ['TZ'] = zone
    time.tzset()

    offset = int(datetime.datetime.now().astimezone().tzinfo.utcoffset(None).total_seconds())

    for phrase in PHRASES:

        for stamp in STAMPS:

            converter = ClientStrings.StringConverter(conversions=[(12, (phrase, 1))])

            try:

                result = converter.Convert(stamp)
                error = False

            except Exception as exc:

                result = str(exc)
                error = True


            cases.append({'zone': zone, 'offset': offset, 'phrase': phrase, 'text': stamp, 'result': result, 'error': error})



path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'fixtures/date_encode_local.json')

with open(path, 'w') as f:

    json.dump({'cases': cases}, f, indent=1)
    f.write('\n')


print(f'wrote {path}: {len(cases)} cases')
