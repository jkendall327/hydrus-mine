#!/usr/bin/env python3
"""Actual ImportingPanel controls and named controller slot counters.

No importer/network request is started. Real controller acquisition/release and
limit refresh run against synthetic held permits. Qt drafts, bounds, save,
serialization, reopen and abandoned-draft cancellation are recorded separately.
"""
import json, sys, tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
KEYS = ['gallery_files', 'gallery_search', 'watcher_files', 'watcher_check', 'misc']

def record(session):
    def drive():
        from hydrus.client.gui.panels.options.ImportingPanel import ImportingPanel
        from hydrus.core import HydrusSerialisable
        from hydrus.client import ClientOptions
        c = session.controller
        original = c.new_options
        old_slots = dict(c._thread_slots)
        panels = []
        c.new_options = original.Duplicate()
        def prefs(options):
            return {key: options.GetInteger('thread_slots_' + key) for key in KEYS}
        def controls(panel):
            return {key: getattr(panel, '_thread_slots_' + key).value() for key in KEYS}
        def snapshot(action, key, **extra):
            return dict(action=action, state=list(c._thread_slots[key]), **extra)
        try:
            defaults = prefs(c.new_options)
            fresh_defaults = prefs(ClientOptions.ClientOptions())
            loaded = ImportingPanel(c.gui, c.new_options); panels.append(loaded)
            for key, value in zip(KEYS, [0, -1, 501, 999, 0]):
                c.new_options.SetInteger('thread_slots_' + key, value)
            raw_before = prefs(c.new_options)
            loaded = ImportingPanel(c.gui, c.new_options); panels.append(loaded)
            loaded_cells = controls(loaded)
            loaded.UpdateOptions()
            loaded_boundaries = dict(before=raw_before, controls=loaded_cells, saved=prefs(c.new_options))
            loaded.resize(1050, 480); loaded.show()
            from qtpy import QtWidgets
            QtWidgets.QApplication.processEvents()
            assert loaded.grab().save(str(HERE / 'fixtures/import_work_slots.png'))
            loaded.hide()
            for key, value in defaults.items(): c.new_options.SetInteger('thread_slots_' + key, value)
            events = []
            for values in [[5, 15, 5, 15, 10], [2, 3, 4, 6, 7], [0, -1, 501, 999, 0], [500] * 5]:
                p = ImportingPanel(c.gui, c.new_options); panels.append(p)
                before = prefs(c.new_options)
                for key, value in zip(KEYS, values):
                    getattr(p, '_thread_slots_' + key).setValue(value)
                draft = controls(p); staged = prefs(c.new_options)
                p.UpdateOptions()
                encoded = HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
                reopened = ImportingPanel(c.gui, c.new_options); panels.append(reopened)
                events.append(dict(input=values, before=before, draft=draft, staged=staged, saved=prefs(c.new_options), round_trip=prefs(encoded), reopened=controls(reopened)))
            before = prefs(c.new_options)
            cancelled = ImportingPanel(c.gui, c.new_options)
            for key in KEYS: getattr(cancelled, '_thread_slots_' + key).setValue(1)
            cancelled.deleteLater()
            cancelled_after = prefs(c.new_options)
            boundaries = []
            for key in KEYS:
                c.new_options.SetInteger('thread_slots_' + key, 2)
                c._UpdateThreadSlotLockLimits()
                assert c._thread_slots[key] == (0, 2)
                steps = []
                for _ in range(3):
                    granted = c.AcquireThreadSlot(key)
                    steps.append(snapshot('acquire', key, granted=granted))
                c.new_options.SetInteger('thread_slots_' + key, 1); c._UpdateThreadSlotLockLimits()
                steps.append(snapshot('lower', key, limit=1))
                steps.append(snapshot('acquire', key, granted=c.AcquireThreadSlot(key)))
                c.ReleaseThreadSlot(key); steps.append(snapshot('release', key))
                steps.append(snapshot('acquire', key, granted=c.AcquireThreadSlot(key)))
                c.ReleaseThreadSlot(key); steps.append(snapshot('release', key))
                steps.append(snapshot('acquire', key, granted=c.AcquireThreadSlot(key)))
                c.new_options.SetInteger('thread_slots_' + key, 3); c._UpdateThreadSlotLockLimits()
                steps.append(snapshot('raise', key, limit=3))
                for _ in range(3):
                    steps.append(snapshot('acquire', key, granted=c.AcquireThreadSlot(key)))
                for _ in range(3):
                    c.ReleaseThreadSlot(key); steps.append(snapshot('release', key))
                boundaries.append(dict(kind=key, events=steps))
            return dict(defaults=defaults, fresh_defaults=fresh_defaults, loaded_boundaries=loaded_boundaries, events=events, cancel_before=before, cancel_after=cancelled_after, boundaries=boundaries)
        finally:
            c.new_options = original
            with c._thread_slot_lock: c._thread_slots = old_slots
            for panel in panels: panel.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui, drive)

def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(path))
        result = json.loads(path.read_text())
    (HERE / 'fixtures/import_work_slots.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote import_work_slots.json')
if __name__ == '__main__': main()
