import json,sys,subprocess
from pathlib import Path
source=sys.argv[1];stage=Path(sys.argv[2]);out=Path(sys.argv[3]);data='docs/rust/gui-coverage'
def read(path):return json.loads(subprocess.check_output(['git','show',source+':'+path]))
prior=read(data+'/reference-inventory.json');new=json.loads((stage/data/'reference-inventory.json').read_text())
assert prior['nodes']==new['nodes']
assert {k:v for k,v in prior.items() if k!='goal_verification'}=={k:v for k,v in new.items() if k!='goal_verification'}
assert not (stage/data/'native-inventory.json').exists()
assert not (stage/data/'overnight/progress.json').exists()
implementation=read(data+'/overnight/progress.json');summary=new['goal_verification'];ledger=json.loads((stage/data/'all-leaves-goal/verification-ledger.json').read_text());check=json.loads((stage/data/'all-leaves-goal/checklist.json').read_text())
old=read(data+'/all-leaves-goal/verification-ledger.json');assert ledger['batches'][:-1]==old['batches']
added={r['id'] for r in ledger['batches'][-1]['manifest']['leaves']}
assert len(added)==9 and summary['historical_verifications']==9
assert summary['implementation_completions']==implementation['concrete_implementation_completions']==376
assert set(summary['verified_feature_ids'])==set(implementation['completed_feature_ids'])|added
assert summary['verified_goal_leaves']==check['verified_goal_leaves']==385
assert sum(r['signed_off_in_implementation_ledger'] for r in check['leaves'])==376
assert {r['id'] for r in check['leaves'] if r['explicitly_verified_historical_leaf']}==added
out.write_text(json.dumps({'source_commit':source,'prior_implementation_signoffs_preserved':376,'new_historical_verifications':sorted(added),'verified_goal_leaves':385,'reference_nodes_unchanged':1812,'native_inventory_unchanged':True,'implementation_ledger_unchanged':True,'passed':True},indent=2)+'\n')
print('Preserved376implementation approvals; added9historical verifications; all inventories unchanged.')
