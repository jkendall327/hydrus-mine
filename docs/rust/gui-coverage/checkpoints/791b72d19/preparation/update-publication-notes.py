"""Root-only notes updater after actual accepted362/green/final3/copy/browser.
Prepared and syntax checked only. Plan exact replacements before any writes.
"""
import json,subprocess
from pathlib import Path
R=Path('/workspace/hydrus-mine');B=Path('/workspace/validation-reviews');D=R/'docs/rust/gui-coverage';S='791b72d194d93e637aeeb0a243c81a9981cf41aa';RUN=37599290504;CP='791b72d19';IDS={'audit-options-external-programs-external-calls-add-defaults','audit-options-open-externally-url-calls-add','audit-options-open-externally-url-calls-edit'}
read=lambda p:json.loads(p.read_text());progress=read(D/'overnight/progress.json');review=read(B/f'final-review-2-{CP}.json');out=read(B/f'ci-{CP}/validation-outcome.json');audit=read(D/f'checkpoints/{CP}/preservation-and-copy-audit.json');browser=read(D/f'checkpoints/{CP}/browser/browser-check.json')
assert progress['source_commit']==review['source_commit']==out['source_commit']==audit['source_commit']==browser['source_commit']==S and review['run_id']==out['run_id']==audit['run_id']==RUN
assert progress['concrete_implementation_completions']==len(progress['completed_feature_ids'])==362 and set(progress['additional_completed_feature_ids'])==IDS and progress['additional_concrete_implementation_completions']==3
assert len(review['claims'])==3 and {c['id']for c in review['claims']if c['approval']is True}==IDS and out['conclusion']=='success'
assert all(out[k]['failed']==out[k]['ignored']==0 for k in ('gui_tests','media_tests'))
assert audit['prior359_nonanchor_nodes_preserved']is True and audit['unselected_assessments_limits_metadata_and_topology_preserved']is True and audit['all_goal1274_and_original_topology_preserved']is True and audit['current_assertions']=={'external_calls':313,'open_externally':156}
assert browser['status']=='passed' and browser['expected_signed_off']==362 and browser['expected_additional']==3 and browser['prior_signed_off']==359 and set(browser['additional_selected_ids'])==IDS
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==S
gui,media=out['gui_tests']['passed'],out['media_tests']['passed'];planned={}
def replace(path,before,after):
 text=planned.get(path,(R/path).read_text());assert text.count(before)==1,(path,before);planned[path]=text.replace(before,after,1)
def section(path,start,end,body):
 text=planned.get(path,(R/path).read_text());assert text.count(start)==text.count(end)==1;lo=text.index(start);hi=text.index(end,lo);planned[path]=text[:lo]+body+text[hi:]
