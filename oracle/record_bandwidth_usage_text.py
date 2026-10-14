#!/usr/bin/env python3
"""Record the reference's per-rule usage strings.

`BandwidthRules.GetBandwidthStringsAndGaugeTuples` (the strings the network
context bandwidth panel shows under "has its own rules", with its threshold
of 0) on real trackers with scripted usage at held times: data and request
rules over a minute, an hour, a day and the month, rules of zero allowed,
usage above the allowance, and no usage at all.

Usage: python oracle/record_bandwidth_usage_text.py
"""
import json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))

from hydrus.core import HydrusConstants as HC, HydrusTime
from hydrus.core.networking import HydrusNetworking

NOW = [0]
HydrusTime.GetNow = lambda: NOW[0]
HydrusTime.GetNowFloat = lambda: float(NOW[0])

T0 = 1767225600 + 15 * 86400 + 12 * 3600  # mid-month, noon, so "this month" is whole
D, R = HC.BANDWIDTH_TYPE_DATA, HC.BANDWIDTH_TYPE_REQUESTS
MB = 1024 ** 2

CASES = [
    {'name': 'day request rule, one used', 'rules': [[R, 86400, 10]], 'reports': [[100, R, 1]]},
    {'name': 'unsorted mixed rules', 'rules': [[D, None, 100 * MB], [D, 86400, 4096], [R, 3600, 20], [R, 60, 5], [D, 7200, 3 * MB]],
     'reports': [[200000, D, 5 * MB], [3000, D, 2048], [3000, R, 4], [30, D, 1000], [30, R, 2]]},
    {'name': 'zero allowed is skipped', 'rules': [[D, 86400, 0], [R, 86400, 3]], 'reports': [[10, R, 2]]},
    {'name': 'usage over the allowance', 'rules': [[R, 86400, 2], [D, 7200, MB]], 'reports': [[10, R, 7], [10, D, 3 * MB]]},
    {'name': 'nothing used', 'rules': [[D, 86400, 8 * 1024 * MB], [R, None, 1000]], 'reports': []},
    {'name': 'thousands of requests', 'rules': [[R, 86400, 1000]], 'reports': [[10, R, 1234]]},
]


def main():
    out = {'now': T0, 'cases': []}
    for case in CASES:
        tracker = HydrusNetworking.BandwidthTracker()
        rules = HydrusNetworking.BandwidthRules()
        for r in case['rules']:
            rules.AddRule(*r)
        for (ago, kind, n) in sorted(case['reports'], key=lambda r: -r[0]):
            NOW[0] = T0 - ago
            if kind == D:
                tracker.ReportDataUsed(n)
            else:
                tracker.ReportRequestUsed(n)
        NOW[0] = T0
        rows = rules.GetBandwidthStringsAndGaugeTuples(tracker, threshold=0)
        out['cases'].append(dict(case, rows=[[s, [v, r]] for (s, (v, r)) in rows]))
    with open(os.path.join(HERE, 'fixtures', 'bandwidth_usage_text.json'), 'w') as f:
        json.dump(out, f, indent=1)
        f.write('\n')
    for c in out['cases']:
        print(c['name'], c['rows'])


if __name__ == '__main__':
    main()
