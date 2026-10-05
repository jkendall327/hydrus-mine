#!/usr/bin/env python3
"""Prepare or stage reviewed GUI coverage; never write canonical files.

All source, inventory, claims and renderer inputs come from an explicit Git
checkpoint. Preparation makes no completion claims. Staging additionally needs
exact successful CI and a fingerprinted review packet selecting original IDs.
"""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess
import types

ROOT = Path(__file__).resolve().parents[1]
DATA = 'docs/rust/gui-coverage'
FROZEN = '4356918f41d2cb2b0159eb8c466d987de9fa9339'
BEFORE = {'missing': 862, 'partial': 378, 'first_pass': 572}
JOBS = {'check': {'fmt', 'clippy', 'test', 'parity ratchet'},
        'parity-models': {'reference and backend tests'},
        'other-platforms (windows-latest)': {'build', 'test'},
        'other-platforms (macos-latest)': {'build', 'test'}}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True)


def read(commit, path):
    return json.loads(git('show', f'{commit}:{path}'))


def digest(value):
    """Hash parsed JSON, ignoring indentation and object key ordering."""
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False).encode()).hexdigest()


def checkpoint(commit):
    require(re.fullmatch(r'[0-9a-f]{40}', commit), 'Use a full 40-character source SHA')
    require(git('rev-parse', '--verify', commit + '^{commit}').strip() == commit,
            'Source commit is unavailable')
    cv = types.ModuleType('checkpoint_gui_coverage')
    cv.__file__ = str(ROOT / 'scripts/gui_coverage.py')
    exec(compile(git('show', f'{commit}:scripts/gui_coverage.py'), cv.__file__, 'exec'), cv.__dict__)
    return cv


class MemoryPatch:
    def __init__(self, value):
        self.value = value

    def read_text(self, encoding='utf-8'):
        return json.dumps(self.value)

    def relative_to(self, root):
        return Path(DATA) / 'overnight/reviewed-publication-input.json'


class Template:
    def __init__(self, commit):
        self.commit = commit

    def __truediv__(self, path):
        require(path == 'viewer.template.html', 'Unexpected renderer input')
        return self

    def read_text(self, encoding='utf-8'):
        return git('show', f'{self.commit}:{DATA}/viewer.template.html')


def inputs(commit):
    baseline = read(commit, f'{DATA}/overnight/baseline.json')
    frozen = read(FROZEN, f'{DATA}/reference-inventory.json')
    require(baseline['git_head'] == FROZEN and len(baseline['reference']) == 1812,
            'Frozen baseline source/denominator changed')
    require(dict(Counter(baseline['reference'].values())) == BEFORE, 'Frozen baseline counts changed')
    require({n['id']: n['status'] for n in frozen['nodes']} == baseline['reference'],
            'Frozen source and baseline ledger disagree')
    snapshots = {name: read(commit, f'{DATA}/{name}-inventory.json') for name in ('reference', 'native')}
    require({n['id'] for n in snapshots['reference']['nodes']} == set(baseline['reference']),
            'Original reference IDs changed')
    prior = read(commit, f'{DATA}/overnight/progress.json')
    claims = {}
    for path in git('ls-tree', '-r', '--name-only', commit, f'{DATA}/parity').splitlines():
        if not path.endswith('.json'):
            continue
        packet = read(commit, path)
        for claim in packet.get('claims', packet.get('entries', [])):
            key = claim.get('reference_id', claim.get('id', claim.get('originalID')))
            require(key in baseline['reference'] and key not in claims, f'Unknown/duplicate claim: {key}')
            require(claim.get('previous_status', claim.get('before')) == baseline['reference'][key],
                    f'Claim changed frozen before status: {key}')
            claims[key] = {'claim': claim, 'manifest': path}
    return baseline, frozen, snapshots, prior, claims


