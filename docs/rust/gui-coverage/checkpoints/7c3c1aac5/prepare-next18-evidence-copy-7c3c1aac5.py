#!/usr/bin/env python3
"""Conditional source7c3 next18 + six prior wording errata preservation/evidence-copy preparation.
Default: check only. --copy-scratch writes a reviewable outside-repo copy tree.
Never publishes, writes canonical files, creates approvals, runs builds or uses network.
"""
import argparse,copy,hashlib,json,shutil,subprocess,tempfile,types
from pathlib import Path

BASE=Path('/workspace/validation-reviews').resolve()
REPO=Path('/workspace/hydrus-mine').resolve()
SOURCE='7c3c1aac5a60a9e3fb1e7a8864cb439914d4fd60'
RUN=37447108755
DATA='docs/rust/gui-coverage'
CHECKPOINT=DATA+'/checkpoints/7c3c1aac5'
FINAL=BASE/'publication-next18-7c3c1aac5-linux'
INPUT_HASHES={}


def require(v,message):
    if not v:raise ValueError(message)


def read(path):
    require(path.is_file() and not path.is_symlink(), 'Missing/nonregular evidence file: '+str(path))
    raw=path.read_bytes();fingerprint=hashlib.sha256(raw).hexdigest();require(str(path) not in INPUT_HASHES or INPUT_HASHES[str(path)]==fingerprint,'Evidence changed during validation: '+str(path));INPUT_HASHES[str(path)]=fingerprint
    return json.loads(raw)


