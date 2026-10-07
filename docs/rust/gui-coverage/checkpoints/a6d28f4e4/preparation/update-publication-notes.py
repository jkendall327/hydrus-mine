"""Root-only guarded updater after accepted365/green/final3/copy/browser.
Prepared and syntax checked only; all exact replacements planned before writes.
Performance observation remains root-owned.
"""
import json,subprocess
from pathlib import Path
R=Path('/workspace/hydrus-mine');B=Path('/workspace/validation-reviews');D=R/'docs/rust/gui-coverage'
S='a6d28f4e498a3d7a8f00ad6a61da62166bc54383';RUN=37613614274;CP='a6d28f4e4'
IDS={'audit-options-open-externally-single-file-calls-add','audit-options-open-externally-single-file-calls-edit','audit-options-open-externally-single-file-calls-delete'}
read=lambda p:json.loads(p.read_text())
progress=read(D/'overnight/progress.json');review=read(B/f'final-review-2-{CP}.json');out=read(B/f'ci-{CP}/validation-outcome.json');audit=read(D/f'checkpoints/{CP}/preservation-and-copy-audit.json');browser=read(D/f'checkpoints/{CP}/browser/browser-check.json')
assert progress['source_commit']==review['source_commit']==out['source_commit']==audit['source_commit']==browser['source_commit']==S
assert review['run_id']==out['run_id']==audit['run_id']==RUN and subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==S
assert progress['concrete_implementation_completions']==len(progress['completed_feature_ids'])==368 and set(progress['additional_completed_feature_ids'])==IDS and progress['additional_concrete_implementation_completions']==3
assert len(review['claims'])==3 and {c['id']for c in review['claims']if c['approval']is True}==IDS and out['conclusion']=='success'
assert all(out[k]['failed']==out[k]['ignored']==0 for k in ('gui_tests','media_tests'))
assert audit['prior365_nonanchor_nodes_preserved']is True and audit['unselected_assessments_limits_metadata_and_topology_preserved']is True and audit['all_goal1274_and_original_topology_preserved']is True and audit['current_assertions']=={'open_externally':360,'menu_choice_wheel':92}
assert browser['status']=='passed' and browser['expected_signed_off']==368 and browser['expected_additional']==3 and browser['prior_signed_off']==365 and set(browser['additional_selected_ids'])==IDS
gui,media=out['gui_tests']['passed'],out['media_tests']['passed'];planned={}
def replace(path,before,after):
 text=planned.get(path,(R/path).read_text());assert text.count(before)==1,(path,before);planned[path]=text.replace(before,after,1)
def section(path,start,end,body):
 text=planned.get(path,(R/path).read_text());assert text.count(start)==text.count(end)==1;lo=text.index(start);hi=text.index(end,lo);planned[path]=text[:lo]+body+text[hi:]
