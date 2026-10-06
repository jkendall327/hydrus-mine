"""Exercise real Git index inputs across publication-only and build changes."""
import copy
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import input_mtime_ledger as ledger

OLD = 1_600_000_000_000_000_000
ARTIFACT = OLD + 10_000_000_000
FRESH = ARTIFACT + 10_000_000_000
DOC = 'docs/rust/gui-coverage/checkpoints/previous/proof.json'
REMOVED = 'docs/rust/gui-coverage/audit/previous.json'
NEW = 'docs/rust/gui-coverage/checkpoints/next/proof.json'
NEIGHBOUR = 'docs/rust/gui-coverage-inputs/value.txt'


class PublicationInputs(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.git('init', '-q')
        for name in ('Cargo.toml', 'ui/main.slint', DOC, REMOVED, NEIGHBOUR):
            self.write(name, 'original', OLD)
        self.git('add', '.')
        self.cached = ledger.capture(self.root)
        for path in self.root.rglob('*'):
            if path.is_file() and '.git' not in path.parts:
                os.utime(path, ns=(FRESH, FRESH))

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, stderr=subprocess.DEVNULL)

    def write(self, name, text, stamp=FRESH):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        os.utime(path, ns=(stamp, stamp))

    def remove(self, name):
        self.git('rm', '-q', '--cached', name)
        (self.root / name).unlink()

    def test_publication_add_change_delete_preserves_build_reuse_without_stamping_evidence(self):
        self.write(DOC, 'revised evidence')
        self.write(NEW, 'new evidence')
        self.git('add', DOC, NEW)
        self.remove(REMOVED)
        current, matching = ledger.preflight(self.root, self.cached, ARTIFACT)
        self.assertEqual(set(matching), {'Cargo.toml', 'ui/main.slint', NEIGHBOUR})
        evidence_times = {name: (self.root / name).stat().st_mtime_ns for name in (DOC, NEW)}
        ledger.stamp_matching(self.root, self.cached, current, matching)
        for name in matching:
            self.assertEqual((self.root / name).stat().st_mtime_ns, OLD)
        for name, stamp in evidence_times.items():
            self.assertEqual((self.root / name).stat().st_mtime_ns, stamp)
        self.assertFalse(any(entry['path'].startswith(ledger.PUBLICATION_PREFIXES)
                             for entry in current['inputs']))

    def test_added_deleted_build_or_neighbouring_docs_input_still_rejects_before_stamp(self):
        for name in ('ui/extra.slint', 'docs/rust/gui-coverage-inputs/new.txt',
                     'docs/rust/gui-coverage/data/new.json',
                     'docs/rust/gui-coverage/checkpoints-extra/new.json'):
            with self.subTest(added=name):
                self.write(name, 'new input')
                self.git('add', name)
                with self.assertRaisesRegex(ValueError, 'input paths added/deleted'):
                    ledger.preflight(self.root, self.cached, ARTIFACT)
                self.assertEqual((self.root / 'Cargo.toml').stat().st_mtime_ns, FRESH)
                self.remove(name)
        self.remove('ui/main.slint')
        with self.assertRaisesRegex(ValueError, 'input paths added/deleted'):
            ledger.preflight(self.root, self.cached, ARTIFACT)
        self.assertEqual((self.root / 'Cargo.toml').stat().st_mtime_ns, FRESH)

    def test_excluded_paths_still_reject_unsafe_index_entries_and_total_count(self):
        def record(path, mode='100644', stage='0'):
            return f'{mode} {"0" * 40} {stage}\t{path}\0'.encode()
        normal = record('Cargo.toml')
        for label, extra in (
            ('symlink mode', record(DOC, '120000')),
            ('submodule mode', record(DOC, '160000')),
            ('conflicted index', record(DOC, stage='1')),
            ('duplicate excluded path', record(DOC) * 2),
            ('parent escape', record('docs/rust/gui-coverage/../escape')),
        ):
            with self.subTest(case=label), patch.object(ledger.subprocess, 'check_output', return_value=normal + extra):
                with self.assertRaises(ValueError):
                    ledger.tracked_inputs(self.root)
        with patch.object(ledger.subprocess, 'check_output', return_value=normal + record(DOC)), patch.object(ledger, 'MAX_INPUTS', 1):
            with self.assertRaisesRegex(ValueError, 'unsupported input count'):
                ledger.tracked_inputs(self.root)
        target = self.root / DOC
        target.unlink()
        target.symlink_to(self.root / 'Cargo.toml')
        with patch.object(ledger.subprocess, 'check_output', return_value=normal + record(DOC)):
            with self.assertRaisesRegex(ValueError, 'symlink'):
                ledger.tracked_inputs(self.root)

    def test_previous_full_repository_ledger_version_is_not_accepted(self):
        previous = copy.deepcopy(self.cached)
        previous['version'] = 1
        with self.assertRaisesRegex(ValueError, 'invalid input ledger'):
            ledger.preflight(self.root, previous, ARTIFACT)


if __name__ == '__main__':
    unittest.main()
