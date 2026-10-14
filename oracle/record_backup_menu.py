#!/usr/bin/env python3
"""Record the reference's Database > backup submenu as the menu updater leaves it.

The real `_InitialiseMenubarGetMenuUpdaterDatabase` runs (its worker and its publish) on the
real client for each state of the backup options and of the file locations; each scenario
records which of the submenu's entries are visible, their text, and, for the multiple
locations note, what clicking it shows. Time is held still (`HydrusTime.GetNow` is a
constant), and `last_backup_time` is set a given number of seconds before it.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    gui = c.gui
    out = {}
    from hydrus.core import HydrusTime
    NOW = 1_700_000_000
    HydrusTime.GetNow = lambda: NOW
    entries = ['set_up_backup_path', 'update_backup', 'change_backup_path', 'restore_backup', 'multiple_location_label']

    def setup(backup_path, ago, all_default):
        def work():
            c.new_options.SetNoneableString('backup_path', backup_path)
            c.new_options.SetNoneableInteger('last_backup_time', None if ago is None else NOW - ago)
            c.client_files_manager.AllLocationsAreDefault = lambda: all_default
        c.CallBlockingToQt(gui, work)
        c.CallBlockingToQt(gui, gui._menu_updater_database.update)
        time.sleep(2)

    def read():
        def work():
            return {name: dict(visible=getattr(gui, '_menubar_database_' + name).isVisible(),
                               text=getattr(gui, '_menubar_database_' + name).text())
                    for name in entries}
        return c.CallBlockingToQt(gui, work)

    # QAction.isVisible is the flag set by setVisible, whether or not a menu is shown
    scenarios = {
        'no_path': (None, None, True),
        'path_never': ('/somewhere', None, True),
        'path_just_now': ('/somewhere', 10, True),
        'path_29_minutes': ('/somewhere', 29 * 60, True),
        'path_31_minutes': ('/somewhere', 31 * 60, True),
        'path_2_hours': ('/somewhere', 2 * 3600, True),
        'path_3_days': ('/somewhere', 3 * 86400, True),
        'multiple_locations': (None, None, False),
        'multiple_locations_with_path': ('/somewhere', 60, False),
    }
    for name, (path, ago, all_default) in scenarios.items():
        setup(path, ago, all_default)
        state = read()
        state['inputs'] = dict(backup_path=path, ago_seconds=ago, all_locations_are_default=all_default)
        out[name] = state

    def note():
        import contextlib
        import io
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            gui._menubar_database_multiple_location_label.trigger()
        return buffer.getvalue()
    out['multiple_locations_note'] = c.CallBlockingToQt(gui, note)
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'backup_menu.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the backup menu')


if __name__ == '__main__':
    main()
