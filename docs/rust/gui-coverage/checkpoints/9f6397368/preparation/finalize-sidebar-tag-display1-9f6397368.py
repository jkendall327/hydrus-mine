import copy, hashlib, json
from pathlib import Path
BASE=Path('/workspace/validation-reviews')
SOURCE='9f6397368d2ecdfcb28e7c7159a8bf195e7d7012'
RUN=37577340514  # Finalizer still requires completed successful exact-source evidence.
EXPECTED_ARTIFACT_ID=11463522766
ROOT_INSPECTED_IMAGES=['sidebar-sort-cog-root.png', 'sidebar-tag-display-1.png', 'sidebar-tag-display-3.png', 'sidebar-tag-display-2.png']
ROOT_REVIEWED_NATIVE_ANCHOR_IDS=[]
assert EXPECTED_ARTIFACT_ID is not None and ROOT_INSPECTED_IMAGES
OUT=BASE/'publication-sidebar-tag-display1-9f6397368-linux'
def read(p):return json.loads(p.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(n,v):(OUT/n).write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n')
mapping=read(BASE/'sidebar-tag-display1/prep-9f6397368/conditional-status-map.json')
original_path=Path(mapping['input_patch'])
assert sha(original_path)==mapping['input_sha256']
original=read(original_path);packet=copy.deepcopy(original)
nodes={n['id']:n for n in packet['reference']['updates']}
assert len(nodes)==1 and set(nodes)==set(mapping['selected_ids'])
ci_path=BASE/'ci-9f6397368/ci-evidence.json';ci=read(ci_path)
assert ci['source_commit']==SOURCE and ci['run_id']==RUN and ci['conclusion']=='success'
assert ci['validation_scope']=='linux' and all(j['conclusion']=='success' for j in ci['jobs'])
claims={};reviews=[]
for number in [2]:
 p=BASE/f'final-review-{number}-9f6397368.json';v=read(p)
 assert p.with_suffix('.md').exists()
 assert v['source_commit']==SOURCE and v['run_id']==RUN and v['artifact_id']==EXPECTED_ARTIFACT_ID
 assert v['validation_scope']=='linux' or v['validation_scope'].startswith('Linux only;')
 for c in v['claims']:
  assert c['approval'] is True and c['id'] in nodes and c['id'] not in claims
  claims[c['id']]=(c,p)
 reviews.append({'path':f'docs/rust/gui-coverage/checkpoints/9f6397368/{p.name}','sha256':sha(p)})
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
 n['implementation_validation'].update({'source_commit':SOURCE,'ci':'passed','tests_executed_at_checkpoint':True,'native_render_review':'passed','review_required':False,'run_url':ci['url'],'validation_scope':'linux','required_platforms':['linux'],'deferred_platforms':['windows','macos'],'independent_review_evidence':{'path':f'docs/rust/gui-coverage/checkpoints/9f6397368/{p.name}','sha256':sha(p)}})
 limits=c.get('caveats',[]);assert isinstance(limits,list) and all(isinstance(s,str)for s in limits)
 added[key]=[s for s in limits if s not in n['limitations']];n['limitations'].extend(added[key])
packet['method']='Publish exactly one original leaf after exact-source full Linux CI and fresh independent behavioral/rendered review. Preserve prior354 IDs and assessments, native inventory and all unselected assessments except exact source anchor remaps and ancestor counts. Windows/macOS deferred; consumer and capture limits retained.'
assert packet['native']==original['native']
preflight=read(BASE/'sidebar-tag-display1/prep-9f6397368/subset-preflight/subset-source-preflight.json')
digest=hashlib.sha256(json.dumps(packet,sort_keys=True,ensure_ascii=False).encode()).hexdigest()
OUT.mkdir(exist_ok=True)
write('selected-patch.json',packet)
write('status-reconciliation-audit.json',{'source_commit':SOURCE,'ci_evidence_sha256':sha(ci_path),'mapping_sha256':sha(BASE/'sidebar-tag-display1/prep-9f6397368/conditional-status-map.json'),'mapping_applied_after_gates':True,'operations':mapping['conditional_operations'],'added_review_scope_limits':added,'reviews':reviews,'selected_ids':list(nodes),'prior354_completion_ids_and_assessments_preserved':True,'source_only_native_anchor_ids':ROOT_REVIEWED_NATIVE_ANCHOR_IDS,'final_patch_digest':digest})
write('review.json',{'source_commit':SOURCE,'reviewed_by':'Codex root with independent exact-source behavioral/rendered review2; separate source/preservation checks','patch_sha256':digest,'reviewed_source_census_sha256':preflight['full_source_census_digest_for_later_review'],'selected_completion_ids':list(nodes),'reviewed_assessment_ids':list(nodes),'reviewed_native_ids':ROOT_REVIEWED_NATIVE_ANCHOR_IDS,'independent_reviews':reviews,'validation_scope':'linux','required_platforms':['linux'],'deferred_platforms':['windows','macos'],'root_actual_native_inspection':ROOT_INSPECTED_IMAGES,'scope':'Only the selected sidebar namespace tag-display action gains credit. No parent, alias, native hierarchy or deferred-platform credit. Browser verification and preservation audit required before canonical publication.'})
print(json.dumps({'selected':len(nodes),'patch_digest':digest,'output':str(OUT)}))
