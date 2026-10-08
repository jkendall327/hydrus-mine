#!/usr/bin/env python3
"""Stage independently reviewed historical-leaf verification, never implementation credit.

Immutable original goal classifications and append-only evidence batches are
separate from the implementation ledger. Artifacts remain pinned to each batch's
source and CI run even when later inventory reports are republished.
"""
import argparse
import copy
from datetime import datetime, timezone
import hashlib
import json
import re
from pathlib import Path, PurePosixPath
import subprocess

import gui_publish as publication

DATA = publication.DATA
GOAL_SOURCE = '1ce5a2c38eb779d544787ce5c09092dd1d7a202f'
GOAL = f'{DATA}/all-leaves-goal'
CROSSWALK = f'{GOAL}/original-1812-id-crosswalk.json'
CHECKLIST = f'{GOAL}/checklist.json'
LEDGER = f'{GOAL}/verification-ledger.json'
EMPTY = {'schema_version': 1, 'classification_source': GOAL_SOURCE, 'batches': []}
require = publication.require
digest = publication.digest


def safe_path(path):
    require(isinstance(path, str) and path and not PurePosixPath(path).is_absolute()
            and all(p not in ('..', '.') for p in path.split('/'))
            and '\\' not in path, 'Evidence path must be repository relative')
    return path


def blob(commit, path):
    return subprocess.check_output(['git', 'show', f'{commit}:{safe_path(path)}'], cwd=publication.ROOT)


def exists(commit, path):
    return bool(publication.git('ls-tree', '--name-only', commit, safe_path(path)).strip())


def load(commit):
    return publication.read(commit, LEDGER) if exists(commit, LEDGER) else copy.deepcopy(EMPTY)


def original_leaves(commit):
    frozen = publication.read(GOAL_SOURCE, CROSSWALK)
    require(publication.read(commit, CROSSWALK) == frozen, 'Immutable goal crosswalk changed')
    rows = {r['id']: r['baseline_status'] for r in frozen['ids']
            if r['current_classification']['classification'] == 'individual_terminal_leaf'}
    require(len(rows) == 1274, 'Original goal denominator changed')
    current = publication.read(commit, CHECKLIST)['leaves']
    require(len(current) == len(rows) and {r['id']: r['baseline_status'] for r in current} == rows,
            'Immutable goal IDs/classifications changed')
    return rows


def append_only(before, after):
    require(after.get('schema_version') == 1 and after.get('classification_source') == GOAL_SOURCE
            and isinstance(after.get('batches'), list), 'Invalid verification ledger schema')
    prior = before['batches']
    require(len(after['batches']) >= len(prior) and after['batches'][:len(prior)] == prior,
            'Prior verification approvals changed or were dropped')


def history(commit):
    """Catch dropped/altered approvals, including edits hidden by later commits."""
    prior = copy.deepcopy(EMPTY)
    changes = publication.git('log', '--first-parent', '--reverse', '--format=%H', commit, '--', LEDGER).splitlines()
    for change in changes:
        require(exists(change, LEDGER), 'Verification ledger was removed')
        current = load(change)
        append_only(prior, current)
        prior = current
    append_only(prior, load(commit))


def validate_batch(batch, rows, implementation_ids, read_artifact):
    manifest, review, ci = (batch[key] for key in ('manifest', 'review', 'ci'))
    source = manifest['source_commit']
    require(isinstance(source, str) and re.fullmatch(r'[0-9a-f]{40}', source), 'Full source SHA required')
    publication.validate_ci(ci, source)
    require(review.get('decision') == 'approved' and bool(str(review.get('reviewed_by', '')).strip()),
            'Independent approved review required')
    require(review.get('source_commit') == source and review.get('manifest_sha256') == digest(manifest)
            and review.get('ci_sha256') == digest(ci), 'Review source/evidence fingerprint differs')
    require(manifest.get('ci_run_id') == ci['run_id'], 'Evidence belongs to another CI run')
    leaves = manifest['leaves']
    ids = [leaf['id'] for leaf in leaves]
    require(ids and len(ids) == len(set(ids)) and sorted(review.get('selected_verification_ids', [])) == sorted(ids),
            'Duplicate, empty or unreviewed verification selection')
    require(all(rows.get(key) == 'first_pass' for key in ids) and not set(ids) & set(implementation_ids),
            'Verification IDs must be original historical leaves without implementation credit')
    evidence = manifest['evidence']
    require(isinstance(evidence, dict) and evidence, 'Missing verification evidence')
    used = set()
    for leaf in leaves:
        keys = leaf['evidence_ids']
        require(isinstance(keys, list) and len(keys) == len(set(keys)) and set(keys) <= set(evidence),
                'Unknown or duplicate leaf evidence')
        used.update(keys)
        require({evidence[key]['kind'] for key in keys} >= {'reference', 'native', 'render'},
                'Each leaf needs reference, native and rendered evidence')
        require(isinstance(leaf.get('limitations'), list) and leaf['limitations']
                and isinstance(leaf.get('behavior'), str) and leaf['behavior'].strip(),
                'Each leaf needs explicit behavior and limitations')
    require(used == set(evidence), 'Unmapped evidence')
    for key, item in evidence.items():
        require(item['kind'] in {'reference', 'native', 'render'} and item.get('description'), 'Invalid evidence kind/scope')
        require(item.get('sources'), 'Evidence lacks source anchors')
        for anchor in item['sources']:
            data = blob(source, anchor['path'])
            require(hashlib.sha256(data).hexdigest() == anchor['sha256'], 'Evidence source hash differs')
        artifact = item['artifact']
        path = safe_path(artifact['path'])
        require(path.startswith(f'{DATA}/checkpoints/'), 'Runtime evidence must be retained in a checkpoint')
        data = read_artifact(path)
        require(hashlib.sha256(data).hexdigest() == artifact['sha256'], 'Runtime artifact hash differs')
        if item['kind'] == 'render':
            require(data.startswith(b'\x89PNG\r\n\x1a\n') and key in review.get('directly_inspected_render_ids', []),
                    'Fresh PNG must be directly inspected')
    return set(ids)


