"""Portable restore safety regressions: temp files only, no Cargo/network/cache access.

Run with Python 3.11+: python -m unittest discover -s .github/scripts/tests -v
Actual Cargo Fresh/test-execution behavior is a separate integration experiment.
"""
import contextlib
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
WORKFLOW = SCRIPTS.parent / 'workflows/rust.yml'
sys.path.insert(0, str(SCRIPTS))
import input_mtime_ledger as ledger
import workspace_artifact_cache as cache

OLD = 1_600_000_000_000_000_000
ARTIFACT = OLD + 10_000_000_000
CHECKOUT = ARTIFACT + 10_000_000_000


class RestoreSafety(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='workspace-cache-regression-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / 'checkout'
        self.root.mkdir()
        self.home = self.base / 'home'
        self.home.mkdir()
        self.target = self.root / 'target'
        self.snapshot = self.root / '.ci-workspace-artifacts'
        self.inputs = {'Cargo.toml': '100644', 'uiish/Cargo.toml': '100644', 'uiish/build.rs': '100644', 'uiish/src/lib.rs': '100644', 'inputs/value.txt': '100644'}
        content = {'Cargo.toml': '[workspace]\nmembers = ["uiish"]\n', 'uiish/Cargo.toml': '[package]\nname = "uiish"\nversion = "0.1.0"\n', 'inputs/value.txt': '1\n'}
        for relative in self.inputs:
            file = self.root / relative
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text(content.get(relative, 'original fixture\n'))
            os.utime(file, ns=(OLD, OLD))
        self.outputs = []
        for item in (patch.object(cache, 'ROOT', self.root), patch.object(cache, 'SNAPSHOT', self.snapshot), patch.object(cache, 'compatibility', return_value=('lane-', 'same-flags')), patch.object(cache, 'output', side_effect=lambda name, value: self.outputs.append((name, value))), patch.object(ledger, 'tracked_inputs', side_effect=self.tracked), patch.object(Path, 'home', return_value=self.home), patch.dict(os.environ, {'CARGO_HOME': str(self.home / '.cargo'), 'GITHUB_SHA': 'fixture'}, clear=True)):
            item.start()
            self.addCleanup(item.stop)
        self.call(cache.restore)
        self.artifacts = []
        for relative in ('target/debug/deps/libuiish-aa.rlib', 'target/debug/.fingerprint/uiish-aa/lib-uiish', 'target/debug/build/uiish-aa/out/generated.rs'):
            file = self.root / relative
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(('compiled-A:' + relative).encode())
            os.utime(file, ns=(ARTIFACT, ARTIFACT))
            self.artifacts.append(file)
        self.call(cache.stage)
        self.assertIn(('save', 'true'), self.outputs)
        self.cached_inputs = ledger.load(self.snapshot / 'inputs.json')
        self.refresh()
        self.outputs.clear()

    def tracked(self, root):
        for relative in self.inputs:
            ledger.safe_path(root, relative)
        return self.inputs.copy()

    def refresh(self):
        for relative in self.inputs:
            os.utime(self.root / relative, ns=(CHECKOUT, CHECKOUT))

    def call(self, function):
        with contextlib.redirect_stdout(io.StringIO()):
            return function()

    def last_safe(self):
        return [value for name, value in self.outputs if name == 'safe'][-1]

    def unchanged(self, original):
        self.assertTrue(ledger.same_inputs(original, ledger.capture(self.root), include_mtime=True))

    def rejected_without_mutation(self):
        original = ledger.capture(self.root)
        original_target = {p.relative_to(self.target).as_posix(): p.read_bytes() for p in self.target.rglob('*') if p.is_file()}
        self.call(cache.restore)
        self.assertEqual(self.last_safe(), 'true')
        self.unchanged(original)
        self.assertEqual(original_target, {p.relative_to(self.target).as_posix(): p.read_bytes() for p in self.target.rglob('*') if p.is_file()})

    def test_healthy_restore_copies_before_stamping_and_invalidates_test_bin_units(self):
        excluded = []
        for unit, name in (('uiish-testbb', 'test-lib-uiish'), ('uiish-bincc', 'bin-uiish')):
            file = self.target / 'debug/.fingerprint' / unit / name
            file.parent.mkdir(parents=True)
            file.write_bytes(b'prior unit must not remain fresh')
            excluded.append(file)
        residual_library = self.target / 'debug/deps/libuiish-old.rlib'
        residual_library.write_bytes(b'residual compiled-B library')
        residual_generated = self.target / 'debug/build/uiish-old/out/stale.rs'
        residual_generated.parent.mkdir(parents=True)
        residual_generated.write_bytes(b'residual generated-B output')
        events = []
        real_copy, real_stamp = cache.shutil.copy2, ledger.stamp_matching
        def record_copy(*args, **kwargs):
            events.append('copy')
            return real_copy(*args, **kwargs)
        def record_stamp(*args, **kwargs):
            events.append('stamp')
            self.assertFalse(any(file.exists() for file in excluded))
            self.assertFalse(residual_library.exists())
            self.assertFalse(residual_generated.exists())
            return real_stamp(*args, **kwargs)
        with patch.object(cache.shutil, 'copy2', side_effect=record_copy), patch.object(ledger, 'stamp_matching', side_effect=record_stamp):
            self.call(cache.restore)
        self.assertEqual(self.last_safe(), 'true')
        self.assertEqual(events, ['copy'] * len(self.artifacts) + ['stamp'])
        self.unchanged(self.cached_inputs)
        for file in self.artifacts:
            self.assertEqual(file.read_bytes(), ('compiled-A:' + file.relative_to(self.root).as_posix()).encode())

    def test_copy_failure_changes_no_source_mtime_and_purges_mixed_target(self):
        original = ledger.capture(self.root)
        with patch.object(cache.shutil, 'copy2', side_effect=OSError('copy failed')), patch.object(ledger, 'stamp_matching', wraps=ledger.stamp_matching) as stamp:
            self.call(cache.restore)
        stamp.assert_not_called()
        self.unchanged(original)
        self.assertFalse(self.target.exists())
        self.assertEqual(self.last_safe(), 'true')  # explicitly cold fallback

    def fail_second_stamp(self, *args, **kwargs):
        real = os.utime
        calls = []
        def fail_second(*stamp_args, **stamp_kwargs):
            calls.append(None)
            if len(calls) == 2:
                raise OSError('second source stamp failed')
            return real(*stamp_args, **stamp_kwargs)
        with patch.object(ledger.os, 'utime', side_effect=fail_second):
            return self.real_stamp(*args, **kwargs)

    def test_partial_stamp_failure_restores_every_mtime_and_purges_target(self):
        original = ledger.capture(self.root)
        self.real_stamp = ledger.stamp_matching
        with patch.object(ledger, 'stamp_matching', side_effect=self.fail_second_stamp):
            self.call(cache.restore)
        self.unchanged(original)
        self.assertFalse(self.target.exists())
        self.assertEqual(self.last_safe(), 'true')

    def test_unrecoverable_rollback_never_claims_safe(self):
        self.real_stamp = ledger.stamp_matching
        with patch.object(ledger, 'stamp_matching', side_effect=self.fail_second_stamp), patch.object(ledger, 'rollback_mtimes', return_value=['cannot restore input mtime']):
            with self.assertRaises(cache.UnsafeRestoreError):
                self.call(cache.restore)
        self.assertEqual(self.last_safe(), 'false')
        self.assertFalse(self.target.exists())

    def test_unrecoverable_target_purge_never_claims_safe(self):
        real = cache.shutil.rmtree
        def fail_only_whole_target(path, *args, **kwargs):
            if Path(path) == self.target:
                raise OSError('cannot invalidate target')
            return real(path, *args, **kwargs)
        with patch.object(cache.shutil, 'copy2', side_effect=OSError('copy failed')) as copied, patch.object(cache.shutil, 'rmtree', side_effect=fail_only_whole_target):
            with self.assertRaises(cache.UnsafeRestoreError):
                self.call(cache.restore)
        copied.assert_called_once()
        self.assertEqual(self.last_safe(), 'false')

    def test_baseline_write_failure_rolls_back_successful_restore_and_blocks(self):
        original = ledger.capture(self.root)
        real = Path.write_text
        def fail_baseline(path, *args, **kwargs):
            if path.name == ledger.BASELINE_NAME:
                raise OSError('baseline write failed')
            return real(path, *args, **kwargs)
        with patch.object(Path, 'write_text', fail_baseline):
            with self.assertRaises(OSError):
                self.call(cache.restore)
        self.unchanged(original)
        self.assertFalse(self.target.exists())
        self.assertEqual(self.last_safe(), 'false')

    def test_changed_external_input_remains_newer_than_cached_artifacts(self):
        file = self.root / 'inputs/value.txt'
        file.write_text('2\n')
        os.utime(file, ns=(CHECKOUT, CHECKOUT))
        self.call(cache.restore)
        self.assertEqual(file.read_text(), '2\n')
        self.assertEqual(file.stat().st_mtime_ns, CHECKOUT)
        self.assertEqual((self.root / 'uiish/src/lib.rs').stat().st_mtime_ns, OLD)
        self.assertEqual(self.last_safe(), 'true')

    def test_backdated_changed_input_rejected_before_any_stamp(self):
        file = self.root / 'inputs/value.txt'
        file.write_text('2\n')
        os.utime(file, ns=(ARTIFACT, ARTIFACT))
        self.rejected_without_mutation()

    def test_added_or_deleted_inputs_reject(self):
        extra = self.root / 'extra.slint'
        extra.write_text('new tracked input')
        self.inputs['extra.slint'] = '100644'
        self.rejected_without_mutation()
        del self.inputs['extra.slint']
        del self.inputs['uiish/build.rs']
        self.rejected_without_mutation()

    def test_missing_incompatible_or_failed_cache_extraction_is_normal_fallback(self):
        metadata = self.snapshot / 'snapshot.json'
        original = metadata.read_text()
        metadata.unlink()
        self.rejected_without_mutation()
        metadata.write_text(original)
        data = json.loads(original)
        data['compatibility'] = 'different-flags'
        metadata.write_text(json.dumps(data))
        self.rejected_without_mutation()
        metadata.write_text(original)
        with patch.dict(os.environ, {'WORKSPACE_CACHE_RESTORE_OUTCOME': 'failure'}), patch.object(cache, 'restore_snapshot') as restore:
            self.rejected_without_mutation()
        restore.assert_not_called()

    def test_partial_snapshot_missing_artifact_rejected(self):
        (self.snapshot / self.artifacts[0].relative_to(self.root)).unlink()
        self.rejected_without_mutation()

    def test_snapshot_inventory_size_count_traversal_and_duplicate_rejected(self):
        path = self.snapshot / 'snapshot.json'
        original = json.loads(path.read_text())
        mutations = []
        for field in ('files', 'bytes'):
            data = copy.deepcopy(original)
            data[field] += 1
            mutations.append(data)
        data = copy.deepcopy(original); data['inventory'][0]['bytes'] += 1; mutations.append(data)
        data = copy.deepcopy(original); data['inventory'][0]['path'] = '../escape'; mutations.append(data)
        data = copy.deepcopy(original); data['inventory'].append(data['inventory'][0].copy()); mutations.append(data)
        for data in mutations:
            with self.subTest(metadata=data):
                path.write_text(json.dumps(data))
                self.rejected_without_mutation()

    def test_environment_target_or_build_layout_rejected(self):
        for name in ('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_TARGET', 'CARGO_BUILD_BUILD_DIR'):
            with self.subTest(variable=name), patch.dict(os.environ, {name: str(self.base / 'unsupported-layout')}):
                self.rejected_without_mutation()

    def test_root_ancestor_default_and_explicit_home_cargo_layout_configs_rejected(self):
        configs = (self.root / '.cargo/config.toml', self.base / '.cargo/config.toml', self.home / '.cargo/config.toml')
        for file in configs:
            file.parent.mkdir(parents=True, exist_ok=True)
            for key in ('target-dir', 'target', 'build-dir'):
                with self.subTest(config=file, key=key):
                    file.write_text(f'[build]\n{key} = "unsupported"\n')
                    self.rejected_without_mutation()
            file.unlink()
        with patch.dict(os.environ, {}, clear=True):
            file = self.home / '.cargo/config.toml'
            file.write_text('[build]\ntarget = "unsupported"\n')
            self.rejected_without_mutation()

    def test_cargo_config_env_target_injections_rejected(self):
        file = self.root / '.cargo/config.toml'
        file.parent.mkdir()
        for name in ('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_TARGET', 'CARGO_BUILD_BUILD_DIR'):
            for value in ('"unsupported"', '{ value = "unsupported", force = true }'):
                with self.subTest(variable=name, value=value):
                    file.write_text(f'[env]\n{name} = {value}\n')
                    self.rejected_without_mutation()

    def make_symlink(self, link, destination, directory=False):
        try:
            link.symlink_to(destination, target_is_directory=directory)
        except (OSError, NotImplementedError):
            self.skipTest('filesystem/user cannot create symlinks')

    def test_target_debug_deps_build_and_deep_destination_symlinks_rejected(self):
        paths = ('target/debug', 'target/debug/deps', 'target/debug/build', 'target/debug/build/uiish-aa/out', 'target/debug/deps/libuiish-aa.rlib')
        for relative in paths:
            with self.subTest(path=relative):
                path = self.root / relative
                moved = path.with_name(path.name + '-real')
                path.rename(moved)
                self.make_symlink(path, moved, moved.is_dir())
                original = ledger.capture(self.root)
                with patch.object(cache.shutil, 'copy2', wraps=cache.shutil.copy2) as copied:
                    self.call(cache.restore)
                copied.assert_not_called()
                self.unchanged(original)
                self.assertEqual(self.last_safe(), 'true')
                path.unlink()
                moved.rename(path)

    def test_snapshot_symlink_rejected(self):
        file = self.snapshot / self.artifacts[0].relative_to(self.root)
        destination = file.with_name(file.name + '-real')
        file.rename(destination)
        self.make_symlink(file, destination)
        self.rejected_without_mutation()

    @unittest.skipUnless(hasattr(os, 'mkfifo'), 'FIFO creation unavailable')
    def test_snapshot_nonregular_file_rejected_without_opening(self):
        fifo = self.snapshot / 'target/debug/deps/fifo'
        os.mkfifo(fifo)
        self.rejected_without_mutation()

    def test_safe_input_path_rejects_escape_and_windows_aliases(self):
        for relative in ('../escape', '/absolute', 'uiish/../Cargo.toml', 'uiish//bad', 'C:/escape', 'uiish\\escape', '//server/file'):
            with self.subTest(path=relative), self.assertRaises((OSError, ValueError)):
                ledger.safe_path(self.root, relative)

    def test_workflow_compile_gate_allows_disabled_key_failure_and_safe_miss(self):
        text = WORKFLOW.read_text()
        block = text.split('      - name: clippy\n', 1)[1].split('      - name:', 1)[0]
        condition = next(line.strip()[4:].strip() for line in block.splitlines() if line.strip().startswith('if: '))
        # Parse only the fixed safety expression already asserted below, never eval workflow text.
        expected = "${{ !cancelled() && (steps.workspace-key.outcome != 'success' || steps.workspace-merge.outputs.safe == 'true') }}"
        self.assertEqual(condition, expected)
        for label, cancelled, key_outcome, safe, should_run in (
            ('disabled', False, 'skipped', None, True),
            ('key failed without stamps', False, 'failure', None, True),
            ('cache miss, safe normal fallback', False, 'success', 'true', True),
            ('failed extraction safely ignored', False, 'success', 'true', True),
            ('unsafe stamp recovery', False, 'success', 'false', False),
            ('safety not proven', False, 'success', None, False),
            ('cancelled', True, 'success', 'true', False),
        ):
            with self.subTest(path=label):
                self.assertEqual(not cancelled and (key_outcome != 'success' or safe == 'true'), should_run)

    def test_workflow_fails_closed_and_gates_non_cancelled_compilation(self):
        text = WORKFLOW.read_text()
        merges = text.count('id: workspace-merge')
        self.assertIn(merges, (1, 3))
        self.assertEqual(text.count('name: fail closed on uncertain workspace restore'), merges)
        self.assertEqual(text.count('WORKSPACE_CACHE_RESTORE_OUTCOME: ${{ steps.workspace-cache.outcome }}'), merges)
        gate = "if: ${{ !cancelled() && (steps.workspace-key.outcome != 'success' || steps.workspace-merge.outputs.safe == 'true') }}"
        self.assertEqual(text.count(gate), 2)
        for name in ('clippy', 'test'):
            block = text.split('      - name: ' + name + '\n', 1)[1].split('      - name:', 1)[0]
            self.assertIn(gate, block)
        self.assertEqual(text.count("steps.workspace-merge.outputs.safe == 'true' && github.event_name"), merges)
        if merges == 1:
            self.assertIn('workspace_cache:', text)
            self.assertIn('if: inputs.workspace_cache', text)
            self.assertIn('inputs.full_validation && inputs.workspace_cache', text)
            self.assertIn('cache-workspace-crates: ${{ !inputs.workspace_cache }}', text)
            other_lanes = text.split('  # Keep exact-reference', 1)[1].split('  workspace-cache-maintenance:', 1)[0]
            self.assertNotIn('workspace-key', other_lanes)
            self.assertNotIn('workspace_artifact_cache', other_lanes)


if __name__ == '__main__':
    unittest.main(verbosity=2)