def masked(text):
    """Keep offsets/newlines while hiding comments and quoted strings."""
    pattern = r'//[^\n]*|/\*[\s\S]*?\*/|"(?:\\[\s\S]|[^"\\])*"'
    return re.sub(pattern, lambda m: ''.join('\n' if c == '\n' else ' ' for c in m[0]), text)


def census(commit, cv, native):
    """Inventory every exported Window and its source declarations/widgets.

    This lexical census is a review aid, not a Slint parser or behavior test.
    Shared component definitions are also listed, so custom tab/star components
    can be followed instead of disappearing behind a generic widget name.
    """
    windows, components = [], []
    for path in git('ls-tree', '-r', '--name-only', commit, 'crates/hydrus-gui/ui').splitlines():
        if not path.endswith('.slint'):
            continue
        source = '\n'.join(cv.source_lines(commit, path))
        clean = masked(source)
        for match in re.finditer(r'\b(?:export\s+)?component\s+(\w+)(?:\s+inherits\s+(\w+))?\s*\{', clean):
            depth, end = 1, match.end()
            while depth and end < len(clean):
                depth += (clean[end] == '{') - (clean[end] == '}')
                end += 1
            require(depth == 0, f'Unbalanced component: {path}:{match[1]}')
            body = clean[match.end():end - 1]
            tokens = []
            # Include declarations, all widget instances, tab titles and literal
            # actions. Do not turn every property into a user-facing feature.
            patterns = {
                'callback': r'\bcallback\s+([\w-]+)\s*\(',
                'property': r'\b(?:in-out|in|out)\s+property\s*<[^;]+?>\s*([\w-]+)',
                'widget': r'\b([A-Z]\w*)\s*\{',
            }
            for kind, pattern in patterns.items():
                for token in re.finditer(pattern, body):
                    offset = match.end() + token.start()
                    line = source.count('\n', 0, offset) + 1
                    tokens.append({'kind': kind, 'name': token[1], 'source': {
                        'path': path, 'line': line, 'line_sha256': cv.line_digest(cv.source_lines(commit, path)[line - 1])}})
            literal_body = source[match.end():end - 1]
            for token in re.finditer(r'\b(?:title|text)\s*:\s*"([^"\n]+)"|root\.action\(\s*"([^"\n]+)"', literal_body):
                offset = match.end() + token.start()
                if not re.match(r'\b(?:title|text)\s*:|root\.action\(', clean[offset:]):
                    continue
                line = source.count('\n', 0, offset) + 1
                tokens.append({'kind': 'literal_control', 'name': token[1] or token[2], 'source': {
                    'path': path, 'line': line, 'line_sha256': cv.line_digest(cv.source_lines(commit, path)[line - 1])}})
            record = {'component': match[1], 'inherits': match[2] or 'Rectangle', 'path': path,
                      'line': source.count('\n', 0, match.start()) + 1,
                      'controls': sorted(tokens, key=lambda t: (t['source']['line'], t['kind'], t['name']))}
            record['source'] = {'path': path, 'line': record['line'],
                                'line_sha256': cv.line_digest(cv.source_lines(commit, path)[record['line'] - 1])}
            components.append(record)
            if match[2] == 'Window' and match[0].lstrip().startswith('export '):
                windows.append(record)
    by_id = {n['id']: n for n in native['nodes']}
    metrics = cv.node_metrics(native)
    for window in windows:
        associations = []
        for node in by_id.values():
            if node.get('component') != window['component']:
                continue
            anchor = node.get('native_source', {})
            path = anchor.get('path', anchor.get('relativepath'))
            if path and re.search(r'\b' + re.escape(window['component']) + r'\b', '\n'.join(cv.source_lines(commit, path))):
                associations.append(node)
        window['node_ids'] = [n['id'] for n in associations]
        window['navigable_node_ids'] = [n['id'] for n in associations if n['status'] != 'unassessed' and (
            metrics[n['id']]['descendant_count'] or n.get('target_id') or n.get('canonical_editor') or n.get('editor_refs'))]
    return {'source_commit': commit, 'exported_window_count': len(windows), 'windows': windows,
            'shared_components': [c for c in components if c['inherits'] != 'Window'],
            'method': 'Lexical source census, including callbacks, properties, widgets, tab/control literals and shared components; requires manual semantic granularity review.'}


