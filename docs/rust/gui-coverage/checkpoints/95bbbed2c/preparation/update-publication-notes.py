"""Root-only guarded one-leaf notes updater. Inert until accepted ledger and gates.
Every exact replacement is prepared before writes; performance remains root-owned.
"""
import json,subprocess
from pathlib import Path
R=Path('/workspace/hydrus-mine');B=Path('/workspace/validation-reviews');D=R/'docs/rust/gui-coverage'
S='95bbbed2cdd5f254a71330a00a2eb15f24b771a8';RUN=37618131717;CP='95bbbed2c';ID='audit-media-services-missing-rating-preview'
rd=lambda p:json.loads(p.read_text())
progress=rd(D/'overnight/progress.json');review=rd(B/f'final-review-2-{CP}.json');out=rd(B/f'ci-{CP}/validation-outcome.json');audit=rd(D/f'checkpoints/{CP}/preservation-and-copy-audit.json');browser=rd(D/f'checkpoints/{CP}/browser/browser-check.json')
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==S
assert progress['source_commit']==review['source_commit']==out['source_commit']==audit['source_commit']==browser['source_commit']==S
assert review['run_id']==out['run_id']==audit['run_id']==RUN and out['conclusion']=='success'
assert progress['concrete_implementation_completions']==len(progress['completed_feature_ids'])==369 and progress['additional_concrete_implementation_completions']==1 and set(progress['additional_completed_feature_ids'])=={ID}
assert len(review['claims'])==1 and review['claims'][0]['id']==ID and review['claims'][0]['approval'] is True
assert all(out[k]['failed']==out[k]['ignored']==0 for k in ['gui_tests','media_tests'])
assert audit['prior368_nonanchor_nodes_preserved'] and audit['unselected_assessments_limits_metadata_and_topology_preserved'] and audit['all_goal1274_and_original_topology_preserved'] and audit['current_assertions']=={'services_editor':156,'menu_choice_wheel':92}
assert browser['status']=='passed' and browser['expected_signed_off']==369 and browser['expected_additional']==1 and browser['prior_signed_off']==368 and set(browser['additional_selected_ids'])=={ID}
gui=out['gui_tests']['passed'];media=out['media_tests']['passed'];planned={}
def replace(path,before,after):
 text=planned.get(path,(R/path).read_text());assert text.count(before)==1,(path,before);planned[path]=text.replace(before,after,1)
def section(path,start,end,body):
 text=planned.get(path,(R/path).read_text());assert text.count(start)==text.count(end)==1;lo=text.index(start);hi=text.index(end,lo);planned[path]=text[:lo]+body+text[hi:]
