import copy, hashlib, json
from pathlib import Path
BASE=Path('/workspace/validation-reviews')
SOURCE='1e2b2e25fd89b24253f4387f71115e675fdb657b'
RUN=37605060836  # Finalizer still requires completed successful exact-source evidence.
EXPECTED_ARTIFACT_ID=None
ROOT_INSPECTED_IMAGES=[]  # Six fresh defining frames remain pending.
ROOT_REVIEWED_NATIVE_ANCHOR_IDS=[]
assert EXPECTED_ARTIFACT_ID is not None and ROOT_INSPECTED_IMAGES
OUT=BASE/'publication-file-call-queue3-1e2b2e25f-linux'
def read(p):return json.loads(p.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(n,v):(OUT/n).write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n')
mapping=read(BASE/'file-call-queue3/prep-1e2b2e25f/conditional-status-map.json')
original_path=Path(mapping['input_patch'])
assert sha(original_path)==mapping['input_sha256']
original=read(original_path);packet=copy.deepcopy(original)
nodes={n['id']:n for n in packet['reference']['updates']}
assert len(nodes)==3 and set(nodes)==set(mapping['selected_ids'])
ci_path=BASE/'ci-1e2b2e25f/ci-evidence.json';ci=read(ci_path)
assert ci['source_commit']==SOURCE and ci['run_id']==RUN and ci['conclusion']=='success'
assert ci['validation_scope']=='linux' and all(j['conclusion']=='success' for j in ci['jobs'])
claims={};reviews=[]
for number in [2]:
 p=BASE/f'final-review-{number}-1e2b2e25f.json';v=read(p)
 assert p.with_suffix('.md').exists()
 assert v['source_commit']==SOURCE and v['run_id']==RUN and v['artifact_id']==EXPECTED_ARTIFACT_ID
 assert v['validation_scope']=='linux' or v['validation_scope'].startswith('Linux only;')
 for c in v['claims']:
  assert c['approval'] is True and c['id'] in nodes and c['id'] not in claims
  claims[c['id']]=(c,p)
 reviews.append({'path':f'docs/rust/gui-coverage/checkpoints/1e2b2e25f/{p.name}','sha256':sha(p)})
assert set(claims)==set(nodes)
for op in sorted(mapping['conditional_operations'],key=lambda x:(x['id'],x['field'],-x.get('original_index',-1))):
 n,f=nodes[op['id']],op['field']
 if op['operation']=='replace_exact_field':
  assert n[f]==op['before'];n[f]=op['conditional_after']
 elif op['operation']=='replace_exact_substring':
  assert n[f].count(op['before'])==1;n[f]=n[f].replace(op['before'],op['conditional_after'])
 else:
  i=op['original_index'];assert n[f][i]==op['before']
  if op['operation']=='remove_exact_list_item':del n[f][i]
  elif op['operation']=='replace_exact_list_item':n[f][i]=op['conditional_after']
  else:raise ValueError(op)
added={}
for key,n in nodes.items():
 c,p=claims[key]
 n['implementation_validation'].update({'source_commit':SOURCE,'ci':'passed','tests_executed_at_checkpoint':True,'native_render_review':'passed','review_required':False,'run_url':ci['url'],'validation_scope':'linux','required_platforms':['linux'],'deferred_platforms':['windows','macos'],'independent_review_evidence':{'path':f'docs/rust/gui-coverage/checkpoints/1e2b2e25f/{p.name}','sha256':sha(p)}})
 limits=c.get('caveats',[]);assert isinstance(limits,list) and all(isinstance(s,str)for s in limits)
 added[key]=[s for s in limits if s not in n['limitations']];n['limitations'].extend(added[key])
packet['method']='Publish exactly three original leaves after exact-source full Linux CI and fresh independent behavioral/rendered review. Preserve prior 362 IDs and assessments, native inventory and all unselected assessments except exact source anchor remaps and ancestor counts. Windows/macOS deferred; consumer and capture limits retained.'
assert packet['native']==original['native']
preflight=read(BASE/'file-call-queue3/prep-1e2b2e25f/subset-preflight/subset-source-preflight.json')
digest=hashlib.sha256(json.dumps(packet,sort_keys=True,ensure_ascii=False).encode()).hexdigest()
OUT.mkdir(exist_ok=True)
write('selected-patch.json',packet)
write('status-reconciliation-audit.json',{'source_commit':SOURCE,'ci_evidence_sha256':sha(ci_path),'mapping_sha256':sha(BASE/'file-call-queue3/prep-1e2b2e25f/conditional-status-map.json'),'mapping_applied_after_gates':True,'operations':mapping['conditional_operations'],'added_review_scope_limits':added,'reviews':reviews,'selected_ids':list(nodes),'prior362_completion_ids_and_assessments_preserved':True,'source_only_native_anchor_ids':ROOT_REVIEWED_NATIVE_ANCHOR_IDS,'final_patch_digest':digest})
write('review.json',{'source_commit':SOURCE,'reviewed_by':'Codex root with independent exact-source behavioral/rendered review2; separate source/preservation checks','patch_sha256':digest,'reviewed_source_census_sha256':preflight['full_source_census_digest_for_later_review'],'selected_completion_ids':list(nodes),'reviewed_assessment_ids':list(nodes),'reviewed_native_ids':ROOT_REVIEWED_NATIVE_ANCHOR_IDS,'independent_reviews':reviews,'validation_scope':'linux','required_platforms':['linux'],'deferred_platforms':['windows','macos'],'root_actual_native_inspection':ROOT_INSPECTED_IMAGES,'scope':'Only the selected nested File Choose, Add/Edit and Order controls gain credit. No parent, alias, native hierarchy or deferred-platform credit. Browser verification and preservation audit required before canonical publication.'})
print(json.dumps({'selected':len(nodes),'patch_digest':digest,'output':str(OUT)}))