def classification(frozen, cv):
    children = Counter(n['parent_id'] for n in frozen['nodes'])
    return {n['id']: ('parent_or_alias' if children[n['id']] or n['kind'] in cv.STRUCTURAL_KINDS
                     or n.get('target_id') or n.get('canonical_editor') else 'concrete_leaf') for n in frozen['nodes']}


def countable(key, claim, classes, baseline):
    """Count implemented original leaves; evidence and navigation never count."""
    return (key in baseline['reference'] and classes.get(key) == 'concrete_leaf'
            and baseline['reference'][key] in {'missing', 'partial'}
            and claim.get('proposed_status', claim.get('proposedStatus')) == 'first_pass'
            and claim.get('change_kind', 'implementation') in {'implementation', 'new_behavior'}
            and claim.get('countAsCompletedLeaf') is not False)


def validate_ci(ci, commit):
    require(ci.get('source_commit') == commit and ci.get('head_sha', ci.get('headSha')) == commit,
            'CI evidence must name the exact source SHA and run head SHA')
    require(ci.get('status') == 'completed' and ci.get('conclusion') == 'success', 'CI is not completed/successful')
    jobs = ci.get('jobs', [])
    require(len(jobs) == len(JOBS) and {j.get('name') for j in jobs} == set(JOBS), 'CI required job set differs')
    run_id = ci.get('run_id')
    require(isinstance(run_id, int) and not isinstance(run_id, bool) and run_id > 0, 'Invalid CI run ID')
    require(ci.get('url') == f'https://github.com/jkendall327/hydrus-mine/actions/runs/{run_id}', 'CI run URL differs')
    for job in jobs:
        require(job.get('run_id') == run_id and job.get('status') == 'completed'
                and job.get('conclusion') == 'success', f'Unsuccessful/wrong-run CI job: {job.get("name")}')
        steps = {s['name']: s for s in job.get('steps', [])}
        for name in JOBS[job['name']]:
            require(name in steps and steps[name].get('status') == 'completed'
                    and steps[name].get('conclusion') == 'success', f'CI required step failed/absent: {job["name"]}/{name}')