replace('docs/rust/GUI.md','Six defining native captures and full Linux validation remain pending; no\nadditional completion is banked by this source change.',f'Six defining native captures passed full Linux run `{RUN}` and fresh\nindependent rendered review. Exactly MIME mapping Add/Edit/Delete are banked:\n368 signed off, 7 existing candidates pending. All {gui} GUI and {media} media\ntests passed. [Evidence](gui-coverage/checkpoints/{CP}/README.md).')
replace('docs/rust/DIFFERENCES.md','Full Linux execution and fresh independent rendered review remain pending.',f'Full Linux run `{RUN}` and fresh independent rendered review passed for\nexactly MIME mapping Add/Edit/Delete.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).')
replace('docs/rust/GUI.md','and exact selection assertions remain. Fresh full Linux validation is pending.',f'and exact selection assertions remain. Repaired full Linux run `{RUN}` passed;\nfailed-run evidence is retained separately. This resized test does not establish\nunresized hide/show dispatch or the precise missed-event cause.')
replace('docs/rust/DIFFERENCES.md','All prior behavior assertions remain; a new full Linux run is required.',f'All prior behavior assertions remain; repaired full Linux run `{RUN}` passed.\nThe failed source and diagnostic evidence remain separate and earn no credit.')
path='docs/rust/gui-coverage/validation-pass/README.md';section(path,'The latest checkpoint validates','The notes below preserve',f'''The latest checkpoint validates `{CP}` and publishes **368 signed-off
original leaves: 3 new in this batch, 128 since the prior 240 across twenty
recent checkpoints, 7 existing candidates still pending**. The 128 figure is
cumulative checkpoint progress, not a last-24-hour sign-off count. Full Linux
validation passed with all {gui} GUI and {media} media tests passing. Exactly
outer MIME mapping Add/Edit/Delete received fresh independent behavioral
and rendered review. Windows/macOS remain deferred.
[Durable checkpoint evidence](../checkpoints/{CP}/README.md) retains six fresh
defining frames, raw logs and source/assertion/reference/preservation proofs.
The goal remains all individual leaves implemented and verified; numeric
checkpoints and inventory assessments are not stopping criteria. Continue
with the next bounded candidate batch after publication.
''');replace(path,'One hundred and twenty-five are now published; 10 remain unresolved.','One hundred and twenty-eight are now published; 7 remain unresolved.')
path='docs/rust/gui-coverage/all-leaves-goal/README.md';replace(path,'At the `1e2b2e25f` validation checkpoint, 365 have explicit implementation\nsign-off and 909 remain outside that ledger. The 10 existing proposed candidates',f'At the `{CP}` validation checkpoint, 368 have explicit implementation\nsign-off and 906 remain outside that ledger. The 7 existing proposed candidates');replace(path,'URL Add/Edit and nested File Choose/Add/Edit/Order without changing the\nexhaustive goal IDs or exclusions.','URL Add/Edit, nested File Choose/Add/Edit/Order and outer MIME mapping\nAdd/Edit/Delete without changing the exhaustive goal IDs or exclusions.')
goal=read(D/'all-leaves-goal/checklist.json');before=goal['leaves'];ids=[r['id']for r in before];done=set(progress['completed_feature_ids']);assert len(ids)==len(set(ids))==1274 and not goal['historical_verification_approvals'] and done<=set(ids)
assert goal['explicit_implementation_signoffs']==365 and goal['remaining_without_explicit_signoff']==909
goal.update(validated_implementation_source=S,explicit_implementation_signoffs=368,remaining_without_explicit_signoff=906,previous_checkpoint_commit='da53daa67f7fb88870ae653f61def7cfbe514242',checkpoint_evidence=f'../checkpoints/{CP}/README.md')
changed=[]
for row in goal['leaves']:
 after=row['id']in done
 if row['signed_off_in_implementation_ledger']!=after:changed.append(row['id'])
 row['signed_off_in_implementation_ledger']=after
