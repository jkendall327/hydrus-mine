"""Retain GitHub API provenance and construct exact-source Linux CI evidence."""
import json
import subprocess
import sys
from pathlib import Path

BASE = Path(__file__).parent
ROOT = Path('/workspace/hydrus-mine')
run_id = int(sys.argv[1])
source = sys.argv[2]

def api(path):
    return json.loads(subprocess.check_output(['gh', 'api', path], cwd=ROOT))

run = api(f'repos/jkendall327/hydrus-mine/actions/runs/{run_id}')
jobs = api(f'repos/jkendall327/hydrus-mine/actions/runs/{run_id}/jobs?per_page=100')
artifacts = api(f'repos/jkendall327/hydrus-mine/actions/runs/{run_id}/artifacts?per_page=100')
for name, value in [('run-final', run), ('jobs-final', jobs), ('artifacts-final', artifacts)]:
    (BASE / f'{name}.json').write_text(json.dumps(value, indent=2) + '\n')
assert run['head_sha'] == source
assert run['status'] == 'completed' and run['conclusion'] == 'success'
ci = {key: run[key] for key in ['head_sha', 'status', 'conclusion', 'run_attempt',
                               'created_at', 'run_started_at', 'updated_at']}
ci.update(source_commit=source, run_id=run_id, url=run['html_url'],
          validation_scope='linux', required_platforms=['linux'],
          deferred_platforms=['windows', 'macos'],
          jobs=[j for j in jobs['jobs'] if j['name'] in ('check', 'parity-models')])
sys.path.insert(0, str(ROOT / 'scripts'))
import gui_publish
gui_publish.validate_ci(ci, source)
assert any(j['name'] == 'publication-safety' and j['conclusion'] == 'success' for j in jobs['jobs'])
(BASE / 'ci-evidence.json').write_text(json.dumps(ci, indent=2) + '\n')
print(json.dumps({'source': source, 'run': run_id, 'status': 'successful exact-source Linux validation'}))