def write_outputs(output, files):
    output = output.resolve()
    repo = ROOT.resolve()
    require(not output.is_relative_to(repo) and not repo.is_relative_to(output), 'Use a dedicated directory outside the repository')
    require(not output.exists(), 'Output directory already exists; choose a fresh review directory')
    output.mkdir(parents=True)
    for relative, value in files.items():
        path = output / relative
        require(path.resolve().is_relative_to(output), 'Output path escapes staging directory')
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value if isinstance(value, str) else json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def prepare(commit, output):
    cv = checkpoint(commit)
    baseline, frozen, snapshots, prior, claims = inputs(commit)
    source = census(commit, cv, snapshots['native'])
    classes = classification(frozen, cv)
    completed = set(prior['completed_feature_ids'])
    proposed = []
    for key, packet in claims.items():
        claim = packet['claim']
        status = claim.get('proposed_status', claim.get('proposedStatus'))
        proposed.append({'id': key, 'manifest': packet['manifest'], 'proposed_status': status,
                         'already_validated': key in completed, 'classification': classes[key],
                         'eligible_if_reviewed_and_validated': key not in completed and countable(key, claim, classes, baseline)})
    drafts = []
    roots = [n['id'] for n in snapshots['native']['nodes'] if n['parent_id'] is None]
    require(len(roots) == 1, 'Native inventory needs one root')
    for window in source['windows']:
        if window['navigable_node_ids']:
            continue
        key = 'proposal.window.' + window['component'].lower()
        drafts.append({'id': key, 'parent_id': roots[0], 'component': window['component'],
                       'label': window['component'], 'kind': 'dialog', 'status': 'partial',
                       'native_source': dict(window['source'], anchor_git_head=commit),
                       'assessment': 'Source census proposal only; reference mapping, control granularity and scoped runtime evidence require review.',
                       'evidence': [], 'remaining': ['Review source wiring, behavior, reference anchors and control granularity before promotion.']})
        for index, control in enumerate(window['controls']):
            if control['kind'] == 'property':
                continue
            drafts.append({'id': key + f'.control-{index:03d}', 'parent_id': key, 'label': control['name'],
                           'kind': 'control', 'status': 'partial', 'native_source': dict(control['source'], anchor_git_head=commit),
                           'assessment': 'Unreviewed lexical control proposal; source presence does not establish runtime behavior.',
                           'evidence': [], 'remaining': ['Review semantic grouping, reference equivalence and scoped regression evidence.']})
    files = {'source-census.json': source, 'claim-proposals.json': proposed,
             'native-hierarchy-proposal.json': {'baseline_git_head': commit, 'review_required': True,
                                              'native': {'additions': drafts}},
             'review-template.json': {'source_commit': commit, 'reviewed_by': '', 'patch_sha256': '',
                                     'selected_completion_ids': [], 'reviewed_assessment_ids': [], 'reviewed_native_ids': [],
                                     'reviewed_source_census_sha256': digest(source)},
             'preparation.json': {'source_commit': commit, 'prior_validated_source_commit': prior['source_commit'],
                                  'prior_completions': len(completed), 'frozen_counts': BEFORE,
                                  'exported_windows': source['exported_window_count'], 'tests_run': False,
                                  'scope': 'No canonical writes or status promotion; source inspection proposals only.'}}
    write_outputs(output, files)
    return files['preparation.json']