assert set(changed)==IDS and [r['id']for r in goal['leaves']]==ids and sum(r['signed_off_in_implementation_ledger']for r in goal['leaves'])==368
planned['docs/rust/gui-coverage/all-leaves-goal/checklist.json']=json.dumps(goal,indent=2,ensure_ascii=False)+'\n'
section('docs/rust/ROADMAP.md','progress reports, not stopping targets.','\nThe first parallel slate',f'''progress reports, not stopping targets. The latest checkpoint banks 368 original
concrete feature completions, including 128 since the prior 240 across twenty
recent Linux checkpoints. This cumulative count is not a last-24-hour sign-off
count. There are 7 existing candidates awaiting sign-off. Keep these counts
separate from inventory and historical first-pass assessments. Validate small
batches routinely and reassess any batch that goes 24 hours without a checkpoint.
Broad feature work must not rebuild an unvalidated backlog. Windows/macOS remain
deferred; strict Linux linting, full tests, reference replay and rendered review
remain required.
The latest checkpoint validates outer MIME mapping Add/Edit/Delete. The exact
outer Remove all selected? prompt preserves GeneralFile protection and nested
count-based questions. Recorded Add/Edit/Cancel/Delete states and full saved
keys/Manager are checked after Add event 9, reopened Edit event 11 and Delete
event 13. All 249 prior assertions and six tests remain; the file now has 360
assertions across seven tests. All {gui} GUI and {media} media tests passed. Six
fresh defining captures use explicit supported sizes and callback routes;
matching Qt child pixels, universal geometry and broader ownership are excluded.
[Evidence](gui-coverage/checkpoints/{CP}/README.md).
''')
inv=read(D/'reference-inventory.json');counts={k:sum(r['status']==k for r in inv['nodes'])for k in ('missing','partial','first_pass')}
replace('docs/rust/ROADMAP.md','now has 461 Missing, 385 Partial and 966 First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 365-item signed-off completion count.',f"now has {counts['missing']} Missing, {counts['partial']} Partial and {counts['first_pass']} First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 368-item signed-off completion count.")
planned[f'docs/rust/gui-coverage/checkpoints/{CP}/README.md']=f'''# Validated outer MIME mapping controls

Validated source `{S}`, full Linux run
[{RUN}](https://github.com/jkendall327/hydrus-mine/actions/runs/{RUN}).
Exactly MIME mapping Add/Edit/Delete gain sign-off: **368 signed off, 7 existing
candidates pending**. All {gui} GUI and {media} media tests passed. Prior 365,
unselected assessments, native 1,799 and all 1,274 goal IDs remain preserved.
All 249 prior assertions and six tests remain; the file now has 360 assertions
across seven tests. The original production change fixes the outer MIME Delete
prompt to Remove all selected?; nested count-based questions remain unchanged.
Failed source `8ada16c76`, run `37609745626`, had 708 passing GUI tests and two
failures, and none of the six selected MIME frames existed. These diagnostic
logs, archive metadata and source reviews grant no credit. The repaired MIME
setup distinguishes static/animated GIF by typed fixture code; all 358 failed
assertions remain and the current file has 360 across seven tests. The wheel
readiness repair retains all 92 assertions, its two-second deadline and one
physical wheel/exact result after a real 1101-to-1100 resize. Its missed-event
cause and reliable unresized hide/show dispatch remain unproven. No additional
wheel completion or production repair is claimed.
Selected Delete source now cites the actual guarded button at Python line 138,
replacing unrelated call-washing prose at 175. Add/Edit retain lines 136/137.

Root and independent reviewer2 inspected six fresh defining frames: MIME chooser,
blank new calls child, existing Edit calls child, saved/reopened mapping, exact
outer Delete question and staged deletion. Chooser is 520x440, calls children
580x340, outer table 960x800 and question 520x200. Full MIME choice order is
asserted independently while the list scrolls; not every option is simultaneously
visible. Callback entry/answers do not prove physical control coordinates,
universal geometry or OS focus/modality. Historical Qt PNG is later outer-panel
context, not a matched chooser/blank-child/question pixel or decoration pair.

Actual Add event 9 is saved and reopened before Edit event 11 is saved/reopened;
Delete event 13 is independently saved/reopened. Full Routing/Manager compares
callable keys and all other fields directly, not labels alone. Declined deletion
and parent Cancel preserve complete draft/Store. Standalone or mixed GeneralFile
selection blocks the entire delete. Accepted outer removal is staged until
Options Apply. Historical reference Delete records Yes; No is separately grounded
in the shared simple-delete accepted-only branch.

Retained genuinely pending outer MIME chooser and captured-MIME question callbacks
cannot affect usable replacement owners; current positive answers still work.
Hidden admission and exact current slot/draft/Store identities are scoped to these
cases. Automatic Main/final Bound destruction, arbitrary page changes, all-MIME
exhaustion, inherited keyboard/column behavior, deeper process/default launches,
parents/aliases/native hierarchy and Windows/macOS receive no added credit.

Desktop/narrow browser checks passed and root inspected both. Exact source,
reference/assertion/capture proofs, raw CI logs, final review, six fresh images
and preservation audit are retained here. Historical source-only/preparation
gates remain clearly historical. The unchanged goal has 906 leaves outside
explicit sign-off. Cumulative 128 since prior 240 across twenty checkpoints is
not a last-24-hour count. Routine performance observations remain separate.
'''
# Every exact replacement and guard passed before any canonical write.
for path,text in planned.items():(R/path).write_text(text)
print('Updated accepted outer MIME mapping notes; performance remains root-owned.')
