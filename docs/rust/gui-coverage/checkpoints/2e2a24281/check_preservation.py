"""Audit staged publication against its immutable validated source."""
import json,sys,subprocess,hashlib
from pathlib import Path
source=sys.argv[1];stage=Path(sys.argv[2]);out=Path(sys.argv[3])
base='docs/rust/gui-coverage/'
selected='audit-media-menu-database-fix-missing-file-archived-times'
def old(path):return json.loads(subprocess.check_output(['git','show',source+':'+path]))
def new(path):return json.loads((stage/path).read_text())
def normalized(value):
 if isinstance(value,list):return [normalized(x) for x in value]
 if isinstance(value,dict):
  omitted={'descendant_status_counts'}
  if 'line' in value and ('path' in value or 'relativepath' in value):omitted|={'line','line_sha256','anchor_git_head'}
  return {k:normalized(v) for k,v in value.items() if k not in omitted}
 return value
counts={}
for kind in ['reference','native']:
 a={n['id']:n for n in old(base+kind+'-inventory.json')['nodes']}
 b={n['id']:n for n in new(base+kind+'-inventory.json')['nodes']}
 assert set(a)==set(b),kind
 unexpected=[key for key in a if not(kind=='reference' and key==selected) and normalized(a[key])!=normalized(b[key])]
 assert not unexpected,(kind,unexpected)
 counts[kind]=len(a)
p=old(base+'overnight/progress.json');q=new(base+'overnight/progress.json')
assert len(p['completed_feature_ids'])==373
assert set(q['completed_feature_ids'])-set(p['completed_feature_ids'])=={selected}
assert set(p['completed_feature_ids'])<=set(q['completed_feature_ids'])
assert q['concrete_implementation_completions']==374
report={'source_commit':source,'prior_signed_off':373,'new_signed_off':374,'added_ids':[selected],'inventory_counts':counts,'unselected_node_preservation':'Exact equality except pinned source-anchor metadata and derived descendant-status aggregates.','passed':True}
out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