def publish(commit, output, patch, review, ci):
    cv = checkpoint(commit)
    validate_ci(ci, commit)
    baseline, frozen, snapshots, prior, claims = inputs(commit)
    validate_ci(prior['ci_evidence'], prior['source_commit'])
    previous_reference = {n['id']: n['status'] for n in snapshots['reference']['nodes']}
    require(prior.get('ci') == 'passed', 'Prior ledger is not validated')
    require(review.get('source_commit') == commit and str(review.get('reviewed_by', '')).strip(), 'Review lacks exact source/reviewer')
    require(review.get('patch_sha256') == digest(patch), 'Reviewed patch fingerprint differs')
    require(patch.get('baseline_git_head') == commit, 'Patch belongs to another source checkpoint')
    for anchor in cv.anchor_objects(patch):
        path = anchor.get('path', anchor.get('relativepath'))
        require(anchor.get('anchor_git_head') == commit, f'Reviewed authored anchor lacks exact checkpoint: {path}')
        line = anchor['line']
        require(isinstance(line, int) and not isinstance(line, bool) and 0 < line <= len(cv.source_lines(commit, path)),
                f'Invalid reviewed anchor: {path}:{line}')
        require(anchor.get('line_sha256') == cv.line_digest(cv.source_lines(commit, path)[line - 1]),
                f'Reviewed anchor fingerprint differs: {path}:{line}')
    source = census(commit, cv, snapshots['native'])
    require(review.get('reviewed_source_census_sha256') == digest(source), 'Reviewed source census differs')
    for name in ('reference', 'native'):
        require(not patch.get(name, {}).get('deletions'), 'Inventory IDs cannot be deleted')
    require(not patch.get('reference', {}).get('additions'), 'Original reference IDs cannot be added')
    changed = {name: {n['id'] for action in ('updates', 'additions') for n in patch.get(name, {}).get(action, [])}
               for name in ('reference', 'native')}
    for name, field in (('reference', 'reviewed_assessment_ids'), ('native', 'reviewed_native_ids')):
        ids = review.get(field, [])
        require(len(ids) == len(set(ids)) and set(ids) == changed[name], f'{name} edits differ from explicitly reviewed IDs')
    selected = review.get('selected_completion_ids', [])
    require(len(selected) == len(set(selected)), 'Duplicate selected completion IDs')
    prior_ids = prior['completed_feature_ids']
    require(len(prior_ids) == len(set(prior_ids)) == prior['concrete_implementation_completions'], 'Stale prior completion ledger')
    require(not set(selected) & set(prior_ids) and set(selected) <= changed['reference'], 'Selected IDs must be new explicitly reviewed reference updates')
    patch_report = cv.apply_patches(snapshots, [MemoryPatch(patch)])
    anchor_reports = {}
    for name in ('reference', 'native'):
        for anchor in cv.anchor_objects(snapshots[name]):
            path = anchor.get('path', anchor.get('relativepath'))
            anchor_commit = anchor.get('anchor_git_head', snapshots[name]['git_head'])
            lines = cv.source_lines(anchor_commit, path)
            line = anchor['line']
            require(isinstance(line, int) and not isinstance(line, bool) and 0 < line <= len(lines),
                    f'Invalid inherited anchor: {path}:{line}')
            require(anchor.get('line_sha256') == cv.line_digest(lines[line - 1]),
                    f'Inherited anchor fingerprint differs: {path}:{line}')
        snapshots[name], anchor_reports[name] = cv.repin(snapshots[name], commit)
        cv.refresh_aggregates(snapshots[name], name)
    cv.refresh_mappings(snapshots)
    reference = {n['id']: n for n in snapshots['reference']['nodes']}
    require(set(reference) == set(baseline['reference']), 'Original IDs changed after patch')
    classes = classification(frozen, cv)
    proposals = {row['id']: dict(row) for row in prior['proposals']}
    require(len(proposals) == len(prior['proposals']), 'Duplicate prior proposal IDs')
    for key in prior_ids:
        require(key in proposals and classes[key] == 'concrete_leaf' and reference[key]['status'] == 'first_pass'
                and baseline['reference'][key] in {'missing', 'partial'}
                and proposals[key].get('change_kind') in {'implementation', 'new_behavior'}
                and proposals[key].get('count_as_completed_leaf') is not False, f'Prior completion regressed/inflated: {key}')
    for key in selected:
        require(key in claims, f'Selected ID has no author manifest: {key}')
        claim = claims[key]['claim']
        status = claim.get('proposed_status', claim.get('proposedStatus'))
        require(countable(key, claim, classes, baseline) and status == reference[key]['status'],
                f'Non-countable selected completion: {key}')
    # A changed original first-pass assessment cannot silently enter the ledger.
    for key in changed['reference']:
        if (classes[key] == 'concrete_leaf' and key not in prior_ids and previous_reference[key] != 'first_pass'
                and baseline['reference'][key] in {'missing', 'partial'}
                and reference[key]['status'] == 'first_pass'):
            claim = claims.get(key, {}).get('claim', {})
            is_completion_claim = claim.get('change_kind', 'implementation') in {'implementation', 'new_behavior'} and claim.get('countAsCompletedLeaf') is not False
            require(not is_completion_claim or key in selected, f'Unselected concrete first-pass promotion: {key}')
    for key in changed['reference']:
        claim = claims.get(key, {}).get('claim', {})
        proposals[key] = {'id': key, 'label': reference[key]['label'], 'before': baseline['reference'][key],
                          'after': reference[key]['status'], 'classification': classes[key],
                          'manifest': claims.get(key, {}).get('manifest', 'reviewed assessment'),
                          'change_kind': claim.get('change_kind', 'implementation') if claim else proposals.get(key, {}).get('change_kind', 'assessment'),
                          'count_as_completed_leaf': key in selected or key in prior_ids,
                          'validated_in_current_checkpoint': True}
    completed_ids = sorted(set(prior_ids) | set(selected))
    now = datetime.now(timezone.utc).isoformat()
    progress = dict(prior)
    completed_rows = [proposals[k] for k in completed_ids]
    progress.update(source_commit=commit, ci='passed', ci_evidence=ci, completed_at_utc=now,
                    milestone=len(completed_ids), concrete_implementation_completions=len(completed_ids),
                    completed_feature_ids=completed_ids, before=BEFORE,
                    after=dict(Counter(n['status'] for n in reference.values())), proposals=list(proposals.values()),
                    prior_validated_source_commit=prior['source_commit'], prior_validated_ci_evidence=prior['ci_evidence'],
                    additional_completed_feature_ids=sorted(selected), additional_concrete_implementation_completions=len(selected),
                    proposed_counts={'additional_original_leaf_completions': len(selected),
                                     'validated_cumulative_original_leaf_completions': len(completed_ids)},
                    completions_by_previous_status=dict(Counter(r['before'] for r in completed_rows)),
                    completions_by_manifest=dict(Counter(Path(r['manifest']).stem for r in completed_rows)),
                    first_pass_parent_or_alias_count=sum(r['classification'] == 'parent_or_alias' and r['after'] == 'first_pass' for r in proposals.values()),
                    first_pass_evidence_only_count=sum(r['change_kind'].startswith('evidence') and r['after'] == 'first_pass' for r in proposals.values()),
                    counting_method='Frozen 1,812 original IDs; explicit reviewed concrete implementation leaves only; parents, aliases, evidence-only changes and native IDs excluded.',
                    validation_summary='Exact successful hosted check, Windows, macOS and parity-models jobs. Source inspection and author reference-recording provenance are separate; no local Cargo or mutation testing by this tool.')
    progress['reference_recording_provenance'] = list(prior.get('reference_recording_provenance', [])) + [
        {'id': key, 'manifest': claims[key]['manifest'], 'source_commit': commit,
         'author_reference_recording': claims[key]['claim'].get('reference_recording', claims[key]['claim'].get('referenceRecording', claims[key]['claim'].get('reference_evidence'))),
         'scope': 'Author declaration; independently of hosted Rust CI.'} for key in selected]
    validations = {}
    for name, data in snapshots.items():
        data['generated_at_utc'] = now
        if 'generated_at' in data:
            data['generated_at'] = now
        data['runtime_validation'] = ci
        data['tests_run'] = True
        data['tests_scope'] = 'Hosted CI at exact source; scoped per-node evidence and limits apply. Historical source-only inspection is not runtime proof.'
        if name == 'native':
            data['caveats'] = [re.sub(r'All \d+ exported Slint Window components',
                f'All {source["exported_window_count"]} exported Slint Window components', caveat)
                for caveat in data.get('caveats', [])]
        curation = data.setdefault('curation', {})
        curation['audit_documents'] = list(dict.fromkeys(curation.get('audit_documents', []) + [
            f'{DATA}/audit/anchor-remap-{commit[:8]}-publication.json', f'{DATA}/overnight/progress.json',
            f'{DATA}/overnight/reviewed-patch.json', f'{DATA}/overnight/ci-evidence.json']))
        data['overnight_progress'] = {k: progress[k] for k in ('goal', 'milestone', 'concrete_implementation_completions',
            'completed_feature_ids', 'before', 'after', 'validation_summary', 'source_commit', 'ci_evidence',
            'additional_concrete_implementation_completions', 'counting_method')}
        for node in data['nodes']:
            if node['id'] in changed[name] and 'implementation_validation' in node:
                node['implementation_validation'].update(ci='passed', source_commit=commit, run_url=ci['url'], tests_executed_at_checkpoint=True)
        validations[name] = cv.validate(data, name)
        require(not validations[name]['audit']['weak_first_pass'], f'{name}: weak first-pass assessment')
    audit = validations['native']['audit']
    require(audit['exported_window_count'] == source['exported_window_count'] and not audit['opaque_native_windows']
            and not audit['unassessed_nodes'] and not audit['catch_all_nodes'], 'Native census is incomplete/opaque')
    # Replay every node difference from the original source. New native nodes
    # stay separate from original reference completion accounting.
    resolved = {'baseline_git_head': commit, 'ci_evidence': ci, 'method': 'Reviewed cumulative replay from frozen original inventories.'}
    replay = {name: read(FROZEN, f'{DATA}/{name}-inventory.json') for name in ('reference', 'native')}
    for name, data in snapshots.items():
        old = {n['id']: n for n in replay[name]['nodes']}
        require(set(old) <= {n['id'] for n in data['nodes']}, 'Frozen native/reference ID was deleted')
        resolved[name] = {'updates': [], 'additions': []}
        for node in data['nodes']:
            if node != old.get(node['id']):
                value = json.loads(json.dumps(node))
                for anchor in cv.anchor_objects(value):
                    anchor['anchor_git_head'] = commit
                resolved[name]['updates' if node['id'] in old else 'additions'].append(value)
    cv.apply_patches(replay, [MemoryPatch(resolved)])
    for name in ('reference', 'native'):
        replay[name], _ = cv.repin(replay[name], commit)
        cv.refresh_aggregates(replay[name], name)
    cv.refresh_mappings(replay)
    for name in ('reference', 'native'):
        require({n['id']: n for n in replay[name]['nodes']} == {n['id']: n for n in snapshots[name]['nodes']}, f'{name}: cumulative patch replay differs')
    cv.DATA = Template(commit)
    html = cv.render(snapshots)
    audit_report = {'source_commit': commit, 'frozen_source_commit': FROZEN, 'patches': patch_report,
                    'inventories': anchor_reports, 'validations': validations, 'review': review,
                    'input_fingerprints': {'patch': digest(patch), 'review': digest(review), 'ci': digest(ci),
                                           'source_census': digest(source), 'prior_ledger': digest(prior)}}
    files = {f'{DATA}/{name}-inventory.json': data for name, data in snapshots.items()}
    files.update({f'{DATA}/overnight/progress.json': progress, f'{DATA}/overnight/reviewed-patch.json': resolved,
                  f'{DATA}/overnight/ci-evidence.json': ci, f'{DATA}/audit/anchor-remap-{commit[:8]}-publication.json': audit_report,
                  'source-census.json': source, 'docs/rust/gui-progress.html': html,
                  'publication-summary.json': {'source_commit': commit, 'prior_completions': len(prior_ids),
                     'additional_completions': len(selected), 'validated_original_leaf_completions': len(completed_ids),
                     'reference_statuses': progress['after'], 'native_entries': len(snapshots['native']['nodes']),
                     'exported_windows': source['exported_window_count'], 'ci_url': ci['url'],
                     'scope': 'Staged outside canonical files; root reviews/copies, updates narrative documents, browser-checks and commits publication.'}})
    write_outputs(output, files)
    return files['publication-summary.json']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('prepare', 'publish'))
    parser.add_argument('--source', required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--patch', type=Path)
    parser.add_argument('--review', type=Path)
    parser.add_argument('--ci-evidence', type=Path)
    args = parser.parse_args()
    if args.mode == 'prepare':
        require(not any((args.patch, args.review, args.ci_evidence)), 'Preparation takes no validation/publication inputs')
        result = prepare(args.source, args.out)
    else:
        require(all((args.patch, args.review, args.ci_evidence)), 'Publication requires patch, review and exact CI evidence')
        result = publish(args.source, args.out, *(json.loads(p.read_text()) for p in (args.patch, args.review, args.ci_evidence)))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f'Coverage publication blocked: {error}') from error
