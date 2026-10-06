#!/usr/bin/env python3
"""Artifact/replay safety tests: no Cargo, compiler, GUI or network processes."""
import copy
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch

import gui_test_artifact as guard


def fake_process(lines, code=0):
    process = MagicMock()
    process.__enter__.return_value = process
    process.stdout = io.StringIO(lines)
    process.wait.return_value = code
    return process


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.manifest = self.root / 'crates/hydrus-gui/Cargo.toml'
        self.manifest.parent.mkdir(parents=True)
        self.manifest.write_text('[package]\nname="hydrus-gui"\n', encoding='utf-8')
        self.executable = self.root / 'target/debug/deps/gui.exe'
        self.executable.parent.mkdir(parents=True)
        self.executable.write_bytes(b'Synthetic executable fixture; never executed.')
        self.sha = 'a' * 40
        # Cargo's real modern ID can omit the package name after the #.
        self.package_id = self.manifest.parent.as_uri() + '#0.1.0'
        self.artifact = {'reason': 'compiler-artifact', 'package_id': self.package_id,
                         'manifest_path': str(self.manifest),
                         'target': {'name': 'gui', 'kind': ['test']},
                         'profile': {'test': True}, 'features': [],
                         'executable': str(self.executable), 'fresh': True}
        self.proof_path = self.root / 'target/ci/windows-gui-artifact.json'
        self.result_path = self.root / 'target/ci/windows-gui-replay.json'
        self.proof = {'schema': 1, 'status': 'ready', 'source_commit': self.sha,
                      'build_exit_code': 0, 'build_command': guard.BUILD,
                      'test_filter': guard.FILTER, 'package_id': self.package_id,
                      'package_name': 'hydrus-gui', 'package_version': '0.1.0',
                      'manifest_path': str(self.manifest), 'artifact': self.artifact,
                      'executable': str(self.executable),
                      'executable_sha256': guard.digest(self.executable),
                      'dll_directories': [str(self.executable.parent),
                                          str(self.executable.parent.parent)]}
        guard.write_json(self.proof_path, self.proof)

    def test_selection_uses_exact_metadata_id_target_kind_profile_and_executable(self):
        decoys = []
        for mutate in ('package', 'name', 'kind', 'profile', 'missing_executable'):
            artifact = copy.deepcopy(self.artifact)
            if mutate == 'package':
                artifact['package_id'] += '-different'
            elif mutate == 'name':
                artifact['target']['name'] = 'other_test'
            elif mutate == 'kind':
                artifact['target']['kind'] = ['lib']
            elif mutate == 'profile':
                artifact['profile']['test'] = False
            else:
                artifact['executable'] = None
            decoys.append(artifact)
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                guard.select_artifact([artifact], self.package_id)
        self.assertEqual(guard.select_artifact([*decoys, self.artifact], self.package_id),
                         self.artifact)

    def test_duplicate_identical_executable_is_ok_but_missing_or_ambiguous_is_not(self):
        self.assertEqual(guard.select_artifact([self.artifact, self.artifact], self.package_id),
                         self.artifact)
        other = dict(self.artifact, executable=str(self.executable) + '-other')
        for artifacts in ([], [self.artifact, other]):
            with self.subTest(artifacts=artifacts), self.assertRaises(ValueError):
                guard.select_artifact(artifacts, self.package_id)

    def test_build_stream_keeps_json_separate_and_displays_rendered_diagnostic(self):
        messages = [{'reason': 'compiler-message', 'message': {'rendered': 'error: useful diagnostic\n'}},
                    self.artifact, {'reason': 'build-finished', 'success': True}]
        lines = ''.join(json.dumps(message) + '\n' for message in messages)
        path = self.root / 'target/ci/windows-build.jsonl'
        with patch.object(guard.subprocess, 'Popen', return_value=fake_process(lines)) as launch:
            with patch('sys.stdout', new_callable=io.StringIO) as human:
                code, actual = guard.capture_build(self.root, path)
        self.assertEqual(code, 0)
        self.assertEqual(actual, messages)
        self.assertEqual(path.read_text(encoding='utf-8'), lines)
        self.assertIn('useful diagnostic', human.getvalue())
        self.assertEqual(launch.call_args.args[0],
                         ['cargo', 'build', '--workspace', '--all-targets', '--locked',
                          '--keep-going', '--message-format=json'])
        self.assertNotIn('stderr', launch.call_args.kwargs)  # inherit human Cargo stderr

    def test_build_failure_preserves_cargo_exit_despite_incomplete_json(self):
        metadata = {'packages': [{'name': 'hydrus-gui', 'id': self.package_id,
                                   'manifest_path': str(self.manifest), 'version': '0.1.0'}],
                    'target_directory': str(self.root / 'target')}
        with patch.object(guard, 'verify_source'), \
                patch.object(guard.subprocess, 'check_output', return_value=json.dumps(metadata)) as discover, \
                patch.object(guard.subprocess, 'Popen', return_value=fake_process('partial output\n', 101)):
            code = guard.prepare(self.root, self.sha, self.proof_path,
                                 self.root / 'target/ci/windows-build.jsonl')
        self.assertEqual(code, 101)
        proof = json.loads(self.proof_path.read_text(encoding='utf-8'))
        self.assertEqual(proof['status'], 'build_failed')
        self.assertEqual(proof['build_exit_code'], 101)
        self.assertNotIn('executable', proof)  # previous ready proof was replaced
        discover.assert_called_once()  # no rustc discovery after Cargo failure

    def test_successful_build_records_exact_source_artifact_digest_and_runtime_paths(self):
        metadata = {'packages': [{'name': 'hydrus-gui', 'id': self.package_id,
                                   'manifest_path': str(self.manifest), 'version': '0.1.0'}],
                    'target_directory': str(self.root / 'target')}
        lines = json.dumps(self.artifact) + '\n' + json.dumps(
            {'reason': 'build-finished', 'success': True}) + '\n'
        rust_libdir = self.root / 'rust-sysroot/lib'
        with patch.object(guard, 'verify_source') as source, \
                patch.object(guard.subprocess, 'check_output',
                             side_effect=[json.dumps(metadata), str(rust_libdir) + '\n']) as discover, \
                patch.object(guard.subprocess, 'Popen', return_value=fake_process(lines)), \
                patch('sys.stdout', new_callable=io.StringIO):
            self.assertEqual(guard.prepare(self.root, self.sha, self.proof_path,
                                          self.root / 'target/ci/windows-build.jsonl'), 0)
        self.assertEqual(source.call_count, 2)  # verify before and after compiling
        self.assertEqual(discover.call_count, 2)  # metadata and rustc path outside guard
        proof = json.loads(self.proof_path.read_text(encoding='utf-8'))
        self.assertEqual(proof['status'], 'ready')
        self.assertEqual(proof['source_commit'], self.sha)
        self.assertEqual(proof['package_id'], self.package_id)
        self.assertEqual(proof['artifact'], self.artifact)
        self.assertEqual(proof['executable_sha256'], guard.digest(self.executable))
        self.assertEqual(proof['cargo_messages_sha256'],
                         guard.digest(self.root / 'target/ci/windows-build.jsonl'))
        self.assertIn(str(rust_libdir), proof['dll_directories'])

    def test_zero_exit_requires_valid_json_and_successful_build_finished(self):
        streams = ('not-json\n', json.dumps(self.artifact) + '\n',
                   json.dumps({'reason': 'build-finished', 'success': False}) + '\n')
        for stream in streams:
            with self.subTest(stream=stream), \
                    patch.object(guard.subprocess, 'Popen', return_value=fake_process(stream)), \
                    self.assertRaises(ValueError):
                guard.capture_build(self.root, self.root / 'messages.jsonl')

    def test_dll_paths_match_target_artifacts_build_scripts_and_rust_sysroot(self):
        target = self.root / 'target'
        generated = target / 'debug/build/native/out'
        outside = self.root / 'outside-target'
        rust_libdir = self.root / 'rust-sysroot/lib'
        messages = [{'reason': 'build-script-executed',
                     'linked_paths': ['native=' + str(generated), str(generated),
                                      'native=' + str(outside)]}]
        actual = guard.dll_directories(messages, self.executable, target, rust_libdir)
        self.assertEqual(actual, [str(generated), str(self.executable.parent.parent),
                                  str(self.executable.parent), str(rust_libdir)])

    def test_dll_order_is_stable_and_excludes_other_profiles_under_same_target(self):
        target = self.root / 'target'
        first = target / 'debug/build/a-native/out'
        last = target / 'debug/build/z-native/out'
        other_profile = target / 'release/build/native/out'
        sibling = target / 'debug-other/build/native/out'
        rust_libdir = self.root / 'rust-sysroot/lib'
        paths = ['native=' + str(last), 'native=' + str(other_profile),
                 'native=' + str(first), 'native=' + str(sibling), 'native=' + str(last)]
        messages = [{'reason': 'build-script-executed', 'linked_paths': paths}]
        expected = [str(first), str(last), str(self.executable.parent.parent),
                    str(self.executable.parent), str(rust_libdir)]
        self.assertEqual(guard.dll_directories(messages, self.executable, target, rust_libdir),
                         expected)
        messages[0]['linked_paths'].reverse()
        self.assertEqual(guard.dll_directories(messages, self.executable, target, rust_libdir),
                         expected)

    def run_replay(self, lines, code=0):
        with patch.object(guard, 'verify_source') as verify, \
                patch.object(guard.subprocess, 'Popen', return_value=fake_process(lines, code)) as launch, \
                patch.object(guard.subprocess, 'check_output', side_effect=AssertionError('Cargo discovery in guard')), \
                patch.dict(os.environ, {'PATH': 'existing-search-path'}), \
                patch('sys.stdout', new_callable=io.StringIO):
            result = guard.replay(self.root, self.sha, self.proof_path, self.result_path)
        verify.assert_called_once_with(self.root, self.sha)
        return result, launch.call_args

    def test_replay_is_direct_nonempty_filtered_run_with_package_cwd_and_dll_environment(self):
        code, call = self.run_replay('running 2 tests\n'
            'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n')
        self.assertEqual(code, 0)
        self.assertEqual(call.args[0], [str(self.executable), 'headless_lifetime::', '--nocapture'])
        self.assertEqual(call.kwargs['cwd'], self.manifest.parent)
        env = call.kwargs['env']
        self.assertEqual(env['PATH'], os.pathsep.join([*self.proof['dll_directories'], 'existing-search-path']))
        self.assertEqual(env['CARGO_MANIFEST_DIR'], str(self.manifest.parent))
        self.assertEqual(env['CARGO_MANIFEST_PATH'], str(self.manifest))
        result = json.loads(self.result_path.read_text(encoding='utf-8'))
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['passed_test_count'], 2)
        self.assertEqual(result['executable_sha256'], guard.digest(self.executable))
        self.assertEqual(result['artifact_proof_sha256'], guard.digest(self.proof_path))
        self.assertNotIn('PATH', result)  # inherited environment is not serialized

    def test_empty_missing_or_multiple_success_summaries_are_rejected(self):
        summary = 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n'
        for lines in ('running 0 tests\n' + summary.replace('1 passed', '0 passed'),
                      'no libtest result\n', summary + summary):
            with self.subTest(lines=lines), self.assertRaises(ValueError):
                self.run_replay(lines)
            self.assertEqual(json.loads(self.result_path.read_text(encoding='utf-8'))['status'], 'failed')

    def test_nonzero_test_exit_is_preserved_even_if_a_success_line_was_printed(self):
        code, _ = self.run_replay('test result: ok. 1 passed; 0 failed; 0 ignored; '
                                 '0 measured; 0 filtered out; finished in 0.00s\n', 101)
        self.assertEqual(code, 101)
        result = json.loads(self.result_path.read_text(encoding='utf-8'))
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['exit_code'], 101)

    def test_changed_executable_source_target_or_profile_never_launches(self):
        for mutate in ('digest', 'source', 'target', 'profile', 'not_ready', 'outside_manifest'):
            proof = copy.deepcopy(self.proof)
            if mutate == 'digest':
                proof['executable_sha256'] = '0' * 64
            elif mutate == 'source':
                proof['source_commit'] = 'b' * 40
            elif mutate == 'target':
                proof['artifact']['target']['name'] = 'unrelated'
            elif mutate == 'profile':
                proof['artifact']['profile']['test'] = False
            elif mutate == 'not_ready':
                proof['status'] = 'build_failed'
            else:
                proof['manifest_path'] = str(self.root.parent / 'outside/Cargo.toml')
                proof['artifact']['manifest_path'] = proof['manifest_path']
            guard.write_json(self.proof_path, proof)
            with self.subTest(mutate=mutate), patch.object(guard, 'verify_source'), \
                    patch.object(guard.subprocess, 'Popen') as launch, self.assertRaises(ValueError):
                guard.replay(self.root, self.sha, self.proof_path, self.result_path)
            launch.assert_not_called()

    def test_source_check_rejects_wrong_sha_and_dirty_tracked_files_without_cargo(self):
        with patch.object(guard.subprocess, 'check_output', return_value=self.sha + '\n'), \
                patch.object(guard.subprocess, 'run', return_value=MagicMock(returncode=0)) as diff:
            guard.verify_source(self.root, self.sha)
            self.assertEqual(diff.call_args.args[0], ['git', 'diff', '--quiet', 'HEAD', '--'])
            with self.assertRaises(ValueError):
                guard.verify_source(self.root, 'b' * 40)
        with patch.object(guard.subprocess, 'check_output', return_value=self.sha + '\n'), \
                patch.object(guard.subprocess, 'run', return_value=MagicMock(returncode=1)), \
                self.assertRaises(ValueError):
            guard.verify_source(self.root, self.sha)

    def test_cli_validation_error_records_failure_and_cannot_reuse_stale_success(self):
        self.proof_path.unlink()
        guard.write_json(self.result_path, {'status': 'passed', 'source_commit': 'b' * 40})
        with patch.object(guard, 'ROOT', self.root), \
                patch('sys.argv', ['gui_test_artifact.py', 'replay', '--source-sha', self.sha]), \
                patch('sys.stderr', new_callable=io.StringIO), \
                patch.object(guard.subprocess, 'Popen') as launch:
            self.assertEqual(guard.main(), 1)
        launch.assert_not_called()
        result = json.loads(self.result_path.read_text(encoding='utf-8'))
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['source_commit'], self.sha)
        self.assertIn('error', result)


if __name__ == '__main__':
    unittest.main()
