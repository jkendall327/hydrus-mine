"""Check completeness, not visual correctness, of this checkpoint's artifacts."""
from pathlib import Path
import hashlib, json, re, struct, sys

packet = Path(sys.argv[1])
source = '2e2a2428112bdeb17a9fb9642e7a43c2934d2325'
tests = re.sub(r'\x1b\[[0-9;]*m', '', (packet / 'tests.log').read_text())
names = [
    'actual_archive_controls_wrap_scroll_and_apply_each_recorded_population',
    'cancel_after_commit_reports_success_and_refreshes_while_cancel_before_write_rolls_back',
    'database_repair_has_owned',
]
for name in names:
    matches = [line for line in tests.splitlines() if 'archive_repair::' + name in line]
    assert matches and all(line.endswith(' ... ok') for line in matches), (name, matches)
summaries = re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored', tests)
assert summaries and all(state == 'ok' and failed == '0' for state, _, failed, _ in summaries)
assert any(passed == '713' for _, passed, _, _ in summaries), summaries
assert any(passed == '67' for _, passed, _, _ in summaries), summaries
expected = {f'archive-repair-{state}-{choice}.png': (680, 480)
            for state in ['warning', 'choices', 'done'] for choice in range(3)}
expected.update({
    'archive-repair-choices-narrow-top.png': (440, 480),
    'archive-repair-choices-short-top.png': (440, 360),
    'archive-repair-choices-short-scrolled.png': (440, 360),
    'archive-repair-choices-wide.png': (840, 640),
    'archive-repair-no-missing.png': (680, 480),
})
images = []
for name, size in expected.items():
    content = (packet / 'native' / name).read_bytes()
    assert content[:8] == b'\x89PNG\r\n\x1a\n', name
    measured = struct.unpack('>II', content[16:24])
    assert measured == size, (name, measured, size)
    images.append({'name': name, 'sha256': hashlib.sha256(content).hexdigest(),
                   'width': measured[0], 'height': measured[1]})
report = {'source_commit': source, 'run_id': 37682619326,
          'required_renders': images, 'test_summaries': summaries,
          'completeness_check_passed': True,
          'visual_review': 'Separate root and independent inspection required.'}
(packet / 'native-render-manifest.json').write_text(json.dumps(report, indent=2) + '\n')
print(f'Confirmed {len(images)} required fresh frames and passing archive regressions.')
