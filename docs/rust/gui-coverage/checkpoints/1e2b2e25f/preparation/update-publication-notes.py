"""Root-only guarded updater after accepted365/green/final3/copy/browser.
Prepared and syntax checked only; all exact replacements planned before writes.
Performance observation remains root-owned.
"""
import json,subprocess
from pathlib import Path
R=Path('/workspace/hydrus-mine');B=Path('/workspace/validation-reviews');D=R/'docs/rust/gui-coverage'
S='1e2b2e25fd89b24253f4387f71115e675fdb657b';RUN=37605060836;CP='1e2b2e25f'
IDS={'audit-options-nested-open-file-call-list-choose','audit-options-nested-open-file-call-list-add-edit','audit-options-nested-open-file-call-list-order'}
read=lambda p:json.loads(p.read_text())
progress=read(D/'overnight/progress.json');review=read(B/f'final-review-2-{CP}.json');out=read(B/f'ci-{CP}/validation-outcome.json');audit=read(D/f'checkpoints/{CP}/preservation-and-copy-audit.json');browser=read(D/f'checkpoints/{CP}/browser/browser-check.json')
assert progress['source_commit']==review['source_commit']==out['source_commit']==audit['source_commit']==browser['source_commit']==S
assert review['run_id']==out['run_id']==audit['run_id']==RUN and subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==S
assert progress['concrete_implementation_completions']==len(progress['completed_feature_ids'])==365 and set(progress['additional_completed_feature_ids'])==IDS and progress['additional_concrete_implementation_completions']==3
assert len(review['claims'])==3 and {c['id']for c in review['claims']if c['approval']is True}==IDS and out['conclusion']=='success'
assert all(out[k]['failed']==out[k]['ignored']==0 for k in ('gui_tests','media_tests'))
assert audit['prior362_nonanchor_nodes_preserved']is True and audit['unselected_assessments_limits_metadata_and_topology_preserved']is True and audit['all_goal1274_and_original_topology_preserved']is True and audit['current_assertions']=={'open_externally':249}
assert browser['status']=='passed' and browser['expected_signed_off']==365 and browser['expected_additional']==3 and browser['prior_signed_off']==362 and set(browser['additional_selected_ids'])==IDS
gui,media=out['gui_tests']['passed'],out['media_tests']['passed'];planned={}
def replace(path,before,after):
 text=planned.get(path,(R/path).read_text());assert text.count(before)==1,(path,before);planned[path]=text.replace(before,after,1)
def section(path,start,end,body):
 text=planned.get(path,(R/path).read_text());assert text.count(start)==text.count(end)==1;lo=text.index(start);hi=text.index(end,lo);planned[path]=text[:lo]+body+text[hi:]
replace('docs/rust/GUI.md','captures and the additional regressions await full Linux validation and fresh\nindependent rendered review; no additional completion is banked yet.',f'captures and the additional regressions passed full Linux run `{RUN}` and\nfresh independent rendered review. Exactly nested File Choose, Add/Edit and\nOrder are banked: 365 signed off, 10 existing candidates pending. All {gui} GUI\nand {media} media tests passed.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).')
replace('docs/rust/DIFFERENCES.md','the post-Apply consumer provide that evidence. The six pending native captures\nuse explicit queue/chooser/notice sizes; the historical Qt outer-panel image is\nnot a matching nested child image. Full Linux and fresh rendered review remain\npending. Outer MIME mapping controls, physical nested button coordinates,',f'the post-Apply consumer provide that evidence. Six fresh native captures\nuse explicit queue/chooser/notice sizes; the historical Qt outer-panel image is\nnot a matching nested child image. Full Linux run `{RUN}` and fresh independent\nrendered review passed for exactly three nested File controls.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).\nOuter MIME mapping controls, physical nested button coordinates,')
path='docs/rust/gui-coverage/validation-pass/README.md';section(path,'The latest checkpoint validates','The notes below preserve',f'''The latest checkpoint validates `{CP}` and publishes **365 signed-off
original leaves: 3 new in this batch, 125 since the prior 240 across nineteen
recent checkpoints, 10 existing candidates still pending**. The 125 figure is
cumulative checkpoint progress, not a last-24-hour sign-off count. Full Linux
validation passed with all {gui} GUI and {media} media tests passing. Exactly
nested File Choose, Add/Edit and Order received fresh independent behavioral
and rendered review. Windows/macOS remain deferred.
[Durable checkpoint evidence](../checkpoints/{CP}/README.md) retains six fresh
defining frames, raw logs and source/assertion/reference/preservation proofs.
The goal remains all individual leaves implemented and verified; numeric
checkpoints and inventory assessments are not stopping criteria. Continue
with the next bounded candidate batch after publication.
''');replace(path,'One hundred and twenty-two are now published; 13 remain unresolved.','One hundred and twenty-five are now published; 10 remain unresolved.')
path='docs/rust/gui-coverage/all-leaves-goal/README.md';replace(path,'At the `791b72d19` validation checkpoint, 362 have explicit implementation\nsign-off and 912 remain outside that ledger. The 13 existing proposed candidates',f'At the `{CP}` validation checkpoint, 365 have explicit implementation\nsign-off and 909 remain outside that ledger. The 10 existing proposed candidates');replace(path,'URL Add/Edit without changing the exhaustive goal IDs or exclusions.','URL Add/Edit and nested File Choose/Add/Edit/Order without changing the\nexhaustive goal IDs or exclusions.')
goal=read(D/'all-leaves-goal/checklist.json');before=goal['leaves'];ids=[r['id']for r in before];done=set(progress['completed_feature_ids']);assert len(ids)==len(set(ids))==1274 and not goal['historical_verification_approvals'] and done<=set(ids)
assert goal['explicit_implementation_signoffs']==362 and goal['remaining_without_explicit_signoff']==912
goal.update(validated_implementation_source=S,explicit_implementation_signoffs=365,remaining_without_explicit_signoff=909,previous_checkpoint_commit='18b3a665e6cb145c10efdc7d13c6da614308d926',checkpoint_evidence=f'../checkpoints/{CP}/README.md')
changed=[]
for row in goal['leaves']:
 after=row['id']in done
 if row['signed_off_in_implementation_ledger']!=after:changed.append(row['id'])
 row['signed_off_in_implementation_ledger']=after
