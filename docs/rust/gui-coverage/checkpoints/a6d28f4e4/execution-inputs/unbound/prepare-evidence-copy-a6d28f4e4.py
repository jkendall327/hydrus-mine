#!/usr/bin/env python3
"""Conditional source a6d28f4e4 mime-mappings3 preservation/evidence-copy preparation.
Default: check only. --copy-scratch writes a reviewable outside-repo copy tree.
Never publishes, writes canonical files, creates approvals, runs builds or uses network.
"""
import argparse,copy,hashlib,json,shutil,subprocess,tempfile,types
from pathlib import Path

BASE=Path('/workspace/validation-reviews').resolve()
REPO=Path('/workspace/hydrus-mine').resolve()
SOURCE='a6d28f4e498a3d7a8f00ad6a61da62166bc54383'
RUN=37613614274  # Actual dispatched run; result/artifact/render approval pending.
DATA='docs/rust/gui-coverage'
CHECKPOINT=DATA+'/checkpoints/a6d28f4e4'
FINAL=BASE/'publication-mime-mappings3-a6d28f4e4-linux'
PREP=BASE/'mime-mappings3/prep-a6d28f4e4'
EXPECTED_ARTIFACT=None  # Actual fresh green-source artifact pending.
EXPECTED_ZIP_SHA256=None  # Actual fresh archive fingerprint pending.
INPUT_HASHES={}


def require(v,message):
    if not v:raise ValueError(message)


def read(path):
    require(path.is_file() and not path.is_symlink(), 'Missing/nonregular evidence file: '+str(path))
    raw=path.read_bytes();fingerprint=hashlib.sha256(raw).hexdigest();require(str(path) not in INPUT_HASHES or INPUT_HASHES[str(path)]==fingerprint,'Evidence changed during validation: '+str(path));INPUT_HASHES[str(path)]=fingerprint
    return json.loads(raw)


def sha(path):
    result=hashlib.sha256(path.read_bytes()).hexdigest();require(str(path) not in INPUT_HASHES or INPUT_HASHES[str(path)]==result,'Evidence changed during validation/copy: '+str(path));INPUT_HASHES[str(path)]=result;return result


def git(*args):return subprocess.check_output(['git',*args],cwd=REPO)


def scratch(path):
    result=path.resolve()
    require(result.is_relative_to(BASE) and not result.is_relative_to(REPO),'Use validation-reviews scratch only: '+str(path))
    return result