replace('docs/rust/GUI.md','The next rating-preview validation adds separate captures of all four contexts','The rating-preview validation adds separate captures of all four contexts')
replace('docs/rust/GUI.md','Full Linux execution and fresh image inspection are pending, so this source\nfollow-up adds no completion yet.',f'Full Linux run `{RUN}` and fresh independent inspection of all four\nframes passed. Exactly the live rating examples leaf is banked: 369 signed off,\n6 existing candidates pending. All {gui} GUI and {media} media tests passed.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).')
replace('docs/rust/GUI.md','The next validation follow-up restores the original 1100×800 hide/show wheel','The validated follow-up restores the original 1100×800 hide/show wheel')
replace('docs/rust/GUI.md','rejection and the exact selected index. Fresh execution is pending; this checks\nrecovery between gestures, not immediate hide/show recovery.',f'rejection and the exact selected index. Full Linux run `{RUN}` passed;\nthis checks recovery between gestures, not immediate hide/show recovery. No\nadditional wheel completion is claimed.')
replace('docs/rust/DIFFERENCES.md','actual owned editor at 640×1000 and await full Linux execution and rendered\nreview.',f'actual owned editor at 640×1000. Full Linux run `{RUN}` and fresh\nindependent rendered review passed for this one leaf.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).')
replace('docs/rust/DIFFERENCES.md',"cause or immediate hide/show recovery; fresh Linux execution remains pending.",f'cause or immediate hide/show recovery. Full Linux run `{RUN}` passed\nthis finite after-expiry precondition; no new wheel completion is claimed.')
section('docs/rust/gui-coverage/validation-pass/README.md','The latest checkpoint validates','The notes below preserve',f'''The latest checkpoint validates `{CP}` and publishes **369 signed-off
original leaves: 1 new in this batch, 129 since the prior 240 across twenty-one
recent checkpoints, 6 existing candidates still pending**. The 129 figure is
cumulative checkpoint progress, not a last-24-hour sign-off count. Full Linux
validation passed with all {gui} GUI and {media} media tests passing. Exactly
the live rating configuration examples leaf received fresh independent
behavioral and rendered review. Windows/macOS remain deferred.
[Durable checkpoint evidence](../checkpoints/{CP}/README.md) retains four fresh
defining frames, raw logs and source/assertion/reference/preservation proofs.
The goal remains all individual leaves implemented and verified; numeric
checkpoints and inventory assessments are not stopping criteria. Continue
with the next bounded candidate batch after publication.
''')
replace('docs/rust/gui-coverage/validation-pass/README.md','One hundred and twenty-eight are now published; 7 remain unresolved.','One hundred and twenty-nine are now published; 6 remain unresolved.')
replace('docs/rust/gui-coverage/all-leaves-goal/README.md','At the `a6d28f4e4` validation checkpoint, 368 have explicit implementation\nsign-off and 906 remain outside that ledger. The 7 existing proposed candidates',f'At the `{CP}` validation checkpoint, 369 have explicit implementation\nsign-off and 905 remain outside that ledger. The 6 existing proposed candidates')
replace('docs/rust/gui-coverage/all-leaves-goal/README.md','Add/Edit/Delete without changing the exhaustive goal IDs or exclusions.','Add/Edit/Delete and live rating configuration examples without changing the\nexhaustive goal IDs or exclusions.')
goal=rd(D/'all-leaves-goal/checklist.json');old=goal['leaves'];ids=[r['id']for r in old];done=set(progress['completed_feature_ids']);assert len(ids)==len(set(ids))==1274 and goal['explicit_implementation_signoffs']==368 and goal['remaining_without_explicit_signoff']==906 and not goal['historical_verification_approvals'] and done<=set(ids)
goal.update(validated_implementation_source=S,explicit_implementation_signoffs=369,remaining_without_explicit_signoff=905,previous_checkpoint_commit='dfce4078c277e932be874b32d882ef07e0f8b694',checkpoint_evidence=f'../checkpoints/{CP}/README.md')
changed=[]
for row in goal['leaves']:
 after=row['id'] in done
 if row['signed_off_in_implementation_ledger']!=after:changed.append(row['id'])
 row['signed_off_in_implementation_ledger']=after