assert set(changed)==IDS and [r['id']for r in goal['leaves']]==ids and sum(r['signed_off_in_implementation_ledger']for r in goal['leaves'])==365
planned['docs/rust/gui-coverage/all-leaves-goal/checklist.json']=json.dumps(goal,indent=2,ensure_ascii=False)+'\n'
section('docs/rust/ROADMAP.md','progress reports, not stopping targets.','\nThe first parallel slate',f'''progress reports, not stopping targets. The latest checkpoint banks 365 original
concrete feature completions, including 125 since the prior 240 across nineteen
recent Linux checkpoints. This cumulative count is not a last-24-hour sign-off
count. There are 10 existing candidates awaiting sign-off. Keep these counts
separate from inventory and historical first-pass assessments. Validate small
batches routinely and reassess any batch that goes 24 hours without a checkpoint.
Broad feature work must not rebuild an unvalidated backlog. Windows/macOS remain
deferred; strict Linux linting, full tests, reference replay and rendered review
remain required.
The latest checkpoint validates nested File Choose, Add/Edit and Order. Eight
recorded value states, action-derived selection, full saved callable keys and
post-Apply first-call Main consumption are checked. All 156 prior assertions and
five tests remain; the file now has 249 assertions across six tests. All {gui} GUI
and {media} media tests passed. Six fresh defining captures use supported explicit
sizes; callback routes do not establish physical nested-button geometry or
matching Qt child pixels. Broader ownership, parent and launch limits remain.
[Evidence](gui-coverage/checkpoints/{CP}/README.md).
''')
inv=read(D/'reference-inventory.json');counts={k:sum(r['status']==k for r in inv['nodes'])for k in ('missing','partial','first_pass')}
replace('docs/rust/ROADMAP.md','now has 464 Missing, 385 Partial and 963 First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 362-item signed-off completion count.',f"now has {counts['missing']} Missing, {counts['partial']} Partial and {counts['first_pass']} First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 365-item signed-off completion count.")
planned[f'docs/rust/gui-coverage/checkpoints/{CP}/README.md']=f'''# Validated nested File queue controls

Validated source `{S}`, full Linux run
[{RUN}](https://github.com/jkendall327/hydrus-mine/actions/runs/{RUN}).
Exactly Choose, Add/Edit and Order gain sign-off: **365 signed off, 10 existing
candidates pending**. All {gui} GUI and {media} media tests passed. Prior 362,
unselected assessments, native 1,799 and all 1,274 goal IDs remain preserved.
All 156 prior assertions and five tests remain; the file now has 249 assertions
across six tests. Only GUI test additions and honest notes changed in validated
source; production, Slint, models and historical manifests/recordings are intact.

Root and independent reviewer2 inspected six fresh defining frames: registered
File chooser, populated queue, explicit single-choice Edit, captured removal
question, reordered queue and exhausted Information. Queue frames are 580x340,
choosers 520x440 and notices 520x200. Actual visual findings remain in the final
review. Callback entry/answers are tested, not physical nested coordinates,
universal geometry, OS focus or matching nested Qt pixel parity. Historical Qt
PNG depicts a later outer routing panel, not these live children.

All eight recorded nested value states are replayed. Selection comes from
recorder actions and queue behavior because the fixture has no selection field.
Complete saved Routing/Manager comparisons establish callable keys directly;
displayed names alone do not. Edit replaces the callable at its selected queue
row while preserving that row identity/position. Down/Up changes the first
saved call used by the Main consumer after ordinary child/parent Apply and
reopening, without reseeding settings. Existing typed viewer/MIME precedence,
explicit-empty fallback and missing-first-call checks remain independent.

Retained old choice/question callbacks are rejected against usable current
successors; hidden child/window actions preserve complete drafts and current
positive actions still work. General Main/final Bound destruction, arbitrary
page changes and ownership guarantees are excluded. Outer MIME controls,
deeper process output, default-launch breadth, parents/aliases/native hierarchy,
Windows/macOS and unrelated actions receive no completion credit.

Desktop/narrow browser checks passed and root inspected both. Source/capture/
assertion/reference proofs, final patch, exact CI logs, fresh images and
preservation audit are durable here. Historical preparation/source-only gates
remain clearly historical; this accepted checkpoint supplies current results.
The unchanged goal has 909 leaves outside explicit sign-off. Cumulative 125
since prior 240 across nineteen checkpoints is not a last-24-hour count.
'''
# Every exact replacement and guard passed before any canonical write.
for path,text in planned.items():(R/path).write_text(text)
print('Updated accepted File queue three notes; performance remains root-owned.')
