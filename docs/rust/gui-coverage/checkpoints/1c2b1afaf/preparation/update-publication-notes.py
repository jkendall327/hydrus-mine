"""Root-only guarded selected1 notes update; inert until actual accepted gates.
All replacements are prepared before canonical writes. Performance remains root-owned.
"""
import json,subprocess
from pathlib import Path
R=Path('/workspace/hydrus-mine');B=Path('/workspace/validation-reviews');D=R/'docs/rust/gui-coverage';S='1c2b1afaf0fff7bf7bc984d17c5623e9e70098a5';RUN=37624387284;CP='1c2b1afaf';ID='audit-media-context-missing-clear-deleted'
rd=lambda p:json.loads(p.read_text())
p=rd(D/'overnight/progress.json');v=rd(B/f'final-review-2-{CP}.json');o=rd(B/f'ci-{CP}/validation-outcome.json');a=rd(D/f'checkpoints/{CP}/preservation-and-copy-audit.json');w=rd(D/f'checkpoints/{CP}/browser/browser-check.json')
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==S
assert p['source_commit']==v['source_commit']==o['source_commit']==a['source_commit']==w['source_commit']==S
assert v['run_id']==o['run_id']==a['run_id']==RUN and o['conclusion']=='success'
assert p['concrete_implementation_completions']==len(p['completed_feature_ids'])==370 and p['additional_concrete_implementation_completions']==1 and set(p['additional_completed_feature_ids'])=={ID}
assert len(v['claims'])==1 and v['claims'][0]['id']==ID and v['claims'][0]['approval'] is True
assert all(o[k]['failed']==o[k]['ignored']==0 for k in ['gui_tests','media_tests'])
assert a['prior369_nonanchor_nodes_preserved'] and a['unselected_assessments_limits_metadata_and_topology_preserved'] and a['all_goal1274_and_original_topology_preserved'] and a['current_assertions']=={'selected_deletion_records':82,'selected_deletion_records_model':11}
assert w['status']=='passed' and w['expected_signed_off']==370 and w['expected_additional']==1 and w['prior_signed_off']==369 and set(w['additional_selected_ids'])=={ID}
assert a['historical_images_counted_as_fresh'] is False
planned={};gui=o['gui_tests']['passed'];media=o['media_tests']['passed']
def replace(path,before,after):
 text=planned.get(path,(R/path).read_text());assert text.count(before)==1,(path,before);planned[path]=text.replace(before,after,1)
def section(path,start,end,body):
 text=planned.get(path,(R/path).read_text());assert text.count(start)==text.count(end)==1;lo=text.index(start);hi=text.index(end,lo);planned[path]=text[:lo]+body+text[hi:]
replace('docs/rust/GUI.md','unobserved same-turn replacement guarantee. Full exact-source Linux validation\nand fresh inspection of the new question capture remain pending. This covers\nthe selected thumbnail action only.',f'unobserved same-turn replacement guarantee. Full Linux run `{RUN}`\nand fresh independent inspection of the measured pointer-question controls\npassed. Exactly this selected thumbnail action gains sign-off: 370 signed,\n5 existing candidates pending. All {gui} GUI and {media} media tests passed.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).')
replace('docs/rust/DIFFERENCES.md','controls and capture await exact-source full Linux execution and fresh rendered\nreview; no selected completion is added by this source follow-up.',f'controls and capture passed full Linux run `{RUN}` and fresh independent\nrendered review. Only the selected thumbnail clear-deletion-record leaf gains\ncredit. [Evidence](gui-coverage/checkpoints/{CP}/README.md).')
section('docs/rust/gui-coverage/validation-pass/README.md','The latest checkpoint validates','The notes below preserve',f'''The latest checkpoint validates `{CP}` and publishes **370 signed-off
original leaves: 1 new in this batch, 130 since the prior 240 across twenty-two
recent checkpoints, 5 existing candidates still pending**. The 130 figure is
cumulative checkpoint progress, not a last-24-hour sign-off count. Full Linux
validation passed with all {gui} GUI and {media} media tests passing. Exactly
the selected thumbnail clear-deletion-record leaf received fresh independent
behavioral and rendered review. Windows/macOS remain deferred.
[Durable checkpoint evidence](../checkpoints/{CP}/README.md) retains the new
defining pointer-question frame, raw logs and source/assertion/reference proofs.
Historical keyboard-only frames and the superseded evidence-only readiness
recommendation remain distinct from the repaired-source approval.
The goal remains all individual leaves implemented and verified; numeric
checkpoints and inventory assessments are not stopping criteria.
''')
replace('docs/rust/gui-coverage/validation-pass/README.md','One hundred and twenty-nine are now published; 6 remain unresolved.','One hundred and thirty are now published; 5 remain unresolved.')
replace('docs/rust/gui-coverage/all-leaves-goal/README.md','At the `95bbbed2c` validation checkpoint, 369 have explicit implementation\nsign-off and 905 remain outside that ledger. The 6 existing proposed candidates',f'At the `{CP}` validation checkpoint, 370 have explicit implementation\nsign-off and 904 remain outside that ledger. The 5 existing proposed candidates')
replace('docs/rust/gui-coverage/all-leaves-goal/README.md','Add/Edit/Delete and live rating configuration examples without changing the\nexhaustive goal IDs or exclusions.','Add/Edit/Delete, live rating configuration examples and selected thumbnail\nclear-deletion records without changing the exhaustive goal IDs or exclusions.')
g=rd(D/'all-leaves-goal/checklist.json');ids=[r['id'] for r in g['leaves']];done=set(p['completed_feature_ids']);assert len(ids)==len(set(ids))==1274 and g['explicit_implementation_signoffs']==369 and g['remaining_without_explicit_signoff']==905 and not g['historical_verification_approvals']
g.update(validated_implementation_source=S,explicit_implementation_signoffs=370,remaining_without_explicit_signoff=904,previous_checkpoint_commit='74596132a9fff04b5d22e1a900bc4f3dc4bb7647',checkpoint_evidence=f'../checkpoints/{CP}/README.md');changed=[]
for r in g['leaves']:
 signed=r['id'] in done
 if r['signed_off_in_implementation_ledger']!=signed:changed.append(r['id'])
 r['signed_off_in_implementation_ledger']=signed
