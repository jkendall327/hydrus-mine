#!/usr/bin/env python3
"""Record real Qt date fields, live previews and converter accept/cancel/reorder.

Reuse the converter recorder's Qt driver with a dedicated date sequence, under
UTC so local controls and historical timestamps remain reproducible.
"""
import os
import time
import record_string_converter_editor as recorder
os.environ['TZ'] = 'UTC'
time.tzset()
recorder.OUT = os.path.join(os.path.dirname(__file__), 'fixtures/string_date_editor.json')
recorder.CASES = [{
    'conversions': [], 'example': '2024-02-29', 'do': [
        ['add', {'type': 'datestring to timestamp (advanced)', 'text': '%Y-%m-%d', 'timezone_decode': 'UTC'}],
        ['add', {'type': 'timestamp to datestring', 'text': '%Y-%m-%d %H:%M:%S', 'timezone_encode': 'UTC'}],
        ['click', 0, False],
        ['edit', {'timezone_decode': 'Offset', 'offset': 3600}],
        ['edit', None],
        ['example', '2023-02-29'],
        ['example', '1970-01-01'],
        ['click', 0, False],
        ['edit', {'timezone_decode': 'Local'}],
        ['click', 1, False],
        ['edit', {'timezone_encode': 'Local', 'text': '%Y-%m-%d %H:%M:%S %f %z %Z'}],
        ['up'], ['down'],
        ['click', 0, False],
        ['edit', {'type': 'datestring to timestamp (easy)'}],
        ['example', '7/18/2023 8:32:00AM'],
        ['example', '2024-01-01T12:00:00+05:30'],
        ['example', 'not a date'],
        ['click', 1, False], ['delete'],
    ],
}]
if __name__ == '__main__':
    import json
    import sys
    import tempfile
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path = os.path.join(work, 'dates.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
            with open(path) as stream:
                result = json.load(stream)
        with open(recorder.OUT, 'w') as stream:
            json.dump(result, stream, indent=1, ensure_ascii=False)
            stream.write('\n')
        print(f'wrote {recorder.OUT}: {len(result["cases"])} cases')
