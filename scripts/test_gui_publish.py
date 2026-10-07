#!/usr/bin/env python3
"""Fast publication safety regressions; no Cargo, network or canonical writes."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch as mock_patch

import gui_publish as publication


class CIScopeTests(unittest.TestCase):
    commit = '1' * 40
    policy = {'schema_version': 1, 'publication_scope': 'linux',
              'required_platforms': ['linux'], 'deferred_platforms': ['windows', 'macos'],
              'effective_date': '2026-10-06', 'reason': 'Owner-authorized Linux delivery.'}

    def evidence(self, scope=None):
        jobs = publication.JOBS
        if scope == 'linux':
            jobs = {name: jobs[name] for name in ('check', 'parity-models')}
        ci = {'source_commit': self.commit, 'head_sha': self.commit, 'run_id': 123,
              'url': 'https://github.com/jkendall327/hydrus-mine/actions/runs/123',
              'status': 'completed', 'conclusion': 'success', 'synthetic_unit_test': True,
              'jobs': [{'name': name, 'run_id': 123, 'head_sha': self.commit,
                        'status': 'completed', 'conclusion': 'success',
                        'steps': [{'name': step, 'status': 'completed', 'conclusion': 'success'}
                                  for step in sorted(steps)]} for name, steps in jobs.items()]}
        if scope is not None:
            ci['validation_scope'] = scope
        if scope == 'linux':
            ci.update(required_platforms=['linux'], deferred_platforms=['windows', 'macos'])
        return ci

    def test_omitted_or_explicit_cross_platform_scope_keeps_four_jobs(self):
        for scope in (None, 'cross-platform'):
            ci = self.evidence(scope)
            with self.subTest(scope=scope), mock_patch.object(publication, 'git') as git:
                validation = publication.validate_ci(ci, self.commit)
                git.assert_not_called()
                self.assertEqual(validation['required_platforms'], ['linux', 'windows', 'macos'])
                self.assertEqual(validation['deferred_platforms'], [])
                self.assertEqual(validation['validation_scope'], 'cross-platform')
                for deferred_job in ('other-platforms (windows-latest)', 'other-platforms (macos-latest)'):
                    bad = copy.deepcopy(ci)
                    bad['jobs'] = [job for job in bad['jobs'] if job['name'] != deferred_job]
                    with self.subTest(missing=deferred_job), self.assertRaisesRegex(ValueError, 'job set'):
                        publication.validate_ci(bad, self.commit)

    def test_linux_policy_is_read_at_exact_source_and_recorded(self):
        text = json.dumps(self.policy, indent=2) + '\n'
        ci = self.evidence('linux')
        before = copy.deepcopy(ci)
        with mock_patch.object(publication, 'git', return_value=text) as git:
            validation = publication.validate_ci(ci, self.commit)
        git.assert_called_once_with('show', f'{self.commit}:.github/publication-validation.json')
        self.assertEqual(ci, before, 'Validation must not rewrite evidence or prior ledger objects')
        self.assertEqual(validation['required_platforms'], ['linux'])
        self.assertEqual(validation['deferred_platforms'], ['windows', 'macos'])
        self.assertEqual(validation['publication_policy']['source_commit'], self.commit)
        self.assertEqual(validation['publication_policy']['sha256'], hashlib.sha256(text.encode()).hexdigest())
        self.assertIn('Windows and macOS validation is deferred', publication.validation_summary(validation))

    def test_linux_requires_complete_owner_policy(self):
        invalid = [None, [], {}, dict(self.policy, schema_version=True),
                   dict(self.policy, schema_version=2), dict(self.policy, publication_scope='cross-platform'),
                   dict(self.policy, required_platforms=['linux', 'windows']),
                   dict(self.policy, deferred_platforms=['macos', 'windows']),
                   dict(self.policy, effective_date='2026-10-05'), dict(self.policy, reason='  ')]
        for policy in invalid:
            with self.subTest(policy=policy), mock_patch.object(publication, 'git', return_value=json.dumps(policy)), self.assertRaises(ValueError):
                publication.validate_ci(self.evidence('linux'), self.commit)
        for failure in ('not json', subprocess.CalledProcessError(128, ['git', 'show'])):
            kwargs = {'side_effect': failure} if isinstance(failure, Exception) else {'return_value': failure}
            with self.subTest(failure=str(failure)), mock_patch.object(publication, 'git', **kwargs), self.assertRaisesRegex(ValueError, 'exact source commit'):
                publication.validate_ci(self.evidence('linux'), self.commit)

    def test_linux_requires_every_successful_step_without_duplicates(self):
        for job_index, job in enumerate(self.evidence('linux')['jobs']):
            for step_index, step in enumerate(job['steps']):
                for fault in ('missing', 'failure', 'skipped', 'running', 'duplicate'):
                    ci = self.evidence('linux')
                    records = ci['jobs'][job_index]['steps']
                    if fault == 'missing':
                        records.pop(step_index)
                    elif fault == 'duplicate':
                        failed = dict(records[step_index], conclusion='failure')
                        records.insert(step_index, failed)
                    elif fault == 'running':
                        records[step_index]['status'] = 'in_progress'
                    else:
                        records[step_index]['conclusion'] = fault
                    with self.subTest(job=job['name'], step=step['name'], fault=fault), mock_patch.object(publication, 'git', return_value=json.dumps(self.policy)), self.assertRaises(ValueError):
                        publication.validate_ci(ci, self.commit)

    def test_linux_keeps_exact_source_run_and_job_gates(self):
        for fault in ('head', 'source', 'job_head', 'job_head_missing', 'job_run', 'run_bool', 'url',
                      'job_failed', 'job_missing', 'job_duplicate', 'run_failed', 'run_running'):
            ci = self.evidence('linux')
            if fault == 'head':
                ci['head_sha'] = '0' * 40
            elif fault == 'source':
                ci['source_commit'] = '0' * 40
            elif fault == 'job_head':
                ci['jobs'][0]['head_sha'] = '0' * 40
            elif fault == 'job_head_missing':
                del ci['jobs'][0]['head_sha']
            elif fault == 'job_run':
                ci['jobs'][0]['run_id'] += 1
            elif fault == 'run_bool':
                ci['run_id'] = True
            elif fault == 'url':
                ci['url'] += '/different'
            elif fault == 'job_failed':
                ci['jobs'][0]['conclusion'] = 'failure'
            elif fault == 'job_missing':
                ci['jobs'].pop()
            elif fault == 'job_duplicate':
                ci['jobs'][1] = copy.deepcopy(ci['jobs'][0])
            elif fault == 'run_failed':
                ci['conclusion'] = 'failure'
            else:
                ci['status'] = 'in_progress'
            with self.subTest(fault=fault), mock_patch.object(publication, 'git', return_value=json.dumps(self.policy)), self.assertRaises(ValueError):
                publication.validate_ci(ci, self.commit)

    def test_unknown_scope_and_inaccurate_platform_claims_reject(self):
        for scope in ('windows', 'crossplatform', '', False):
            with self.subTest(scope=scope), self.assertRaisesRegex(ValueError, 'Unknown CI validation scope'):
                publication.validate_ci(self.evidence(scope), self.commit)
        ci = self.evidence()
        ci['validation_scope'] = None
        with self.assertRaisesRegex(ValueError, 'Unknown CI validation scope'):
            publication.validate_ci(ci, self.commit)
        for scope in ('linux', 'cross-platform'):
            for field in ('required_platforms', 'deferred_platforms'):
                ci = self.evidence(scope)
                ci[field] = ['linux', 'windows', 'macos'] if field == 'required_platforms' else []
                if scope == 'cross-platform':
                    ci[field] = ['linux']
                with self.subTest(scope=scope, field=field), mock_patch.object(publication, 'git', return_value=json.dumps(self.policy)), self.assertRaises(ValueError):
                    publication.validate_ci(ci, self.commit)
        for field in ('required_platforms', 'deferred_platforms'):
            ci = self.evidence('linux')
            del ci[field]
            with self.subTest(absent=field), mock_patch.object(publication, 'git', return_value=json.dumps(self.policy)), self.assertRaises(ValueError):
                publication.validate_ci(ci, self.commit)


class PublicationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.commit = publication.git('rev-parse', 'HEAD').strip()
        cls.cv = publication.checkpoint(cls.commit)
        cls.baseline, cls.frozen, cls.snapshots, cls.prior, cls.claims = publication.inputs(cls.commit)

    def test_ci_requires_exact_sha_all_jobs_and_successful_steps(self):
        ci = self.prior['ci_evidence']
        publication.validate_ci(ci, self.prior['source_commit'])
        for mutation in ('head', 'job', 'step', 'run'):
            bad = copy.deepcopy(ci)
            if mutation == 'head':
                bad['head_sha'] = '0' * 40
            elif mutation == 'job':
                bad['jobs'].pop()
            elif mutation == 'step':
                bad['jobs'][0]['steps'] = []
            else:
                bad['jobs'][0]['run_id'] += 1
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                publication.validate_ci(bad, self.prior['source_commit'])

    def test_historical_ci_cannot_be_downgraded_by_current_worktree_policy(self):
        # Freeze the last checkpoint before Linux-only publication was allowed.
        # The current ledger advances and legitimately contains Linux evidence.
        historical = publication.read(
            '7f8f4f1cf0827b1336aa278fbfb01794f057e4bc',
            f'{publication.DATA}/overnight/progress.json')
        ci = copy.deepcopy(historical['ci_evidence'])
        ci.update(validation_scope='linux', required_platforms=['linux'],
                  deferred_platforms=['windows', 'macos'])
        ci['jobs'] = [job for job in ci['jobs'] if job['name'] in ('check', 'parity-models')]
        with self.assertRaisesRegex(ValueError, 'exact source commit'):
            publication.validate_ci(ci, historical['source_commit'])
        publication.validate_ci(historical['ci_evidence'], historical['source_commit'])

    def test_original_frozen_leaves_only(self):
        classes = {'leaf': 'concrete_leaf', 'parent': 'parent_or_alias'}
        baseline = {'reference': {'leaf': 'missing', 'parent': 'missing', 'green': 'first_pass'}}
        claim = {'proposedStatus': 'first_pass'}
        self.assertTrue(publication.countable('leaf', claim, classes, baseline))
        self.assertFalse(publication.countable('parent', claim, classes, baseline))
        self.assertFalse(publication.countable('new-native-id', claim, classes, baseline))
        for excluded in ({'change_kind': 'evidence'}, {'countAsCompletedLeaf': False}, {'proposedStatus': 'partial'}):
            self.assertFalse(publication.countable('leaf', dict(claim, **excluded), classes, baseline))
        self.assertEqual(len(self.baseline['reference']), 1812)
        self.assertEqual(publication.BEFORE, self.prior['before'])

    def test_nonempty_selected_publication_stages_and_enforces_selection_gates(self):
        # Synthetic unit fixture: remove one genuinely reviewed historical leaf
        # from an in-memory prior ledger, then republish it. The real historical
        # inventories, classifier, anchors, replay and HTML renderer exercise the
        # whole staging path; no synthetic CI is written to canonical files.
        commit = '2388372db1fc45681e16155ed6ab38c620493052'
        cv = publication.checkpoint(commit)
        baseline, frozen, snapshots, prior, claims = publication.inputs(commit)
        classes = publication.classification(frozen, cv)
        selected = next(key for key in prior['completed_feature_ids']
                        if key in claims and publication.countable(key, claims[key]['claim'], classes, baseline))
        node = copy.deepcopy(next(n for n in snapshots['reference']['nodes'] if n['id'] == selected))
        for anchor in cv.anchor_objects(node):
            path = anchor.get('path', anchor.get('relativepath'))
            line, _ = cv.remap_line(anchor.get('anchor_git_head', snapshots['reference']['git_head']),
                                    commit, path, anchor['line'])
            anchor.update(line=line, anchor_git_head=commit,
                          line_sha256=cv.line_digest(cv.source_lines(commit, path)[line - 1]))
        fixture = copy.deepcopy((baseline, frozen, snapshots, prior, claims))
        fixture[3]['completed_feature_ids'].remove(selected)
        fixture[3]['concrete_implementation_completions'] -= 1
        fixture[3]['proposals'] = [row for row in fixture[3]['proposals'] if row['id'] != selected]
        next(n for n in fixture[2]['reference']['nodes'] if n['id'] == selected)['status'] = baseline['reference'][selected]
        patch = {'baseline_git_head': commit, 'reference': {'updates': [node]}}
        review = {'source_commit': commit, 'reviewed_by': 'SYNTHETIC UNIT TEST ONLY',
                  'patch_sha256': publication.digest(patch), 'reviewed_assessment_ids': [selected],
                  'reviewed_native_ids': [], 'selected_completion_ids': [selected],
                  'reviewed_source_census_sha256': publication.digest(publication.census(commit, cv, snapshots['native']))}
        ci = copy.deepcopy(prior['ci_evidence'])
        ci.update(source_commit=commit, head_sha=commit, synthetic_unit_test=True,
                  scope='Synthetic unit-test CI attestation; not hosted evidence for this SHA.')
        for job in ci['jobs']:
            job['head_sha'] = commit
        canonical_digest = publication.digest(publication.read(commit, f'{publication.DATA}/overnight/progress.json'))
        with tempfile.TemporaryDirectory(prefix='gui-publication-synthetic-test-') as directory:
            with mock_patch.object(publication, 'inputs', side_effect=lambda _: copy.deepcopy(fixture)):
                out = Path(directory) / 'accepted'
                summary = publication.publish(commit, out, patch, review, ci)
                progress = json.loads((out / publication.DATA / 'overnight/progress.json').read_text())
                self.assertEqual(summary['additional_completions'], 1)
                self.assertEqual(summary['validated_original_leaf_completions'], len(prior['completed_feature_ids']))
                self.assertEqual(progress['additional_completed_feature_ids'], [selected])
                self.assertEqual(len(progress['completed_feature_ids']), len(set(progress['completed_feature_ids'])))
                self.assertEqual(progress['before'], publication.BEFORE)
                self.assertTrue(progress['ci_evidence']['synthetic_unit_test'])
                self.assertEqual(summary['validation_scope'], 'cross-platform')
                self.assertEqual(progress['required_platforms'], ['linux', 'windows', 'macos'])
                self.assertEqual(progress['deferred_platforms'], [])
                self.assertEqual(progress['prior_validated_ci_evidence'], prior['ci_evidence'])
                reference = json.loads((out / publication.DATA / 'reference-inventory.json').read_text())
                self.assertEqual({n['id'] for n in reference['nodes']}, set(baseline['reference']))
                self.assertEqual(next(n['status'] for n in reference['nodes'] if n['id'] == selected), 'first_pass')
                self.assertTrue((out / 'docs/rust/gui-progress.html').is_file())
                # Mock only the exact immutable source policy for this synthetic
                # staging case; the real historical checkpoint has no Linux policy.
                linux_ci = copy.deepcopy(ci)
                linux_ci.update(validation_scope='linux', required_platforms=['linux'],
                                deferred_platforms=['windows', 'macos'])
                linux_ci['jobs'] = [job for job in linux_ci['jobs'] if job['name'] in ('check', 'parity-models')]
                original_git = publication.git
                def source_policy(*args):
                    if args == ('show', f'{commit}:{publication.PUBLICATION_POLICY}'):
                        return json.dumps(CIScopeTests.policy)
                    return original_git(*args)
                linux_out = Path(directory) / 'accepted-linux'
                with mock_patch.object(publication, 'git', side_effect=source_policy):
                    linux_summary = publication.publish(commit, linux_out, patch, review, linux_ci)
                linux_progress = json.loads((linux_out / publication.DATA / 'overnight/progress.json').read_text())
                self.assertEqual(linux_summary['validation_scope'], 'linux')
                self.assertEqual(linux_summary['required_platforms'], ['linux'])
                self.assertEqual(linux_summary['deferred_platforms'], ['windows', 'macos'])
                self.assertEqual(linux_progress['prior_validated_ci_evidence'], prior['ci_evidence'])
                self.assertEqual(linux_progress['completed_feature_ids'], progress['completed_feature_ids'])
                self.assertIn('Windows and macOS validation is deferred', linux_progress['validation_summary'])
                self.assertEqual(linux_progress['ci_evidence']['publication_policy']['source_commit'], commit)
                for name in ('reference', 'native'):
                    data = json.loads((linux_out / publication.DATA / f'{name}-inventory.json').read_text())
                    self.assertEqual(data['overnight_progress']['required_platforms'], ['linux'])
                    self.assertEqual(data['runtime_validation']['deferred_platforms'], ['windows', 'macos'])
                    self.assertIn('deferred platforms: windows, macos', data['tests_scope'])
                messages = {'unselected': 'Unselected concrete first-pass promotion',
                            'duplicate': 'Duplicate selected completion IDs',
                            'noncountable': 'Non-countable selected completion',
                            'wrong_source': 'CI evidence must name the exact source SHA'}
                for gate in ('unselected', 'duplicate', 'noncountable', 'wrong_source'):
                    bad_review, bad_ci = copy.deepcopy(review), copy.deepcopy(ci)
                    if gate == 'unselected':
                        bad_review['selected_completion_ids'] = []
                    elif gate == 'duplicate':
                        bad_review['selected_completion_ids'].append(selected)
                    elif gate == 'noncountable':
                        fixture[4][selected]['claim']['countAsCompletedLeaf'] = False
                    else:
                        bad_ci['head_sha'] = '0' * 40
                    rejected = Path(directory) / gate
                    with self.subTest(gate=gate), self.assertRaisesRegex(ValueError, messages[gate]):
                        publication.publish(commit, rejected, patch, bad_review, bad_ci)
                    self.assertFalse(rejected.exists())
                    fixture[4][selected]['claim'].pop('countAsCompletedLeaf', None)
        self.assertEqual(canonical_digest, publication.digest(publication.read(commit,
                         f'{publication.DATA}/overnight/progress.json')))

    def test_source_mask_preserves_following_code_after_escaped_newline(self):
        source = 'Text { text: "question \\\n continued { quoted }"; }\ncallback answer();'
        masked = publication.masked(source)
        self.assertEqual(len(source), len(masked))
        self.assertEqual(source.count('\n'), masked.count('\n'))
        self.assertIn('callback answer();', masked)
        self.assertNotIn('quoted', masked)

    def test_census_matches_source_audit_and_finds_shared_star_controls(self):
        source = publication.census(self.commit, self.cv, self.snapshots['native'])
        pinned = copy.deepcopy(self.snapshots['native'])
        # Census is source-only; the audit reads this commit without pretending
        # the inherited inventory's anchors were repinned or behavior validated.
        pinned['git_head'] = self.commit
        actual = self.cv.audit_diagnostics(pinned, 'native')
        self.assertEqual(source['exported_window_count'], actual['exported_window_count'])
        self.assertEqual({(w['component'], w['path']) for w in source['windows']},
                         {(w['component'], w['path']) for w in actual['opaque_native_windows']} |
                         {(w['component'], w['path']) for w in source['windows'] if w['navigable_node_ids']})
        controls = [c for record in source['shared_components'] + source['windows'] for c in record['controls']]
        self.assertTrue(any(c['name'] == '★' for c in controls))
        self.assertTrue(any(c['name'] == 'defaults-action' for c in controls))
        self.assertTrue(any('tab-chosen' == c['name'] for c in controls))
        for control in controls:
            anchor = control['source']
            self.assertEqual(anchor['line_sha256'], self.cv.line_digest(
                self.cv.source_lines(self.commit, anchor['path'])[anchor['line'] - 1]))

    def test_failed_publication_leaves_no_output(self):
        with tempfile.TemporaryDirectory(prefix='gui-publication-test-') as directory:
            out = Path(directory) / 'staged'
            with self.assertRaises(ValueError):
                publication.publish(self.commit, out, {}, {}, self.prior['ci_evidence'])
            self.assertFalse(out.exists())

    def test_reviewed_changed_anchor_requires_exact_fingerprint(self):
        # Synthetic metadata exercises the local gate, never attests hosted CI.
        ci = copy.deepcopy(self.prior['ci_evidence'])
        ci.update(source_commit=self.commit, head_sha=self.commit)
        for job in ci['jobs']:
            job['head_sha'] = self.commit
        node = self.snapshots['native']['nodes'][0]
        patch = {'baseline_git_head': self.commit, 'native': {'updates': [{
            'id': node['id'], 'native_source': {'path': 'crates/hydrus-gui/ui/main.slint',
            'line': 1, 'line_sha256': '0' * 64, 'anchor_git_head': self.commit}}]}}
        review = {'source_commit': self.commit, 'reviewed_by': 'isolated gate regression',
                  'patch_sha256': publication.digest(patch)}
        with tempfile.TemporaryDirectory(prefix='gui-publication-test-') as directory:
            out = Path(directory) / 'staged'
            with self.assertRaisesRegex(ValueError, 'Reviewed anchor fingerprint differs'):
                publication.publish(self.commit, out, patch, review, ci)
            self.assertFalse(out.exists())

    def test_prepare_is_unreviewed_and_does_not_promote_native_controls(self):
        canonical = {name: publication.digest(data) for name, data in self.snapshots.items()}
        with tempfile.TemporaryDirectory(prefix='gui-publication-test-') as directory:
            out = Path(directory) / 'review'
            summary = publication.prepare(self.commit, out)
            self.assertFalse(summary['tests_run'])
            proposal = json.loads((out / 'native-hierarchy-proposal.json').read_text())
            self.assertTrue(proposal['review_required'])
            self.assertTrue(all(n['status'] == 'partial' for n in proposal['native']['additions']))
            for name, expected in canonical.items():
                self.assertEqual(expected, publication.digest(publication.read(self.commit,
                    f'{publication.DATA}/{name}-inventory.json')))
            with self.assertRaises(ValueError):
                publication.prepare(self.commit, out)


if __name__ == '__main__':
    unittest.main()