def validate_implementation(commit, implementation, read_record, pending_review=None):
    """Retain the approved foundation and require reviewed, eligible additions."""
    trusted = publication.read(GOAL_SOURCE, f'{DATA}/overnight/progress.json')
    rows = original_leaves(commit)
    ids = implementation['completed_feature_ids']
    require(implementation.get('ci') == 'passed', 'Implementation ledger is not validated')
    publication.validate_ci(implementation['ci_evidence'], implementation['source_commit'])
    require(len(ids) == len(set(ids)) == implementation['concrete_implementation_completions']
            and all(rows.get(key) in {'partial', 'missing'} for key in ids),
            'Implementation credit must retain original deficit-leaf eligibility')
    proposals = {p['id']: p for p in implementation['proposals']}
    require(len(proposals) == len(implementation['proposals']), 'Duplicate implementation proposals')
    for key in ids:
        claim = proposals.get(key, {})
        require(claim.get('classification') == 'concrete_leaf' and claim.get('after') == 'first_pass'
                and claim.get('before') == rows[key]
                and claim.get('change_kind') in {'implementation', 'new_behavior'}
                and claim.get('count_as_completed_leaf') is not False,
                'Implementation completion lacks eligible proposal')
    require(set(trusted['completed_feature_ids']) <= set(ids), 'Approved implementation sign-offs were dropped')
    if implementation == trusted:
        return
    source = implementation['source_commit']
    prior = publication.read(source, f'{DATA}/overnight/progress.json')
    require(prior != implementation, 'Implementation approval chain does not advance')
    validate_implementation(source, prior, lambda path: blob(source, path))
    added = set(ids) - set(prior['completed_feature_ids'])
    require(set(prior['completed_feature_ids']) <= set(ids)
            and sorted(added) == implementation.get('additional_completed_feature_ids')
            and len(added) == implementation.get('additional_concrete_implementation_completions'),
            'Implementation additions or prior preservation differ')
    if pending_review is None:
        audit = json.loads(read_record(f'{DATA}/audit/anchor-remap-{source[:8]}-publication.json'))
        require(audit['source_commit'] == source
                and audit['input_fingerprints']['ci'] == digest(implementation['ci_evidence']),
                'Implementation audit/CI differs')
        review = audit['review']
    else:
        review = pending_review
    require(review.get('source_commit') == source and review.get('decision') == 'approved'
            and bool(str(review.get('reviewed_by', '')).strip())
            and sorted(review.get('selected_completion_ids', [])) == sorted(added),
            'Implementation additions lack exact independent approval')


def implementation_history(commit):
    """A valid old ledger must not roll back a newer published checkpoint."""
    path = f'{DATA}/overnight/progress.json'
    prior = set(publication.read(GOAL_SOURCE, path)['completed_feature_ids'])
    for change in publication.git('log', '--first-parent', '--reverse', '--format=%H',
                                  f'{GOAL_SOURCE}..{commit}', '--', path).splitlines():
        value = publication.read(change, path)
        current = set(value['completed_feature_ids'])
        require(prior <= current, 'Published implementation history regressed')
        validate_implementation(change, value, lambda p: blob(change, p))
        prior = current


def validate_ledger(commit, ledger, implementation, read_artifact, implementation_review=None):
    rows = original_leaves(commit)
    implementation_history(commit)
    committed = publication.read(commit, f'{DATA}/overnight/progress.json')
    require(set(committed['completed_feature_ids']) <= set(implementation['completed_feature_ids']),
            'Candidate rolls back committed implementation credit')
    validate_implementation(commit, implementation, read_artifact, implementation_review)
    append_only(EMPTY, ledger)
    ids = implementation['completed_feature_ids']
    require(len(ids) == len(set(ids)) == implementation['concrete_implementation_completions']
            and set(ids) <= set(rows), 'Implementation ledger differs from original goal')
    verified = set()
    for index, batch in enumerate(ledger['batches']):
        source = batch['manifest']['source_commit']
        # Each approval extends the exact prior ledger at its validated source.
        require(load(source)['batches'] == ledger['batches'][:index], 'Batch prior-source history differs')
        require(original_leaves(source) == rows, 'Batch goal classification differs')
        added = validate_batch(batch, rows, ids, read_artifact)
        require(not added & verified, 'Duplicate historical verification across batches')
        verified.update(added)
    return {'original_goal_leaves': len(rows), 'implementation_completions': len(ids),
            'historical_verifications': len(verified), 'verified_goal_leaves': len(set(ids) | verified),
            'verified_feature_ids': sorted(set(ids) | verified), 'historical_feature_ids': sorted(verified),
            'batches': [{'source_commit': b['manifest']['source_commit'], 'ci_url': b['ci']['url'],
                        'completed_at_utc': b['completed_at_utc'], 'leaves': b['manifest']['leaves']}
                       for b in ledger['batches']]}