assert changed==[ID] and [r['id'] for r in g['leaves']]==ids and sum(r['signed_off_in_implementation_ledger'] for r in g['leaves'])==370
planned['docs/rust/gui-coverage/all-leaves-goal/checklist.json']=json.dumps(g,indent=2,ensure_ascii=False)+'\n'
section('docs/rust/ROADMAP.md','progress reports, not stopping targets.','\nThe first parallel slate',f'''progress reports, not stopping targets. The latest checkpoint banks 370 original
concrete feature completions, including 130 since the prior 240 across twenty-two
recent Linux checkpoints. This cumulative count is not a last-24-hour sign-off
count. There are 5 existing candidates awaiting sign-off. Keep these counts
separate from inventory and historical first-pass assessments. Validate small
batches routinely and reassess any batch that goes 24 hours without a checkpoint.
Broad feature work must not rebuild an unvalidated backlog. Windows/macOS remain
deferred; strict Linux linting, full tests, reference replay and rendered review
remain required.
The latest checkpoint validates the selected thumbnail clear-deletion-record
action's real Yes/No controls and leaf-specific Return/Escape. All 44 old GUI
assertions and three tests remain; current coverage is 82 assertions/four tests.
The model's 11 assertions/two tests remain unchanged. All {gui} GUI and {media}
media tests passed. One fresh question frame shows the measured controls.
Captured-record and physical-queue preservation, hidden refusal and processed
Escape retirement before a usable successor are proved within finite scope;
unobserved same-turn replacement, generic owner and Qt question pixel parity
remain excluded. [Evidence](gui-coverage/checkpoints/{CP}/README.md).
''')
inv=rd(D/'reference-inventory.json');counts={k:sum(n['status']==k for n in inv['nodes'])for k in ['missing','partial','first_pass']}
replace('docs/rust/ROADMAP.md','now has 457 Missing, 385 Partial and 970 First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 369-item signed-off completion count.',f"now has {counts['missing']} Missing, {counts['partial']} Partial and {counts['first_pass']} First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 370-item signed-off completion count.")
planned[f'docs/rust/gui-coverage/checkpoints/{CP}/README.md']=f'''# Validated selected thumbnail clear-deletion records

Validated source `{S}`, full Linux run
[{RUN}](https://github.com/jkendall327/hydrus-mine/actions/runs/{RUN}).
Exactly one selected thumbnail action gains sign-off: **370 signed off,
5 existing candidates pending**. All {gui} GUI and {media} media tests passed.
Prior 369 completions, unselected assessments, native 1,799 and all 1,274 goal
IDs remain preserved. Only the selected goal sign-off flag changes; 904 goal
leaves remain outside the explicit ledger.

The repaired Main question has real Yes/No controls and retains Enter/Escape.
Root and independent reviewer2 inspected the new 1100×700
`selected-deletion-records-pointer-question.png` at the actual retained adapter.
It shows the exact question and both measured controls after the captured files
and current selection diverge, before physical No. Real press/release No and Yes
and leaf-specific keys preserve full relevant deletion tables, captured eligible
identities, physical queue and unrelated membership. Hidden refusal and a usable
successor are covered. The held old press is released after processed actual
Escape press/release retires old controls; no unobserved same-turn atomic
replacement or generic question-ownership guarantee is made.

All 44 prior GUI assertions and three tests remain as an exact original prefix;
current coverage is 82 assertions across four tests. The model's 11 assertions
and two tests are unchanged, preserving captured eligibility and actual
later-batch failure committed-prefix behavior. The existing writer batches at
64 records without filesystem deletion or queue cancellation. Global clear,
viewer producers, parents, aliases and the separate Undelete confirmation
consumer receive no new credit.

The historical Qt recording and menu PNG remain unchanged. They ground exact
messages, selected eligibility, genuine menu dispatch and content-write behavior;
the image is not a paired question screenshot. No matching question pixel/modal
placement, universal geometry, OS or deferred-platform claim is made. The old
`selected_deletion_records_question.png` is supplemental only. Prior keyboard-only
CI/images remain in [the preceding checkpoint](../95bbbed2c/README.md), while the
superseded evidence-only readiness and pointer-gap reconciliation are retained
here separately. Cached exact Slint generation proves compilation of authored
UI only, not Rust execution or runtime.

Desktop/narrow browser checks passed and root inspected both. Source, actual
full CI logs, final review, fresh image and preservation audit are retained here.
Cumulative 130 since 240 across twenty-two checkpoints is not a last-24-hour
count. Routine performance observations are separate and root-owned.
'''
for path,text in planned.items():(R/path).write_text(text)
print('Updated accepted selected1 notes; performance remains root-owned.')