def strings(value):
    if isinstance(value,str):yield value
    elif isinstance(value,list):
        for item in value:yield from strings(item)
    elif isinstance(value,dict):
        for item in value.values():yield from strings(item)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--final',type=Path,default=FINAL)
    parser.add_argument('--stage',type=Path,default=FINAL/'staged')
    parser.add_argument('--ci-root',type=Path,default=BASE/'ci-a6d28f4e4')
    parser.add_argument('--native-root',type=Path,default=BASE/'ci-a6d28f4e4/native-renders')
    parser.add_argument('--browser',type=Path,default=BASE/'report-browser-a6d28f4e4/browser-check.json',help='Actual passed browser-check.json for this exact staged report')
    parser.add_argument('--output',type=Path,default=FINAL/'copy-ready',help='New scratch directory; never repository destination')
    parser.add_argument('--copy-scratch',action='store_true',help='After all gates pass, atomically create outside-repo copy tree and preservation audit')
    args=parser.parse_args()
    final,stage,ciroot,native,browser,out=map(scratch,[args.final,args.stage,args.ci_root,args.native_root,args.browser,args.output])
    require(type(RUN) is int and type(EXPECTED_ARTIFACT) is int and isinstance(EXPECTED_ZIP_SHA256,str) and len(EXPECTED_ZIP_SHA256)==64,'Fresh artifact/archive placeholders are unbound; wait actual green CI and retrieved artifact')
    require(not out.exists(),'Refuse overwriting evidence-copy output: '+str(out))
    selection=read(PREP/'selection.json');selected=set(selection['selected_candidate_ids'])
    require(selection['source_commit']==SOURCE and selection['run_id']==RUN and len(selected)==3,'Selection source/3 differs')
    # Existence gates occur before loading source validators or writing anything.
    paths=[ciroot/'ci-evidence.json', final/'selected-patch.json', final/'review.json',final/'status-reconciliation-audit.json',browser,native/'native-render-manifest.json']
    values=[read(p) for p in paths];ci,patch,review,reconcile,web,manifest=values
    pub=types.ModuleType('copy_source_publication');pub.__file__=str(REPO/'scripts/gui_publish.py')
    exec(compile(git('show',SOURCE+':scripts/gui_publish.py'),pub.__file__,'exec'),pub.__dict__)
    validation=pub.validate_ci(ci,SOURCE)
    require(ci['source_commit']==SOURCE and ci['run_id']==RUN and ci['conclusion']=='success','CI exact-source/run/conclusion differs')
    require(validation['validation_scope']=='linux' and validation['required_platforms']==['linux'] and validation['deferred_platforms']==['windows','macos'],'Publication platform scope differs')
    normalized_ci=dict(ci,**validation)
    cv=pub.checkpoint(SOURCE);baseline,frozen,original,prior,source_claims=pub.inputs(SOURCE)
    prior_ids=set(prior['completed_feature_ids']);require(len(prior_ids)==prior['concrete_implementation_completions']==365 and not prior_ids&selected,'Prior365/selection differs')
    pub.validate_ci(prior['ci_evidence'],prior['source_commit'])
    goal=json.loads(git('show',SOURCE+':'+DATA+'/all-leaves-goal/checklist.json'))
    require(goal['distinct_terminal_leaves']==1274 and goal['explicit_implementation_signoffs']==365 and goal['remaining_without_explicit_signoff']==909,'Immutable all-leaves goal/baseline counts differ')
    require(patch['baseline_git_head']==review['source_commit']==reconcile['source_commit']==SOURCE,'Final source differs')
    require(pub.digest(patch)==review['patch_sha256']==reconcile['final_patch_digest'],'Final review/audit patch digests differ')
    edits=patch['reference']['updates'];require(len(edits)==3 and {r['id'] for r in edits}==selected,'Patch edits differ from selected3')
    require(not patch['reference'].get('additions') and not patch['reference'].get('deletions') and not patch['native'].get('additions') and not patch['native'].get('deletions'),'Unexpected reference/native edits')
    require(len(review['selected_completion_ids'])==len(review['reviewed_assessment_ids'])==3 and set(review['selected_completion_ids'])==set(review['reviewed_assessment_ids'])==selected and not review['reviewed_native_ids'],'Root review IDs differ')
    require(not patch['native']['updates'],'Unexpected native updates; this three-leaf packet has no explicit anchor corrections')
    require(str(review.get('reviewed_by','')).strip() and review.get('root_actual_native_inspection'),'Root actual inspection/reviewer is absent')
    require(manifest['source_commit']==SOURCE and manifest['run_id']==RUN and type(manifest['artifact_id']) is int and manifest['artifact_id']==EXPECTED_ARTIFACT and manifest['zip_sha256']==EXPECTED_ZIP_SHA256,'Fresh native manifest identity differs')
    image_rows=manifest['images'];images={r['name']:r for r in image_rows};require(len(images)==len(image_rows),'Duplicate native manifest images')
    checklist=read(PREP/'render-checklist.json');require(checklist['source_commit']==SOURCE and checklist['run_id']==RUN and set(checklist['selected_ids'])==selected,'Checklist exact source/run/selection differs')
    required_images=set(checklist['expected_native_capture_names']);require(len(required_images)==6 and required_images<=set(images),'Fresh manifest misses selected defining captures')
    copied_images=set();approvals={};reviews=[];historical_image_provenance=[]
    for number in [2]:
        path=BASE/f'final-review-{number}-a6d28f4e4.json';packet=read(path);md=path.with_suffix('.md');require(md.is_file(),'Review Markdown missing: '+str(md))
        require(packet['source_commit']==SOURCE and packet['run_id']==RUN and packet['artifact_id']==manifest['artifact_id'],'Independent review source/run/artifact differs: '+str(number))
        scope=packet['validation_scope'];require(scope=='linux' or scope.startswith('Linux only;'),'Independent review Linux scope differs')
        if 'native_zip_sha256' in packet:require(packet['native_zip_sha256']==manifest['zip_sha256'],'Review native archive hash differs')
        fresh_hashed=set();historical_images=[]
        def verify_image_hashes(value,context_source=SOURCE):
            if isinstance(value,dict):
                declared=value.get('source_commit',context_source)
                candidates=[value.get(k) for k in ('name','filename','path','file')]
                for candidate in candidates:
                    if not isinstance(candidate,str) or not candidate.endswith('.png') or 'sha256' not in value:continue
                    image_path=Path(candidate);name=image_path.name
                    if not image_path.is_absolute() and image_path.parts[:2]==('oracle','fixtures'):
                        require('..' not in image_path.parts and isinstance(declared,str) and len(declared)==40,'Unsafe committed reference image provenance')
                        reference=REPO/image_path
                        require(reference.resolve().is_relative_to((REPO/'oracle/fixtures').resolve()) and reference.is_file() and not reference.is_symlink() and sha(reference)==value['sha256'],'Committed reference image working bytes differ: '+candidate)
                        require(hashlib.sha256(git('show',declared+':'+str(image_path))).hexdigest()==value['sha256'] and hashlib.sha256(git('show',SOURCE+':'+str(image_path))).hexdigest()==value['sha256'],'Committed reference image historical/current source bytes differ: '+candidate)
                        if declared!=SOURCE:historical_images.append({'path':str(reference),'source_commit':declared,'sha256':value['sha256'],'unchanged_at_current_source':True})
                        continue
                    if declared!=SOURCE:
                        require(isinstance(declared,str) and len(declared)==40,'Historical image source is not explicit immutable SHA')
                        require(image_path.is_absolute() and image_path.is_file() and not image_path.is_symlink() and sha(image_path)==value['sha256'],'Historical reviewer image fingerprint differs: '+candidate)
                        historical_images.append({'path':candidate,'source_commit':declared,'sha256':value['sha256']})
                    elif image_path.is_absolute() and image_path.resolve().is_relative_to(REPO):
                        relative=image_path.resolve().relative_to(REPO)
                        require(image_path.is_file() and not image_path.is_symlink() and sha(image_path)==value['sha256'] and hashlib.sha256(git('show',SOURCE+':'+str(relative))).hexdigest()==value['sha256'],'Committed reference image differs: '+candidate)
                    elif name in images:
                        if image_path.is_absolute():require(image_path.resolve()==(native/name).resolve(),'Fresh native image path belongs to another checkpoint: '+candidate)
                        require(value['sha256']==images[name]['sha256'],'Fresh reviewer image fingerprint differs: '+candidate);fresh_hashed.add(name)
                for child in value.values():verify_image_hashes(child,declared)
            elif isinstance(value,list):
                for child in value:verify_image_hashes(child,context_source)
        verify_image_hashes(packet)
        historical_image_provenance.extend(historical_images)
        require(fresh_hashed,'Independent packet lacks source-bound hash records for fresh native images: '+str(number));copied_images|=fresh_hashed
        for claim in packet['claims']:
            key=claim['id'];require(key in selected and key not in approvals and claim['approval'] is True,'Unselected/duplicate/unapproved independent claim: '+key)
            approvals[key]=(claim,path)
        reviews.append((path,md))
    require(set(approvals)==selected,'Independent approvals do not cover exactly3')
    require(required_images<=copied_images,'Independent fresh image fingerprint records do not cover all selected defining captures')
    for record in review['independent_reviews']:
        evidence=BASE/Path(record['path']).name;require(sha(evidence)==record['sha256'],'Root independent-review evidence hash differs')
    require({Path(r['path']).name for r in review['independent_reviews']}=={p.name for p,_ in reviews},'Root reviewer packet set differs')
    root_images={Path(s).name for s in strings(review['root_actual_native_inspection']) if s.endswith('.png')};require(root_images and root_images<=set(images),'Root inspected image absent from fresh manifest');copied_images|=root_images
    require(copied_images,'No actual image evidence')
    for name in copied_images:
        require(Path(name).name==name,'Unsafe image name');path=native/name;require(path.is_file() and not path.is_symlink() and sha(path)==images[name]['sha256'] and path.stat().st_size==images[name]['bytes'],'Native image bytes differ: '+name)
    # Reproduce only conditional text operations + verbatim approving caveats.
    mapping=read(PREP/'conditional-status-map.json');require(mapping['source_commit']==SOURCE and mapping['run_id']==RUN and set(mapping['selected_ids'])==selected,'Mapping exact source/run/selection differs');initial=read(Path(mapping['input_patch']))
    require(sha(Path(mapping['input_patch']))==mapping['input_sha256'] and sha(PREP/'conditional-status-map.json')==reconcile['mapping_sha256'] and mapping['conditional_operations']==reconcile['operations'],'Reconciliation mapping provenance differs')
    require(reconcile['mapping_applied_after_gates'] is True and reconcile['ci_evidence_sha256']==sha(ciroot/'ci-evidence.json'),'Reconciliation actual gate/CI provenance differs')
    require(patch['native']==initial['native'],'Final native source-only patch differs from prepared input')
    require(len(mapping['conditional_operations'])==9 and sum(op['operation']=='remove_exact_list_item' for op in mapping['conditional_operations'])==6 and sum(op['operation']=='replace_exact_list_item' for op in mapping['conditional_operations'])==3,'Expected six pending-status removals and three exact historical provenance replacements')
    expected={r['id']:copy.deepcopy(r) for r in initial['reference']['updates']};final_nodes={r['id']:r for r in edits}
    for op in sorted(mapping['conditional_operations'],key=lambda x:(x['id'],x['field'],-x.get('original_index',-1))):
        row=expected[op['id']]
        if op['operation']=='replace_exact_field':
            require(row[op['field']]==op['before'],'Original scalar mapping text differs');row[op['field']]=op['conditional_after'];continue
        index=op['original_index'];require(row[op['field']][index]==op['before'],'Original mapping text differs')
        if op['operation']=='remove_exact_list_item':del row[op['field']][index]
        elif op['operation']=='replace_exact_list_item':row[op['field']][index]=op['conditional_after']
        else:raise ValueError('Unexpected mime-mappings3 mapping operation')
    for key,row in expected.items():
        claim,path=approvals[key];limits=claim.get('caveats',[])
        added=reconcile['added_review_scope_limits'][key];require(all(s in limits for s in added),'Added limit not in approving claim')
        row['limitations'].extend(added)
        for field,value in row.items():
            if field!='implementation_validation':require(value==final_nodes[key][field],'Final selected behavior/evidence scope changed: '+key+'/'+field)
        want_validation=copy.deepcopy(row['implementation_validation']);want_validation.update(source_commit=SOURCE,ci='passed',tests_executed_at_checkpoint=True,native_render_review='passed',review_required=False,run_url=ci['url'],validation_scope='linux',required_platforms=['linux'],deferred_platforms=['windows','macos'],independent_review_evidence={'path':CHECKPOINT+'/'+path.name,'sha256':sha(path)})
        require(final_nodes[key]['implementation_validation']==want_validation,'Final selected metadata differs from exact root recipe: '+key)
        v=final_nodes[key]['implementation_validation'];require(v['manifest']==row['implementation_validation']['manifest'] and v['source_commit']==SOURCE and v['ci']=='passed' and v['tests_executed_at_checkpoint'] is True and v['native_render_review']=='passed' and v['review_required'] is False,'Selected validation metadata differs')
        require(v['validation_scope']=='linux' and v['required_platforms']==['linux'] and v['deferred_platforms']==['windows','macos'],'Selected platform metadata differs')
        e=v['independent_review_evidence'];require(e['path']==CHECKPOINT+'/'+path.name and e['sha256']==sha(path),'Selected independent evidence differs')
    # Staged nodes must equal the exact-source patch/repin/aggregate pipeline.
    expected_inventory=copy.deepcopy(original);cv.apply_patches(expected_inventory,[pub.MemoryPatch(patch)])
    for side in ('reference','native'):
        expected_inventory[side],_=cv.repin(expected_inventory[side],SOURCE);cv.refresh_aggregates(expected_inventory[side],side)
    cv.refresh_mappings(expected_inventory)
    staged={side:read(stage/f'{DATA}/{side}-inventory.json') for side in ('reference','native')}
    deltas={};anchor_changes=[];ancestor_counts=[]
    for side in ('reference','native'):
        before={r['id']:r for r in original[side]['nodes']};after={r['id']:r for r in staged[side]['nodes']};want={r['id']:r for r in expected_inventory[side]['nodes']}
        # gui_publish merges only normalized passed metadata into selected rows.
        if side=='reference':
            for key in selected:want[key]['implementation_validation'].update(ci='passed',source_commit=SOURCE,run_url=ci['url'],tests_executed_at_checkpoint=True,**validation)
        require(after==want,'Staged nodes differ from exact source pipeline: '+side)
        require(set(before)==set(after) and len(after)==(1812 if side=='reference' else 1799),'Staged topology/denominator differs')
        cv.validate(staged[side],side)
        deltas[side]={key:sorted(k for k in set(row)|set(after[key]) if row.get(k)!=after[key].get(k)) for key,row in before.items() if row!=after[key]}
        if side=='reference':
            require(all(all(before[key].get(k)==after[key].get(k) for k in set(before[key])|set(after[key]) if k not in ('native_source','reference_source','evidence','native_wiring_source','descendant_status_counts')) for key in prior_ids),'Prior365 non-anchor complete objects changed')
        for key,old in before.items():
            if side=='reference' and key in selected:continue
            new=after[key]
            if old.get('native_wiring_source')!=new.get('native_wiring_source'):
                require(isinstance(old.get('native_wiring_source'),dict) and isinstance(new.get('native_wiring_source'),dict) and all(old['native_wiring_source'].get(f)==new['native_wiring_source'].get(f) for f in set(old['native_wiring_source'])|set(new['native_wiring_source']) if f not in ('line','line_sha256','anchor_git_head')),'Native wiring non-coordinate content changed: '+key)
            # Exact pipeline permits source line remapping and correct ancestor
            # descendant counts, never statuses/assessment/limits/ownership credit.
            require(all(old.get(k)==new.get(k) for k in set(old)|set(new) if k not in ('native_source','reference_source','evidence','native_wiring_source','descendant_status_counts')),'Unselected non-anchor object changed: '+side+'/'+key)
            if old.get('descendant_status_counts')!=new.get('descendant_status_counts'):ancestor_counts.append({'inventory':side,'id':key,'before':old['descendant_status_counts'],'after':new['descendant_status_counts']})
            if any(old.get(k)!=new.get(k) for k in ('native_source','reference_source','evidence','native_wiring_source')):anchor_changes.append({'inventory':side,'id':key,'fields':[k for k in ('native_source','reference_source','evidence','native_wiring_source') if old.get(k)!=new.get(k)],'proof':'Exact source pipeline equals staged whole node; inherited anchors remap using committed unchanged line/hash/context, never nearest guesses. No explicit authored semantic source substitution in this packet.'})
    progress=read(stage/f'{DATA}/overnight/progress.json');summary=read(stage/'publication-summary.json');stage_ci=read(stage/f'{DATA}/overnight/ci-evidence.json');audit=read(stage/f'{DATA}/audit/anchor-remap-{SOURCE[:8]}-publication.json')
    require(set(progress['completed_feature_ids'])==prior_ids|selected and len(progress['completed_feature_ids'])==progress['concrete_implementation_completions']==368 and set(progress['additional_completed_feature_ids'])==selected and progress['additional_concrete_implementation_completions']==3,'Staged completion ledger differs')
    require(progress['before']==prior['before'] and summary['prior_completions']==365 and summary['additional_completions']==3 and summary['validated_original_leaf_completions']==368 and summary['native_entries']==1799,'Staged summary/frozen baseline differs')
    require(pub.digest(stage_ci)==pub.digest(normalized_ci) and audit['input_fingerprints']['patch']==pub.digest(patch) and audit['input_fingerprints']['review']==pub.digest(review) and audit['input_fingerprints']['ci']==pub.digest(normalized_ci) and audit['input_fingerprints']['prior_ledger']==pub.digest(prior),'Staged CI/input fingerprints differ')
    census=read(stage/'source-census.json');require(pub.digest(census)==review['reviewed_source_census_sha256']==audit['input_fingerprints']['source_census'],'Staged reviewed source census differs')
    html=stage/'docs/rust/gui-progress.html';stage_audit=stage/f'{DATA}/audit/anchor-remap-{SOURCE[:8]}-publication.json'
    require(web['status']=='passed' and web['source_commit']==SOURCE and web['expected_signed_off']==368 and web['expected_additional']==3 and web['prior_signed_off']==365 and set(web['additional_selected_ids'])==selected,'Browser source/counts/selected scope differs')
    require(web['html_sha256']==sha(html) and web['publication_audit_sha256']==sha(stage_audit),'Browser reviewed a different HTML/audit')
    require(not web.get('js_errors') and not web.get('failed_requests') and not web.get('blocked_external_requests') and web['inventory_counts']=={'reference':1812,'native':1799},'Browser errors/inventory counts differ')
    require(web['publication_performed'] is False,'Browser packet asserts prior publication')
    screenshot_sources=[]
    for label in ('desktop','narrow'):
        shot=web['screenshots'][label];path=scratch(Path(shot['path']));require(path.is_file() and not path.is_symlink() and sha(path)==shot['sha256'],'Browser screenshot hash differs: '+label);screenshot_sources.append((path,label))
    # Only after every gate, assemble an optional scratch mirror for root review.
    files={}
    for relative in [f'{DATA}/reference-inventory.json',f'{DATA}/native-inventory.json',f'{DATA}/overnight/progress.json',f'{DATA}/overnight/reviewed-patch.json',f'{DATA}/overnight/ci-evidence.json',f'{DATA}/audit/anchor-remap-{SOURCE[:8]}-publication.json','docs/rust/gui-progress.html']:
        path=stage/relative;require(path.is_file(),'Staged payload missing: '+relative);files[relative]=path
    files['publication-summary.json']=stage/'publication-summary.json';files['source-census.json']=stage/'source-census.json'
    for name in ('selected-patch.json','review.json','status-reconciliation-audit.json'):files[CHECKPOINT+'/'+name]=final/name
    files[CHECKPOINT+'/ci-evidence.json']=ciroot/'ci-evidence.json';files[CHECKPOINT+'/native-render-manifest.json']=native/'native-render-manifest.json';files[CHECKPOINT+'/browser/browser-check.json']=browser
    for path,label in screenshot_sources:files[CHECKPOINT+'/browser/report-'+label+'.png']=path
    for path,md in reviews:files[CHECKPOINT+'/'+path.name]=path;files[CHECKPOINT+'/'+md.name]=md
    for name in copied_images:files[CHECKPOINT+'/native/'+name]=native/name
    for name in ('full-check.log','tests.log','parity.log','maintenance.log'):
        path=ciroot/name;require(path.is_file() and not path.is_symlink(),'Completed CI log missing: '+name);files[CHECKPOINT+'/'+name]=path
    root_render=read(ciroot/'root-render-review.json')
    require(root_render['source_commit']==SOURCE and root_render['run_id']==RUN and root_render['artifact_id']==EXPECTED_ARTIFACT and root_render['native_zip_sha256']==EXPECTED_ZIP_SHA256 and root_render['independent_review_pending'] is False,'Root render record identity or review gate differs')
    require(required_images<={r['name'] for r in root_render['actually_viewed']} and all(r['name'] in images and r['sha256']==images[r['name']]['sha256'] for r in root_render['actually_viewed']),'Root render record must cover all six required image fingerprints; supplemental images must also be fresh/hash-exact')
    files[CHECKPOINT+'/root-render-review.json']=ciroot/'root-render-review.json'
    for name in ('clippy.log','validation-outcome.json'):
        path=ciroot/name
        if path.is_file():
            if name.endswith('.json'):
                outcome=read(path);require(outcome['source_commit']==SOURCE and outcome['run_id']==RUN and outcome['conclusion']=='success','Optional outcome source/gate differs')
            files[CHECKPOINT+'/'+name]=path
    for name in ('selection.json','preparation1.json','conditional-status-map.json','prior365-preservation-proof.json','render-checklist.json','preparation-summary.json','scope-reconciliation-proof.json','source-anchor-proof.json','committed-source-review-provenance.json','binding-anchor-proof.json','inherited-wiring-anchor-proof.json','README.md'):
        path=PREP/name;require(path.is_file() and not path.is_symlink(),'Preparation proof missing: '+name);files[CHECKPOINT+'/preparation/'+name]=path
    for name in ('source-review-2.json','source-review-2.md','assertion-preservation-3.json','expected-captures-3.json','implementation-notes-3.md'):
        path=BASE/'mime-mappings3'/name;require(path.is_file()and not path.is_symlink(),'Exact source preparation missing: '+name);files[CHECKPOINT+'/preparation/'+name]=path
    files[CHECKPOINT+'/preparation/subset-source-preflight.json']=PREP/'subset-preflight/subset-source-preflight.json'
    files[CHECKPOINT+'/preparation/'+Path(__file__).name]=Path(__file__)
    files[CHECKPOINT+'/preparation/update-publication-notes.py']=PREP/'update-publication-notes.py'
    old_review=read(BASE/'mime-mappings3/source-review-2.json')
    require(old_review['source_commit']=='8ada16c768f07d89c676b0ec09a6a56150f26ace' and old_review['approval'] is False and not old_review['source_blockers'] and sha(BASE/'mime-mappings3/source-review-2.json')=='26978e5f06afb881e0a9f7e41bfc2a6c342a67838ad431af13012c51e972bbb7','Frozen original source-only review differs')
    source_review=read(BASE/'mime-mappings3/repair1/source-review-2.json')
    require(source_review['source_commit']==SOURCE and source_review['approval'] is False and source_review['builds_run'] is False and source_review['native_images_viewed']==[] and not source_review['source_blockers'] and sha(BASE/'mime-mappings3/repair1/source-review-2.json')=='580567f58c265e25ac1ba4d068cd2d1fceee7b2316b2c2949e1bdfa365adc1aa','Frozen repaired source-only review differs')
    receipt=read(BASE/'mime-mappings3/assertion-preservation-3.json');proof=receipt['files'][0];production=receipt['files'][1]
    repair=read(BASE/'mime-mappings3/repair1/assertion-preservation-3.json');wheel=read(BASE/'mime-mappings3/repair1/wheel-assertion-preservation-root.json')
    require(receipt['base_commit']=='da53daa67f7fb88870ae653f61def7cfbe514242' and proof['assertions_before']==249 and proof['assertions_after']==358 and proof['tests_before']==6 and proof['tests_after']==7 and proof['original_assertions_preserved'] is True and proof['entire_baseline_prefix_bytewise_preserved'] is True and production['inserted_lines']==1 and production['removed_lines']==11 and production['prefix_before_file_action_bytewise_preserved'] is True,'Historical original source/assertion/outer-only production proof differs')
    require(repair['failed_source_assertions']==358 and repair['repair_assertions']==360 and repair['tests_after']==7 and repair['original_assertions_preserved'] and repair['banked_249_assertion_file_is_exact_prefix'] and hashlib.sha256(git('show',SOURCE+':'+proof['path'])).hexdigest()==repair['repair_source_sha256'] and hashlib.sha256(git('show',SOURCE+':'+production['path'])).hexdigest()==production['source_sha256']==repair['production_source_sha256'],'Current repaired MIME assertion/production proof differs')
    require(wheel['before_assertions']==wheel['after_assertions']==92 and wheel['all_behavior_tests_and_input_helpers_bytewise_unchanged'] and wheel['overall_timeout_seconds']==2 and wheel['single_wheel_no_retry'] and hashlib.sha256(git('show',SOURCE+':'+wheel['file'])).hexdigest()==wheel['sha256'],'Current wheel source/assertion/deadline/input proof differs')
    for name in ('source-review-2.json','source-review-2.md','assertion-preservation-3.json','wheel-assertion-preservation-root.json','diagnosis-3.md','menu-choice-wheel-triage-2.json','menu-choice-wheel-triage-2.md','repair.diff'):
        path=BASE/'mime-mappings3/repair1'/name;require(path.is_file() and not path.is_symlink(),'Repair provenance missing: '+name);files[CHECKPOINT+'/preparation/repair1/'+name]=path
    failed_root=BASE/'ci-8ada16c76';failed=read(failed_root/'failed-outcome.json')
    require(failed['source_commit']=='8ada16c768f07d89c676b0ec09a6a56150f26ace' and failed['run_id']==37609745626 and failed['conclusion']=='failure' and failed['gui_tests']['passed']==708 and failed['gui_tests']['failed']==2,'Historical failed outcome differs')
    failed_manifest=read(failed_root/'native-render-manifest.json')
    require(failed_manifest['source_commit']==failed['source_commit'] and failed_manifest['run_id']==failed['run_id'] and failed_manifest['artifact_id']==11477743874 and failed_manifest['zip_sha256']=='c62b8014bf20bdab6545427caccc782dc4100108f9de69c011aec04ad3fd7534','Historical failed native archive provenance differs')
    require(not required_images&{r['name']for r in failed_manifest['images']},'Unexpected selected failed-run capture; re-review diagnostic scope')
    for name in ('failed-outcome.json','full-check.log','hydrus-tests.log','tests.log','clippy.log','hydrus-clippy.log','parity.log','cache-maintenance.log','run-latest.json','jobs-latest.json','transitions.jsonl','test-extraction-provenance.json','native-render-manifest.json','artifact-11477743874.json','artifact-11478071120.json','artifact-11478248680.json'):
        path=failed_root/name;require(path.is_file() and not path.is_symlink(),'Failed-run diagnostic evidence missing: '+name);files[CHECKPOINT+'/failed-runs/8ada16c76/'+name]=path
    for name in ('source-review-2.json','source-review-2.md','assertion-preservation-3.json','expected-captures-3.json','implementation-notes-3.md'):
        path=BASE/'mime-mappings3/repair1/failed-source-8ada16c76'/name;require(path.is_file() and not path.is_symlink(),'Frozen failed-source proof missing: '+name);files[CHECKPOINT+'/failed-runs/8ada16c76/source/'+name]=path
    files[CHECKPOINT+'/preparation/historical-preflight.md']=BASE/'file-call-queue3/next-mime-preflight-3.md'
    # These committed inputs retain original recording authorship; not a new rerun.
    for relative in ('oracle/record_open_externally.py','oracle/fixtures/open_externally.json','oracle/fixtures/open_externally.png'):
        path=REPO/relative;require(path.is_file()and not path.is_symlink() and sha(path)==hashlib.sha256(git('show',SOURCE+':'+relative)).hexdigest(),'Committed reference input differs');files[CHECKPOINT+'/reference/committed/'+path.name]=path
    copied_files={relative:{'input':str(path),'sha256':sha(path),'bytes':path.stat().st_size} for relative,path in files.items()}
    report={'source_commit':SOURCE,'run_id':RUN,'helper_sha256':sha(Path(__file__)),'state':'gates_and_staged_preservation_verified_for_root_review','publication_performed':False,'new_approval_created':False,'canonical_writes':False,'selected_count':3,'prior365_nonanchor_nodes_preserved':True,'all_goal1274_and_original_topology_preserved':True,'goal_updates_after_root_actual_publication_only':True,'unselected_assessments_limits_metadata_and_topology_preserved':True,'native1799_preserved_except_exact_source_anchor_remaps':True,'node_delta_fields':deltas,'exact_source_anchor_remaps':anchor_changes,'ancestor_count_summaries':ancestor_counts,'staged_ledger':368,'pending_after_root_banks3':7,'validation_scope':'linux','required_platforms':['linux'],'deferred_platforms':['windows','macos'],'patch_digest':pub.digest(patch),'source_census_digest':pub.digest(census),'artifact_id':manifest['artifact_id'],'historical_image_provenance':historical_image_provenance,'historical_images_counted_as_fresh':False,'current_assertions':{'open_externally':360,'menu_choice_wheel':92},'prior_assertions':{'open_externally':249},'completed_failed_run_evidence_added':True,'failed_source_selected_captures':0,'failed_run_completion_credit':0,'files':copied_files,'scope':'Evidence integrity/preservation audit only. Existing root/reviewer packets supply approval; helper does not inspect images or grant approval.'}
    if args.copy_scratch:
        out.parent.mkdir(parents=True,exist_ok=True)
        temporary=Path(tempfile.mkdtemp(prefix=out.name+'.tmp-',dir=out.parent))
        try:
            for relative,path in files.items():
                target=temporary/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(path,target);require(sha(target)==copied_files[relative]['sha256'],'Evidence changed during copy: '+relative)
            (temporary/CHECKPOINT/'preservation-and-copy-audit.json').write_text(json.dumps(report,indent=2)+'\n')
            checkpoint_readme=temporary/CHECKPOINT/'README.md'
            checkpoint_readme.write_text('# Prepared Linux publication evidence packet\n\nExact source `'+SOURCE+'`, run '+str(RUN)+'. Existing root and one independent review packet cover exactly three selected leaves; required Linux CI and fresh artifact identity/fingerprints match. The scratch staged ledger is prior 365 + 3 = 368. This three-leaf cohort leaves 7 pending. This helper creates no approval, canonical publication or commit. Windows/macOS are deferred under immutable source policy.\n\nSee review.json, selected-patch.json, ci-evidence.json, browser/browser-check.json, native-render-manifest.json and preservation-and-copy-audit.json. Root separately reviews/copies this tree, updates narrative documents and commits/pushes the checkpoint.\n')
            temporary.rename(out)
        except BaseException:
            shutil.rmtree(temporary);raise
    print(json.dumps({'state':report['state'],'copy_tree_created':args.copy_scratch,'output':str(out) if args.copy_scratch else None,'files_verified':len(files),'prior365_preserved':True,'canonical_writes':False,'publication_performed':False},indent=2))


if __name__=='__main__':
    try:main()
    except (KeyError,ValueError,OSError,subprocess.CalledProcessError) as error:
        raise SystemExit('BLOCKED (no copy/publication): '+str(error))