assert changed==[ID] and [r['id']for r in goal['leaves']]==ids and sum(r['signed_off_in_implementation_ledger']for r in goal['leaves'])==369
planned['docs/rust/gui-coverage/all-leaves-goal/checklist.json']=json.dumps(goal,indent=2,ensure_ascii=False)+'\n'
section('docs/rust/ROADMAP.md','progress reports, not stopping targets.','\nThe first parallel slate',f'''progress reports, not stopping targets. The latest checkpoint banks 369 original
concrete feature completions, including 129 since the prior 240 across twenty-one
recent Linux checkpoints. This cumulative count is not a last-24-hour sign-off
count. There are 6 existing candidates awaiting sign-off. Keep these counts
separate from inventory and historical first-pass assessments. Validate small
batches routinely and reassess any batch that goes 24 hours without a checkpoint.
Broad feature work must not rebuild an unvalidated backlog. Windows/macOS remain
deferred; strict Linux linting, full tests, reference replay and rendered review
remain required.
The latest checkpoint validates live rating configuration examples. Four fresh
frames show all four contexts for like, numerical and counter services plus the
inline counter input. All 139 prior GUI assertions and five tests remain; the
file now has 156 assertions across five tests. All {gui} GUI and {media} media
tests passed. Samples do not persist file ratings. Saved dimensions and current
native action states differ from Qt final stills; no pixel equality, live Preview
canvas, cross-edge mixed-button capture, parent or extra wheel credit is claimed.
[Evidence](gui-coverage/checkpoints/{CP}/README.md).
''')
inv=rd(D/'reference-inventory.json');counts={k:sum(n['status']==k for n in inv['nodes'])for k in ['missing','partial','first_pass']}
replace('docs/rust/ROADMAP.md','now has 458 Missing, 385 Partial and 969 First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 368-item signed-off completion count.',f"now has {counts['missing']} Missing, {counts['partial']} Partial and {counts['first_pass']} First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 369-item signed-off completion count.")
planned[f'docs/rust/gui-coverage/checkpoints/{CP}/README.md']=f'''# Validated live rating configuration examples

Validated source `{S}`, full Linux run
[{RUN}](https://github.com/jkendall327/hydrus-mine/actions/runs/{RUN}).
Exactly one live rating configuration examples leaf gains sign-off: **369 signed
off, 6 existing candidates pending**. All {gui} GUI and {media} media tests passed.
Prior 368, all unselected assessments, native 1,799 and all 1,274 goal IDs remain
preserved. All 139 prior services-editor assertions and five tests remain;
the file now has 156 assertions across five tests. Production, Slint and
reference inputs are unchanged.

Root and independent reviewer2 inspected four fresh 640×1000 frames: all four
contexts for like/dislike, numerical and counter services, plus the actual owned
inline counter input before its response. Thumbnails, Media Viewer, Preview
Window and Dialog (Default) labels, graphics and footer are visible. Each capture
retains the actual editor adapter and uses one ordinary ScrollView wheel with
bounded real timer/render/animation settling. Only the first control has public
geometry; complete four-context readability comes from actual image inspection,
not the getter. Finite supported coordinates do not prove universal geometry.

Native saved icon sizes 20/15/12/12 and existing action states differ from Qt
final stills. Native Thumbnail/Preview like samples remain selected; Qt's first
sample is cleared. Native counter Thumbnail/Preview are 1, Media Viewer 0 and
Dialog 12,345, whereas the Qt Thumbnail is 2. Numerical native Thumbnail/Preview
are 3/7 and the other samples -/7 with independently asserted colors/padding.
These demonstrate different finite states, not identical-state pixel parity.
The inline native counter input replaces the separate Qt modal child.

Existing staged/Apply/Cancel/reopen, sample-only persistence, real rating-count
isolation, retained-owner callbacks, one-star normalization and whole-widget
pointer/fraction/drag assertions remain. Preview Window is an example label,
not implementation of a Preview canvas. Mixed-button cross-edge capture, general
rating/service/rendering parents, SVG backend completeness, OS chrome, aliases
and deferred platforms receive no added credit.

The independent wheel follow-up retains all 92 assertions, original 1100×800,
two-second deadline and one physical wheel/exact result after at least 810 ms
of timer/layout pumping for Slint's source-backed 800 ms gesture filter to
expire. This finite after-timeout test does not establish immediate hide/show
recovery or the exact historical failure cause, and earns no extra wheel credit.
Historical failed-source diagnostics remain in
[the preceding checkpoint](../a6d28f4e4/README.md); current expiry/source proofs
are retained here without duplicating its failed-run archive.

Desktop/narrow browser checks passed and root inspected both. Exact source,
reference/assertion/capture proofs, raw CI logs, final review, four fresh images
and preservation audit are retained here. Historical preparation gates remain
clearly historical. The goal has 905 leaves outside explicit sign-off.
Cumulative 129 since prior 240 across twenty-one checkpoints is not a last-24-hour
count. Routine hosted performance observations remain separate.
'''
for path,text in planned.items():(R/path).write_text(text)
print('Updated accepted one-leaf rating notes; performance remains root-owned.')
