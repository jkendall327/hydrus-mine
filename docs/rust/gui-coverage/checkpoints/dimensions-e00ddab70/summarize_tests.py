"""Summarize actual Cargo result lines without inferring unexecuted tests."""
import json
import re
from pathlib import Path

base = Path(__file__).parent
text = re.sub(r'\x1b\[[0-9;]*m', '', (base / 'tests.log').read_text())
rows = []
target = None
for line in text.splitlines():
    if 'Running ' in line or line.strip().startswith('Doc-tests '):
        target = line.strip()
    match = re.search(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;', line)
    if match:
        rows.append(dict(target=target, result=match[1], passed=int(match[2]),
                         failed=int(match[3]), ignored=int(match[4])))
assert rows and all(r['result'] == 'ok' and r['failed'] == 0 for r in rows)
for name in ['dimensions_presets_pointer_acceptance_reaches_page_and_persistent_recent_history',
             'dimensions_presets_cancel_hidden_and_retired_callbacks_do_not_change_owner']:
    assert re.search(r'test [^\n]*' + name + r' \.\.\. ok', text), name
result = {'scope': 'Exact-source hosted Linux workspace test log',
          'passed': sum(r['passed'] for r in rows),
          'failed': sum(r['failed'] for r in rows),
          'ignored': sum(r['ignored'] for r in rows), 'suites': rows}
(base / 'test-summary.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({k: v for k, v in result.items() if k != 'suites'}))
