from pathlib import Path
import json,shutil
p=Path('/workspace/validation-reviews/or-emoji');stage=p/'staged';packet=p/'packet-draft'
assert json.loads((p/'preservation.json').read_text())['passed']
assert json.loads((p/'browser/browser-check.json').read_text())['passed']
shutil.copy2(p/'preservation.json',packet/'preservation.json')
shutil.copy2(stage/'publication-summary.json',packet/'publication-summary.json')
shutil.copytree(p/'browser',packet/'browser',dirs_exist_ok=True)
(packet/'browser/root-browser-review.json').write_text(json.dumps({'reviewer':'root','source_commit':'88e9851e6ab8ecec08eb03f0d5cd3a2f8b7d2339','passed':True,'directly_inspected':['report-desktop.png','report-narrow.png'],'observations':['375 signed-off total and selected OR editor are visible.','Assessment, limits and source links remain readable at desktop and narrow widths.','No horizontal overflow, page errors or external requests.']},indent=2)+'\n')
shutil.copytree(stage/'docs','docs',dirs_exist_ok=True)
shutil.copytree(packet,'docs/rust/gui-coverage/checkpoints/88e9851e6',dirs_exist_ok=False)
path=Path('docs/rust/ROADMAP.md');s=path.read_text();a=s.index("The owner's current goal");b=s.index('The first parallel slate',a)
s=s[:a]+'''The owner's current goal (2026-10-06) is **all individual report feature leaves
implemented, verified and published**, with Linux first. Numeric checkpoints are
progress reports, not stopping targets. The latest checkpoint banks 375 original
concrete feature completions, including 135 since the prior 240 across twenty-five
recent Linux checkpoints. This cumulative count is not a last-24-hour sign-off
count. No existing candidate remains awaiting sign-off; 899 of the 1,274 goal
leaves remain outside the explicit completion ledger. Keep these counts separate
from inventory and historical first-pass assessments. Validate small batches
routinely and reassess any batch that goes 24 hours without a checkpoint.
Broad feature work must not rebuild an unvalidated backlog. Windows/macOS remain
deferred; strict Linux linting, full tests, reference replay and rendered review
remain required.
The latest checkpoint validates the OR connecting-string editor, including the
previously blank saved fox. Startup now supplies monochrome outlines in the
actual text-control fallback chain while preserving preceding platform fonts.
Raw saved text, Cancel/reopen, legacy import, ownership and existing literal OR
consumers retain their behavior. Strict Clippy and all 2,198 workspace tests
passed, including 714 GUI and 67 media tests. Fresh Qt replay, six native frames
and independent review passed. The ASCII frame and three ordinary Latin/CJK/editor
frames are unchanged; the fox changes only within its field. Only this original
editor leaf gains credit; broader OR layout/rendering remains separately assessed.
[Evidence](gui-coverage/checkpoints/88e9851e6/README.md).

'''+s[b:]
s=s.replace('now has 452 Missing, 385 Partial and 975 First pass entries','now has 451 Missing, 385 Partial and 976 First pass entries').replace('374-item signed-off completion count','375-item signed-off completion count')
s=s.replace('Continuous source work now implements 134 further original leaf proposals over','The earlier continuous source slate proposed 134 further original leaves over')
s=s.replace('`python3 scripts/gui_burndown.py --commit HEAD`; do not substitute its proposed\ntotal of 374 for the validated 240 ledger.','`python3 scripts/gui_burndown.py --commit HEAD`; proposal counts are historical\nand must not replace the latest validated ledger above.')
path.write_text(s)
def replace(path,old,new):
 p=Path(path);s=p.read_text();assert s.count(old)==1,(path,s.count(old));p.write_text(s.replace(old,new))
replace('docs/rust/GUI.md','''Ordinary Latin/CJK glyph choices and existing literal OR
consumers retain their behavior. The regression requires visible ink inside the
fox field; exact-source hosted execution and fresh rendered review remain pending.''','''Ordinary Latin/CJK glyph choices and existing literal OR consumers retain their
behavior. Full Linux validation at `88e9851e6` and independent fresh rendered
review pass: 2,198 workspace tests, including 714 GUI and 67 media tests. The
actual fox field paints visibly; its ASCII counterpart and three ordinary text
frames are unchanged. This signs off only the original editor leaf, bringing the
ledger to 375 with no existing candidate pending.
[Evidence](gui-coverage/checkpoints/88e9851e6/README.md).''')
replace('docs/rust/DIFFERENCES.md','''labels, member/header colours and default/collapsed copy output. This proposes
only the original editor control; broader OR list layout and renderer families
remain Partial. Native rendered regression and Rust tests are authored for hosted
execution; no local Cargo builds, Rust tests or mutation runs were performed.''','''labels, member/header colours and default/collapsed copy output. This signs off
only the original editor control; broader OR list layout and renderer families
remain Partial. The original author packet's pending validation is superseded by
the exact-source Linux checkpoint below.''')
replace('docs/rust/DIFFERENCES.md','''outrank the fallback. Emoji are
monochrome and need not match Qt's glyph
shape or colour.''','''outrank the fallback. Emoji are monochrome and need not match Qt's glyph shape
or colour.''')
replace('docs/rust/DIFFERENCES.md','''byte-identical. Exact-source Linux execution and fresh frame review remain gates
for this single control, with no parent credit or custom OR renderer activation.''','''byte-identical. Full Linux validation and independent fresh image review pass at
`88e9851e6`: all 2,198 workspace tests, including 714 GUI and 67 media tests.
The stronger font-selection test rejects the earlier Emoji-only adapter; its
failed full run and blank field remain diagnostic history. The local cached-library
probe is separate from current application validation. Exactly one original leaf
is published, bringing the ledger to 375 with no existing candidate pending;
there is no parent credit or custom OR renderer activation. Windows/macOS remain
deferred. [Evidence](gui-coverage/checkpoints/88e9851e6/README.md).''')
print('Copied validated publication and updated current narrative.')
