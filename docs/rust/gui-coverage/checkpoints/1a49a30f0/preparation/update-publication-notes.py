"""Root-only guarded notes; prepare all exact replacements before writes.
No performance attribution or new approval; actual accepted373 and gates required.
"""
import json,subprocess
from pathlib import Path
R=Path('/workspace/hydrus-mine');B=Path('/workspace/validation-reviews');D=R/'docs/rust/gui-coverage';P=Path(__file__).parent;S='1a49a30f058d47ea29cf6ea1f1dc919d14d9f243';RUN=37644009033;CP='1a49a30f0'
assert type(RUN) is int, 'Actual corrected-source CI dispatch is still unbound'
rd=lambda p:json.loads(p.read_text());selection=rd(P/'selection.json');ids=set(selection['selected_candidate_ids']);p=rd(D/'overnight/progress.json');v=rd(B/f'final-review-2-{CP}.json');o=rd(B/f'ci-{CP}/validation-outcome.json');a=rd(D/f'checkpoints/{CP}/preservation-and-copy-audit.json');w=rd(D/f'checkpoints/{CP}/browser/browser-check.json')
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==S
assert p['source_commit']==v['source_commit']==o['source_commit']==a['source_commit']==w['source_commit']==S
assert v['run_id']==o['run_id']==a['run_id']==RUN and o['conclusion']=='success'
assert p['concrete_implementation_completions']==len(p['completed_feature_ids'])==373 and p['additional_concrete_implementation_completions']==3 and set(p['additional_completed_feature_ids'])==ids
assert len(v['claims'])==3 and {c['id']for c in v['claims']}==ids and all(c['approval'] is True for c in v['claims'])
assert all(o[k]['failed']==o[k]['ignored']==0 for k in ['gui_tests','media_tests'])
assert a['prior370_nonanchor_nodes_preserved'] and a['unselected_assessments_limits_metadata_and_topology_preserved'] and a['all_goal1274_and_original_topology_preserved'] and a['current_assertions']=={'thumbnail_navigation':57,'thumbnail_navigation_model':19}
assert w['status']=='passed' and w['expected_signed_off']==373 and w['expected_additional']==3 and w['prior_signed_off']==370 and set(w['additional_selected_ids'])==ids
plan=rd(P/'notes-exact-replacement-plan.json');assert plan['source_commit']==S and plan['run_id']==RUN;gui=o['gui_tests']['passed'];media=o['media_tests']['passed'];planned={}
for edit in plan['replacements']:
 path=edit['path'];text=planned.get(path,(R/path).read_text());assert text.count(edit['before'])==1,(path,edit['before']);after=edit['conditional_after'].replace('{GUI_PASSED}',str(gui)).replace('{MEDIA_PASSED}',str(media)).replace('{RUN_ID}',str(RUN));planned[path]=text.replace(edit['before'],after,1)
g=rd(D/'all-leaves-goal/checklist.json');before=g['leaves'];original_ids=[r['id'] for r in before];assert len(original_ids)==len(set(original_ids))==1274 and g['explicit_implementation_signoffs']==370 and g['remaining_without_explicit_signoff']==904 and not g['historical_verification_approvals'];done=set(p['completed_feature_ids']);changed=[]
for r in g['leaves']:
 signed=r['id'] in done
 if signed!=r['signed_off_in_implementation_ledger']:changed.append(r['id'])
 r['signed_off_in_implementation_ledger']=signed
assert set(changed)==ids and len(changed)==3 and [r['id']for r in g['leaves']]==original_ids
assert sum(r['signed_off_in_implementation_ledger']for r in g['leaves'])==373
g.update(validated_implementation_source=S,explicit_implementation_signoffs=373,remaining_without_explicit_signoff=901,previous_checkpoint_commit='2e4912e688110d7b3ed6aad07aed0e72835d65d0',checkpoint_evidence=f'../checkpoints/{CP}/README.md');planned['docs/rust/gui-coverage/all-leaves-goal/checklist.json']=json.dumps(g,indent=2,ensure_ascii=False)+'\n'
inv=rd(D/'reference-inventory.json');counts={k:sum(n['status']==k for n in inv['nodes'])for k in ['missing','partial','first_pass']};path='docs/rust/ROADMAP.md';before='now has 456 Missing, 385 Partial and 971 First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 370-item signed-off completion count.';assert planned[path].count(before)==1;planned[path]=planned[path].replace(before,f"now has {counts['missing']} Missing, {counts['partial']} Partial and {counts['first_pass']} First pass entries; its status changes\nalso include parent/alias assessments, which do not inflate the 373-item signed-off completion count.")
planned[f'docs/rust/gui-coverage/checkpoints/{CP}/README.md']=plan['checkpoint_readme'].replace('{GUI_PASSED}',str(gui)).replace('{MEDIA_PASSED}',str(media)).replace('{RUN_ID}',str(RUN))
for path,text in planned.items():(R/path).write_text(text)
print('Accepted thumbnail-navigation3 notes updated; performance remains root-owned.')