def inherited(commit, implementation, implementation_review=None):
    history(commit)
    if not exists(commit, LEDGER):
        return None
    return validate_ledger(commit, load(commit), implementation, lambda p: blob(commit, p), implementation_review)


def checklist(commit, implementation, summary):
    value = publication.read(commit, CHECKLIST)
    signed = set(implementation['completed_feature_ids'])
    historical = set(summary['historical_feature_ids'])
    value.update(validated_implementation_source=implementation['source_commit'],
                 explicit_implementation_signoffs=len(signed), historical_verification_approvals=sorted(historical),
                 explicit_historical_verifications=len(historical), verified_goal_leaves=len(signed | historical),
                 remaining_without_explicit_signoff=len(value['leaves'])-len(signed | historical))
    for leaf in value['leaves']:
        leaf['signed_off_in_implementation_ledger'] = leaf['id'] in signed
        leaf['explicitly_verified_historical_leaf'] = leaf['id'] in historical
    return value


def publish(commit, output, manifest, review, ci, artifacts):
    require(manifest.get('source_commit') == commit, 'Manifest belongs to another source')
    history(commit)
    prior = load(commit)
    implementation = publication.read(commit, f'{DATA}/overnight/progress.json')
    # Retained artifacts must come from the immutable source, never a new packet.
    validate_ledger(commit, prior, implementation, lambda p: blob(commit, p))
    batch = {'completed_at_utc': datetime.now(timezone.utc).isoformat(),
             'manifest': manifest, 'review': review, 'ci': ci}
    ledger = copy.deepcopy(prior)
    ledger['batches'].append(batch)
    new_paths = {safe_path(e['artifact']['path']) for e in manifest['evidence'].values()}
    require(not any(exists(commit, path) for path in new_paths), 'New evidence cannot replace existing checkpoint artifacts')
    artifacts = artifacts.resolve()
    def read_artifact(path):
        if path not in new_paths:
            return blob(commit, path)
        target = (artifacts / safe_path(path)).resolve()
        require(target.is_relative_to(artifacts), 'Artifact escapes packet')
        return target.read_bytes()
    summary = validate_ledger(commit, ledger, implementation, read_artifact)
    snapshots = {name: publication.read(commit, f'{DATA}/{name}-inventory.json') for name in ('reference', 'native')}
    snapshots['reference']['goal_verification'] = summary
    cv = publication.checkpoint(commit)
    cv.DATA = publication.Template(commit)
    files = {LEDGER: ledger, CHECKLIST: checklist(commit, implementation, summary),
             f'{DATA}/reference-inventory.json': snapshots['reference'],
             'docs/rust/gui-progress.html': cv.render(snapshots), 'verification-summary.json': summary}
    # Validate paths and read every byte before creating the output directory.
    payloads = {p: read_artifact(p) for p in new_paths}
    publication.write_outputs(output, files)
    for path, data in payloads.items():
        target = output / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    return summary


def check_working(reference):
    """Validate canonical publication files, including uncommitted staged copies."""
    path = publication.ROOT / LEDGER
    commit = publication.git('rev-parse', 'HEAD').strip()
    history(commit)
    if not path.exists():
        require(not exists(commit, LEDGER), 'Working verification ledger was removed')
        return
    ledger = json.loads(path.read_text())
    append_only(load(commit), ledger)
    require(json.loads((publication.ROOT / CROSSWALK).read_text()) == publication.read(GOAL_SOURCE, CROSSWALK),
            'Immutable goal crosswalk changed')
    implementation = json.loads((publication.ROOT / DATA / 'overnight/progress.json').read_text())
    summary = validate_ledger(commit, ledger, implementation,
                              lambda p: (publication.ROOT / safe_path(p)).read_bytes())
    require(reference.get('goal_verification') == summary, 'Report verification summary is stale')
    require(json.loads((publication.ROOT / CHECKLIST).read_text()) == checklist(commit, implementation, summary),
            'Goal checklist differs from published verification union')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--review', required=True, type=Path)
    parser.add_argument('--ci-evidence', required=True, type=Path)
    parser.add_argument('--artifacts', required=True, type=Path)
    args = parser.parse_args()
    result = publish(args.source, args.out, *(json.loads(p.read_text()) for p in
                     (args.manifest, args.review, args.ci_evidence)), args.artifacts)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