def sha(path):
    require(path.is_file() and not path.is_symlink(),'Missing/nonregular fingerprinted evidence: '+str(path))
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
    parser.add_argument('--ci-root',type=Path,default=BASE/'ci-7c3c1aac5')
    parser.add_argument('--native-root',type=Path,default=BASE/'ci-7c3c1aac5/native-renders')
    parser.add_argument('--browser',type=Path,default=BASE/'report-browser-7c3c1aac5/browser-check.json',help='Actual passed browser-check.json for this exact staged report')
    parser.add_argument('--output',type=Path,default=FINAL/'copy-ready',help='New scratch directory; never repository destination')
    parser.add_argument('--copy-scratch',action='store_true',help='After all gates pass, atomically create outside-repo copy tree and preservation audit')
    args=parser.parse_args()
    final,stage,ciroot,native,browser,out=map(scratch,[args.final,args.stage,args.ci_root,args.native_root,args.browser,args.output])
    require(not out.exists(),'Refuse overwriting evidence-copy output: '+str(out))
    selection=read(BASE/'next18-selection-preparation-7c3c1aac5.json');selected=set(selection['selected_candidate_ids'])
    require(selection['source_commit']==SOURCE and len(selected)==18,'Selection source/18 differs')
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
    prior_ids=set(prior['completed_feature_ids']);require(len(prior_ids)==prior['concrete_implementation_completions']==301 and not prior_ids&selected,'Prior301/selection differs')
    pub.validate_ci(prior['ci_evidence'],prior['source_commit'])
    classes=pub.classification(frozen,cv)
    require(all(key in source_claims and pub.countable(key,source_claims[key]['claim'],classes,baseline) for key in selected),'Selected source-manifest claim is not countable')
    require(patch['baseline_git_head']==review['source_commit']==reconcile['source_commit']==SOURCE,'Final source differs')
    require(pub.digest(patch)==review['patch_sha256']==reconcile['final_patch_digest'],'Final review/audit patch digests differ')
    erratum_path=BASE/'review4-banked-erratum-6b66240c1-at-7c3c1aac5.json';erratum=read(erratum_path)
    errata_path=BASE/'review4-banked-erratum-reference-patch-7c3c1aac5.json';errata=read(errata_path)
    erratum_proof_path=BASE/'review4-banked-erratum-anchor-proof-7c3c1aac5.json';erratum_proof=read(erratum_proof_path)
    errata_ids=set(erratum['affected_selected_ids'])
    require(len(errata_ids)==6 and errata_ids<=prior_ids and not errata_ids&selected and erratum['completion_delta']==0 and not erratum['selected_for_new_completion'],'Errata IDs/credit differ')
    require(erratum['source_commit_checked']==errata['baseline_git_head']==erratum_proof.get('source_commit',erratum_proof.get('to_commit'))==SOURCE,'Erratum immutable source proof target differs')
    canonical_reference_sha256=hashlib.sha256(git('show',SOURCE+':'+DATA+'/reference-inventory.json')).hexdigest()
    if 'canonical_reference_input_sha256' in erratum_proof:require(erratum_proof['canonical_reference_input_sha256']==canonical_reference_sha256,'Erratum canonical reference fingerprint differs')
    remapping=erratum_proof.get('anchor_remapping',erratum_proof)
    require(remapping['to_commit']==SOURCE and remapping['from_commit']==original['reference']['git_head'],'Erratum anchor proof source checkpoint differs')
    require(set(reconcile['zero_credit_wording_errata_ids'])==errata_ids and reconcile['wording_erratum_sha256']==sha(erratum_path),'Root erratum audit differs')
    for record in (erratum['historical_review'],erratum['historical_markdown']):
        require(hashlib.sha256(git('show',SOURCE+':'+record['path'])).hexdigest()==record['sha256'] and sha(REPO/record['path'])==record['sha256'],'Archived evidence was changed')
    require(hashlib.sha256(git('show',SOURCE+':'+erratum['frozen_ledger']['path'])).hexdigest()==erratum['frozen_ledger']['sha256'],'Erratum prior301 ledger proof differs')
    original_repinned,_=cv.repin(copy.deepcopy(original['reference']),SOURCE)
    original_repinned_nodes={n['id']:n for n in original_repinned['nodes']}
    expected_errata={key:copy.deepcopy(original_repinned_nodes[key]) for key in errata_ids}
    # Whole-inventory repin elides per-anchor origin when it equals git_head;
    # an authored patch must state that exact source explicitly.
    for node in expected_errata.values():
        for anchor in cv.anchor_objects(node):anchor['anchor_git_head']=SOURCE
    literal_changes=[]
    for correction in erratum['corrections']:
        require(not correction['completed_ledger_membership'] and correction['unbanked_reference_id'] not in prior_ids,'Erratum invents completion membership')
        require(next(n for n in original['reference']['nodes'] if n['id']==correction['unbanked_reference_id'])['status']==correction['reference_status'],'Erratum unbanked status differs')
        for key in correction['affected_selected_ids']:
            require(key in errata_ids,'Erratum correction exceeds six IDs');limits=expected_errata[key]['limitations']
            require(limits.count(correction['incorrect_exact_string'])==1,'Erratum before literal differs')
            i=limits.index(correction['incorrect_exact_string']);limits[i]=correction['replacement_exact_string']
            literal_changes.append({'id':key,'field':'limitations','index':i,'before':correction['incorrect_exact_string'],'after':correction['replacement_exact_string']})
    require(len(literal_changes)==6 and {r['id']:r for r in errata['reference']['updates']}==expected_errata and not errata['reference']['additions'] and not any(errata['native'].values()),'Erratum changes more than reviewed literals/exact anchors')
    edits=patch['reference']['updates'];require(len(edits)==24 and {r['id'] for r in edits}==selected|errata_ids,'Patch edits differ from18 new +6 prior errata')
    require(not patch['reference'].get('additions') and not patch['reference'].get('deletions') and not patch['native'].get('updates') and not patch['native'].get('additions') and not patch['native'].get('deletions'),'Unexpected reference/native edits')
    require(len(review['selected_completion_ids'])==18 and len(review['reviewed_assessment_ids'])==24 and set(review['selected_completion_ids'])==selected and set(review['reviewed_assessment_ids'])==selected|errata_ids and not review['reviewed_native_ids'],'Root review IDs differ')
    require(str(review.get('reviewed_by','')).strip() and review.get('root_actual_native_inspection'),'Root actual inspection/reviewer is absent')
    require(manifest['source_commit']==SOURCE and manifest['run_id']==RUN and type(manifest['artifact_id']) is int,'Fresh native manifest identity differs')
    image_rows=manifest['images'];images={r['name']:r for r in image_rows};require(len(images)==len(image_rows),'Duplicate native manifest images')
    copied_images=set();approvals={};prior_approvals={};reviews=[]
    for number in [2,3,4,5,6]:
        path=BASE/f'final-review-{number}-7c3c1aac5.json';packet=read(path);md=path.with_suffix('.md');require(md.is_file(),'Review Markdown missing: '+str(md))
        require(packet['source_commit']==SOURCE and packet['run_id']==RUN and packet['artifact_id']==manifest['artifact_id'],'Independent review source/run/artifact differs: '+str(number))
        scope=packet['validation_scope'];require(scope=='linux' or scope.startswith('Linux only;'),'Independent review Linux scope differs')
        if 'native_zip_sha256' in packet:require(packet['native_zip_sha256']==manifest['zip_sha256'],'Review native archive hash differs')
        names={Path(s).name for s in strings(packet) if s.endswith('.png') and Path(s).name in images}
        require(names,'Independent packet names no images from fresh manifest: '+str(number));copied_images|=names
        def verify_image_hashes(value):
            if isinstance(value,dict):
                candidates=[value.get(k) for k in ('name','filename','path','file')]
                for candidate in candidates:
                    if isinstance(candidate,str) and candidate.endswith('.png') and Path(candidate).name in images and 'sha256' in value:
                        require(value['sha256']==images[Path(candidate).name]['sha256'],'Reviewer image fingerprint differs: '+candidate)
                for child in value.values():verify_image_hashes(child)
            elif isinstance(value,list):
                for child in value:verify_image_hashes(child)
        verify_image_hashes(packet)
        for claim in packet['claims']:
            key=claim['id'];require(key in selected and key not in approvals and claim['approval'] is True,'Unselected/duplicate/unapproved independent claim: '+key)
            approvals[key]=(claim,path)
        if number==4:
            prior_approvals={c['id']:c for c in packet['errata_prior_claims']}
            require(len(packet['errata_prior_claims'])==6 and set(prior_approvals)==errata_ids and all(c['approval'] is True for c in prior_approvals.values()),'Fresh six prior erratum approvals missing/held')
            for correction in erratum['corrections']:
                require(all(correction['replacement_exact_string'] in prior_approvals[k]['caveats'] for k in correction['affected_selected_ids']),'Fresh prior review does not preserve corrected caveats')
        reviews.append((path,md))
    require(set(approvals)==selected,'Independent approvals do not cover exactly18')
    for record in review['independent_reviews']:
        evidence=BASE/Path(record['path']).name;require(sha(evidence)==record['sha256'],'Root independent-review evidence hash differs')
    require({Path(r['path']).name for r in review['independent_reviews']}=={p.name for p,_ in reviews},'Root reviewer packet set differs')
    root_images={Path(s).name for s in strings(review['root_actual_native_inspection']) if s.endswith('.png')};require(root_images and root_images<=set(images),'Root inspected image absent from fresh manifest');copied_images|=root_images
    require(copied_images,'No actual image evidence')
    for name in copied_images:
        require(Path(name).name==name,'Unsafe image name');path=native/name;require(path.is_file() and not path.is_symlink() and sha(path)==images[name]['sha256'] and path.stat().st_size==images[name]['bytes'],'Native image bytes differ: '+name)
    # Reproduce only conditional text operations + verbatim approving caveats.
    mapping=read(BASE/'next18-conditional-status-text-map-7c3c1aac5.json');initial=read(Path(mapping['input_patch']))
    require(sha(Path(mapping['input_patch']))==mapping['input_sha256'] and sha(BASE/'next18-conditional-status-text-map-7c3c1aac5.json')==reconcile['mapping_sha256'] and mapping['conditional_operations']==reconcile['operations'],'Reconciliation mapping provenance differs')
    require(reconcile['mapping_applied_after_gates'] is True and reconcile['ci_evidence_sha256']==sha(ciroot/'ci-evidence.json'),'Reconciliation actual gate/CI provenance differs')
    expected={r['id']:copy.deepcopy(r) for r in initial['reference']['updates']};final_nodes={r['id']:r for r in edits}
    for op in sorted(mapping['conditional_operations'],key=lambda x:(x['id'],x['field'],-x['original_index'])):
        require(op['id'] in selected and op['field'] in ('limitations','remaining'),'Mapping exceeds selected limitation fields');row=expected[op['id']];index=op['original_index'];require(row[op['field']][index]==op['before'],'Original mapping text differs')
        if op['operation']=='remove_exact_list_item':del row[op['field']][index]
        elif op['operation']=='replace_exact_list_item':row[op['field']][index]=op['conditional_after']
        else:raise ValueError('Unexpected next18 mapping operation')
    def validation_metadata(row,path):
        row['implementation_validation'].update(source_commit=SOURCE,ci='passed',tests_executed_at_checkpoint=True,native_render_review='passed',review_required=False,run_url=ci['url'],validation_scope='linux',required_platforms=['linux'],deferred_platforms=['windows','macos'],independent_review_evidence={'path':CHECKPOINT+'/'+path.name,'sha256':sha(path)})
    for key,row in expected.items():
        claim,path=approvals[key];limits=claim.get('preserved_behavior_limits',claim.get('caveats',claim.get('surface_caveats',[])))
        require(isinstance(limits,list) and all(isinstance(t,str) for t in limits),'Invalid fresh caveat list')
        added=reconcile['added_review_scope_limits'][key];require(added==[t for t in limits if t not in row['limitations']],'Fresh behavior caveats omitted/invented')
        row['limitations'].extend(added);validation_metadata(row,path)
        require(row==final_nodes[key],'Final selected behavior/evidence/metadata exceeds literal map and fresh caveats: '+key)
    require(set(expected)==selected,'Initial mapping input differs from18 selected')
    prior_review_path=BASE/'final-review-4-7c3c1aac5.json'
    for key,row in expected_errata.items():
        validation_metadata(row,prior_review_path)
        require(row==final_nodes[key],'Prior erratum changed more than literal limitation/approved exact metadata: '+key)
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
            for key in selected|errata_ids:want[key]['implementation_validation'].update(ci='passed',source_commit=SOURCE,run_url=ci['url'],tests_executed_at_checkpoint=True,**validation)
        require(after==want,'Staged nodes differ from exact source pipeline: '+side)
        require(set(before)==set(after) and len(after)==(1812 if side=='reference' else 1799),'Staged topology/denominator differs')
        checked=cv.validate(staged[side],side)
        require(not checked['audit']['weak_first_pass'],'Weak FirstPass in staged inventory: '+side)
        if side=='native':require(not checked['audit']['opaque_native_windows'] and not checked['audit']['unassessed_nodes'] and not checked['audit']['catch_all_nodes'],'Opaque/incomplete staged native inventory')
        deltas[side]={key:sorted(k for k in set(row)|set(after[key]) if row.get(k)!=after[key].get(k)) for key,row in before.items() if row!=after[key]}
        if side=='reference':
            require(all(before[k]['status']==after[k]['status']=='first_pass' and before[k]['assessment']==after[k]['assessment'] for k in prior_ids),'Prior301 ID/status/assessment changed')
        for key,old in before.items():
            if side=='reference' and key in selected:continue
            new=after[key]
            # Exact pipeline permits source line remapping and correct ancestor
            # descendant counts; the six errata literals/metadata were proven separately.
            # No unselected status, assessment or ownership-credit changes.
            require(all(old.get(k)==new.get(k) for k in set(old)|set(new) if k not in (('native_source','reference_source','evidence','descendant_status_counts','limitations','implementation_validation') if side=='reference' and key in errata_ids else ('native_source','reference_source','evidence','descendant_status_counts'))),'Unselected non-anchor object changed: '+side+'/'+key)
            if old.get('descendant_status_counts')!=new.get('descendant_status_counts'):ancestor_counts.append({'inventory':side,'id':key,'before':old['descendant_status_counts'],'after':new['descendant_status_counts']})
            if any(old.get(k)!=new.get(k) for k in ('native_source','reference_source','evidence')):anchor_changes.append({'inventory':side,'id':key,'fields':[k for k in ('native_source','reference_source','evidence') if old.get(k)!=new.get(k)],'proof':'Exact source cv.repin output equals staged whole node; unchanged-line/hash mapping only, no alternate declaration or nearest guesses.'})
    progress=read(stage/f'{DATA}/overnight/progress.json');summary=read(stage/'publication-summary.json');stage_ci=read(stage/f'{DATA}/overnight/ci-evidence.json');audit=read(stage/f'{DATA}/audit/anchor-remap-{SOURCE[:8]}-publication.json')
    require(set(progress['completed_feature_ids'])==prior_ids|selected and len(progress['completed_feature_ids'])==progress['concrete_implementation_completions']==319 and set(progress['additional_completed_feature_ids'])==selected and progress['additional_concrete_implementation_completions']==18,'Staged completion ledger differs')
    require(progress['before']==prior['before'] and summary['prior_completions']==301 and summary['additional_completions']==18 and summary['validated_original_leaf_completions']==319 and summary['native_entries']==1799,'Staged summary/frozen baseline differs')
    require(pub.digest(stage_ci)==pub.digest(normalized_ci) and audit['input_fingerprints']['patch']==pub.digest(patch) and audit['input_fingerprints']['review']==pub.digest(review) and audit['input_fingerprints']['ci']==pub.digest(normalized_ci) and audit['input_fingerprints']['prior_ledger']==pub.digest(prior),'Staged CI/input fingerprints differ')
    census=read(stage/'source-census.json');prepared=read(BASE/'publication-subset-preflight-next18-7c3c1aac5-linux/subset-source-preflight.json')
    require(prepared['source_commit']==SOURCE and prepared['source_schema_preflight']=='passed' and set(prepared['selected_candidate_ids'])==selected,'Prepared source/schema/selection differs')
    require(pub.digest(census)==review['reviewed_source_census_sha256']==audit['input_fingerprints']['source_census']==prepared['full_source_census_digest_for_later_review'],'Staged reviewed source census differs')
    html=stage/'docs/rust/gui-progress.html';stage_audit=stage/f'{DATA}/audit/anchor-remap-{SOURCE[:8]}-publication.json'
    require(web['status']=='passed' and web['source_commit']==SOURCE and web['expected_signed_off']==319 and web['expected_additional']==18 and web['prior_signed_off']==301 and set(web['additional_selected_ids'])==selected,'Browser source/counts/selected scope differs')
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
    for name in ('linux-full.log','parity-models.log'):
        path=ciroot/name;require(path.is_file() and not path.is_symlink(),'Completed CI log missing: '+name);files[CHECKPOINT+'/'+name]=path
    for name in ('clippy.log','validation-outcome.json'):
        path=ciroot/name
        if path.is_file():
            if name.endswith('.json'):
                outcome=read(path);require(outcome['source_commit']==SOURCE and outcome['run_id']==RUN and outcome['conclusion']=='success','Optional outcome source/gate differs')
            files[CHECKPOINT+'/'+name]=path
    for path in (erratum_path,errata_path,erratum_proof_path,BASE/'next18-conditional-status-text-map-7c3c1aac5.json',BASE/'next18-conditional-status-text-map-7c3c1aac5-proof.json',BASE/'next18-bank301-preservation-proof-7c3c1aac5.json',BASE/'publication-subset-preflight-next18-7c3c1aac5-linux/subset-source-preflight.json',Path(__file__),Path(__file__).with_suffix('.md'),BASE/'next18-evidence-helper-preparation-proof-7c3c1aac5.json',BASE/'failed115d-preservation-proof-for-7c3c1aac5.json',BASE/'next18-six-errata-preservation-check-7c3c1aac5.json',BASE/'review4-banked-erratum-6b66240c1-at-7c3c1aac5.md'):
        files[CHECKPOINT+'/'+path.name]=path
    for record in (erratum['historical_review'],erratum['historical_markdown']):
        path=REPO/record['path'];files[CHECKPOINT+'/historical/'+path.name]=path
    failed_proof=read(BASE/'failed115d-preservation-proof-for-7c3c1aac5.json')
    require(failed_proof['source_commit']=='115d5532a0fa0fe9a937a23b556dfd3aed965175' and failed_proof['run_id']==37443398472 and failed_proof['conclusion']=='failure','Failed115d provenance identity differs')
    for record in failed_proof['files']:
        path=scratch(Path(record['path']));require(sha(path)==record['sha256'] and path.stat().st_size==record['bytes'],'Retained failed115d input changed')
        require(Path(record['name']).name==record['name'],'Unsafe failed115d evidence name')
        files[CHECKPOINT+'/failed-runs/115d5532a/'+record['name']]=path
    copied_files={relative:{'input':str(path),'sha256':sha(path),'bytes':path.stat().st_size} for relative,path in files.items()}
    report={'source_commit':SOURCE,'run_id':RUN,'helper_sha256':sha(Path(__file__)),'state':'gates_and_staged_preservation_verified_for_root_review','publication_performed':False,'new_approval_created':False,'canonical_writes':False,'selected_count':18,'prior301_ids_statuses_assessments_preserved':True,'prior295_non_anchor_whole_objects_preserved':True,'six_prior_errata_ids':sorted(errata_ids),'six_literal_limitation_changes':literal_changes,'six_fresh_review_metadata_updates_exact':True,'canonical_reference_input_sha256':canonical_reference_sha256,'unselected_except_explicit_six_errata_non_anchor_objects_preserved':True,'native1799_preserved_except_exact_source_anchor_remaps':True,'node_delta_fields':deltas,'exact_source_anchor_remaps':anchor_changes,'ancestor_count_summaries':ancestor_counts,'staged_ledger':319,'pending_after_root_banks18':56,'validation_scope':'linux','required_platforms':['linux'],'deferred_platforms':['windows','macos'],'patch_digest':pub.digest(patch),'source_census_digest':pub.digest(census),'artifact_id':manifest['artifact_id'],'files':copied_files,'scope':'Evidence integrity/preservation audit only. Existing root/reviewer packets supply approval; helper does not inspect images or grant approval.'}
    if args.copy_scratch:
        out.parent.mkdir(parents=True,exist_ok=True)
        temporary=Path(tempfile.mkdtemp(prefix=out.name+'.tmp-',dir=out.parent))
        try:
            for relative,path in files.items():
                target=temporary/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(path,target);require(sha(target)==copied_files[relative]['sha256'],'Evidence changed during copy: '+relative)
            (temporary/CHECKPOINT/'preservation-and-copy-audit.json').write_text(json.dumps(report,indent=2)+'\n')
            checkpoint_readme=temporary/CHECKPOINT/'README.md'
            checkpoint_readme.write_text('# Prepared Linux publication evidence packet\n\nExact source `'+SOURCE+'`, run '+str(RUN)+'. Existing root and five independent review packets cover exactly18 new selected leaves plus six prior zero-credit wording corrections; required Linux CI and fresh artifact identity/fingerprints match. The scratch staged ledger is prior301 +18 =319, leaving56 pending. Six prior limitation corrections preserve statuses/assessments and receive separate fresh review. This helper creates no approval, canonical publication or commit. Windows/macOS are deferred under immutable source policy.\n\nSee review.json, selected-patch.json, ci-evidence.json, browser/browser-check.json, native-render-manifest.json and preservation-and-copy-audit.json. Original six-leaf historical evidence is preserved; the literal erratum and anchor proof document its correction without rewriting archives. Root separately reviews/copies this tree, updates narrative documents and commits/pushes the checkpoint.\n')
            temporary.rename(out)
        except BaseException:
            shutil.rmtree(temporary);raise
    print(json.dumps({'state':report['state'],'copy_tree_created':args.copy_scratch,'output':str(out) if args.copy_scratch else None,'files_verified':len(files),'prior301_statuses_assessments_preserved':True,'six_errata_only':True,'canonical_writes':False,'publication_performed':False},indent=2))


if __name__=='__main__':
    try:main()
    except (KeyError,ValueError,TypeError,StopIteration,OSError,subprocess.CalledProcessError) as error:
        raise SystemExit('BLOCKED (no copy/publication): '+str(error))