replace('docs/rust/GUI.md','temporary hide/re-show and a positive current answer. These additions await\nfull Linux validation and fresh rendered review; no new completion is banked.',f'temporary hide/re-show and a positive current answer. Full Linux run `{RUN}`\nand fresh independent rendered review passed for exactly URL Add/Edit.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).')
replace('docs/rust/GUI.md','retired selector against a usable replacement. Six new native captures and the\nnew focused test await full Linux validation and fresh rendered review.',f'retired selector against a usable replacement. Full Linux run `{RUN}` and\nindependent review of six fresh defining frames passed for Add Defaults.\nTogether with URL Add/Edit, this checkpoint publishes 362 signed off, with\n13 existing candidates pending; all {gui} GUI and {media} media tests passed.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).')
replace('docs/rust/DIFFERENCES.md','notice image. Six new native states and stronger full-state ownership/persistence\nassertions await hosted Linux validation and independent fresh-image review.',f'notice image. Full Linux run `{RUN}` and independent review of six fresh\nURL defining states passed, retaining full-state ownership/persistence assertions.')
replace('docs/rust/DIFFERENCES.md','comparison. New defining captures and full-state regressions remain pending\nhosted Linux validation and independent rendered review.',f'comparison. Full Linux run `{RUN}` and independent review of six fresh\nAdd Defaults frames passed, retaining the full-state regressions.\n[Evidence](gui-coverage/checkpoints/{CP}/README.md).')
path='docs/rust/gui-coverage/validation-pass/README.md';section(path,'The latest checkpoint validates','The notes below preserve',f'''The latest checkpoint validates `{CP}` and publishes **362 signed-off
original leaves: 3 new in this batch, 122 since the prior 240 across eighteen
recent checkpoints, 13 existing candidates still pending**. The 122 figure is
cumulative checkpoint progress, not a last-24-hour sign-off count. Full Linux
validation passed with all {gui} GUI and {media} media tests passing. Exactly
registered-call Add Defaults and URL Add/Edit received fresh independent
behavioral and rendered review. Windows/macOS remain deferred.
[Durable checkpoint evidence](../checkpoints/{CP}/README.md) retains twelve new
defining frames, raw logs and complete source/assertion/reference/preservation
proofs. The goal remains all individual leaves implemented and verified;
numeric checkpoints and inventory assessments are not stopping criteria.
Continue with the next bounded candidate batch after publication.
''');replace(path,'One hundred and nineteen are now published; 16 remain unresolved.','One hundred and twenty-two are now published; 13 remain unresolved.')
path='docs/rust/gui-coverage/all-leaves-goal/README.md';replace(path,'At the `1943b19d9` validation checkpoint, 359 have explicit implementation\nsign-off and 915 remain outside that ledger. The 16 existing proposed candidates\nare a subset of',f'At the `{CP}` validation checkpoint, 362 have explicit implementation\nsign-off and 912 remain outside that ledger. The 13 existing proposed candidates\nare a subset of');replace(path,'command-editor controls plus registered-call Delete/Duplicate without changing\nthe exhaustive goal IDs or exclusions.','command-editor controls, registered-call Delete/Duplicate and Add Defaults, plus\nURL Add/Edit without changing the exhaustive goal IDs or exclusions.')
goal=read(D/'all-leaves-goal/checklist.json');ids=[r['id']for r in goal['leaves']];done=set(progress['completed_feature_ids']);assert len(ids)==len(set(ids))==1274 and not goal['historical_verification_approvals'] and done<=set(ids)
goal.update(validated_implementation_source=S,explicit_implementation_signoffs=362,remaining_without_explicit_signoff=912,previous_checkpoint_commit='b52957c9f8843c4765acc81baa00c63cff63244e',checkpoint_evidence=f'../checkpoints/{CP}/README.md')
for row in goal['leaves']:row['signed_off_in_implementation_ledger']=row['id']in done
assert [r['id']for r in goal['leaves']]==ids and sum(r['signed_off_in_implementation_ledger']for r in goal['leaves'])==362;planned['docs/rust/gui-coverage/all-leaves-goal/checklist.json']=json.dumps(goal,indent=2,ensure_ascii=False)+'\n'
section('docs/rust/ROADMAP.md','progress reports, not stopping targets.','\nThe first parallel slate',f'''progress reports, not stopping targets. The latest checkpoint banks 362 original
concrete feature completions, including 122 since the prior 240 across eighteen
recent Linux checkpoints. This cumulative count is not a last-24-hour sign-off
count. There are 13 existing candidates awaiting sign-off. Keep these counts
separate from inventory and historical first-pass assessments. Validate small
batches routinely and reassess any batch that goes 24 hours without a checkpoint.
Broad feature work must not rebuild an unvalidated backlog. Windows/macOS remain
deferred; strict Linux linting, full tests, reference replay and rendered review
remain required.
The latest checkpoint validates registered-call Add Defaults and URL Add/Edit.
Twelve fresh defining frames supplement exact factory/choice/order/store replays,
finite physical parent/menu admission and retained-owner checks. All 242 prior
external-call and 107 routing assertions remain; current files have 313 assertions
across ten tests and 156 across five respectively. All {gui} GUI and {media} media
tests passed. Supported dimensions, callback-driven child answers, absent matching
Qt child/popup pixels and broader ownership/launch limits remain explicit.
[Evidence](gui-coverage/checkpoints/{CP}/README.md).
''')
inv=read(D/'reference-inventory.json');counts={k:sum(r['status']==k for r in inv['nodes'])for k in ('missing','partial','first_pass')};replace('docs/rust/ROADMAP.md','now has 467 Missing, 385 Partial and 960 First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 359-item signed-off completion count.',f"now has {counts['missing']} Missing, {counts['partial']} Partial and {counts['first_pass']} First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 362-item signed-off completion count.")
planned[f'docs/rust/gui-coverage/checkpoints/{CP}/README.md']=f'''# Validated Add Defaults and URL Add/Edit

Validated source `{S}`, full Linux run
[{RUN}](https://github.com/jkendall327/hydrus-mine/actions/runs/{RUN}).
Exactly three original leaves gain sign-off: **362 signed off, 13 existing
candidates pending**. All {gui} GUI and {media} media tests passed. Prior 359,
unselected assessments, native 1,799 and all 1,274 goal IDs remain preserved.
All 242 prior external-call and 107 routing assertions remain. The current files
have 313 assertions/ten tests and 156/five respectively. Production, Slint,
models, reference recorder/fixture inputs and historical manifests are unchanged.
Selected URL reference anchors now cite actual URL QueueListBox callbacks at
OpenExternallyPanel.py116/117, replacing mistaken MIME registration136/137 only.
URL Edit replaces the callable at the selected queue position; retained queue-row
position does not imply keeping the replaced callable's stable key.

Root and independent reviewer2 inspected all twelve fresh defining frames:
defaults popup, custom platform question, initial/selected fifteen-choice selector,
all-default draft and saved reopening; two-choice Add, one-choice Add, one-choice
Edit, exhausted Information, appended URL draft and saved URL reopening. Existing
routing image is additional context and earns no extra leaf credit. Actual visual
findings, supported bounds and framework differences remain in the final review.

Physical Add Defaults menu admission and keyboard menu choice are finite to the
standard unscrolled/no-search1100x800 scale1 view. Physical URL parent Add/Edit
admission is finite to960x800 scale1; accepting current Edit replaces rather
than appends. Child answers/toggles/acceptance use callbacks; no physical chooser
or selector button acceptance, universal/default geometry or OS focus claim.
Native selectors highlight row booleans rather than Qt checkbox pixels. Scrollable
long tables and elided command cells are backed by separate full typed values,
not claims that every complete raw row is visible simultaneously.

The historical actual Qt defaults replay at6b matches default_routes and
three-platform factories without normalization; its raw JSON differs only at
26 generated key leaves plus two decoded-export keys, interpreter matches and
no temporary-path fields differ. The unchanged Qt callable-child and later routing
panel PNGs provide context, with no matching popup/selector/chooser/notice pixels,
placement, decoration or typography parity. Reference launch transport is captured,
not a new OS launcher or external network execution claim.

Defaults retirement covers ordinary Cancel/native close/slot cancellation and
parent Cancel against a usable successor. URL tests also cover child/parent
hide/re-show and positive current action. Complete saved Manager/Routing and
visible draft/selection/flags/slot identities remain guarded. Automatic Main or
final Bound destruction, arbitrary page switching and general ownership receive
no credit. Add/Edit/Delete/Duplicate/import/export beyond selected actions, MIME/
nested File/default-route launching, parents/aliases/native hierarchy, process
output and Windows/macOS remain outside this checkpoint. Historical failed-run
evidence remains preserved in prior immutable checkpoints and earns no credit.

Desktop/narrow report checks passed and root inspected both. Exact CI logs,
independent review, twelve fresh frames, source/capture/assertion proofs, reference
provenance, final patch, artifact hashes and preservation audit are retained here.
The unchanged exhaustive goal has912 leaves outside explicit sign-off. Cumulative
122 since prior240 across eighteen checkpoints is not a last-24-hour count.
'''
for path,text in planned.items():
 for a,b in [('Panel.py116/117','Panel.py 116/117'),('registration136/137','registration 136/137'),('no-search1100x800 scale1','no search at 1100x800, scale 1'),('to960x800 scale1','to 960x800, scale 1'),('at6b','at 6b'),('has912','has 912'),('prior240','prior 240')]:text=text.replace(a,b)
 planned[path]=text
# All lookups and exact before strings passed before the first write.
for path,text in planned.items():(R/path).write_text(text)
print('Updated accepted three-leaf notes; performance observation remains root-owned.')
