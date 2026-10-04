#!/usr/bin/env python3
"""Record real session backup retention, timestamp collisions and append routing.

Boots basic, saves deterministic empty notebook trees synchronously through the
controller, queries the actual database backup timestamps, and appends one older
snapshot with the real notebook action. Clock reversal and exact-ms overwrite are
covered; retention is set to two while the recorded default remains ten.
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from hydrus.core import HydrusSerialisable, HydrusTime
    from hydrus.client.gui.pages import ClientGUISession
    controller = session.controller
    gui = controller.gui
    default = controller.new_options.GetInteger('number_of_gui_session_backups')
    controller.new_options.SetInteger('number_of_gui_session_backups', 2)
    now = [1700000000000]
    old_now = HydrusTime.GetNowMS
    HydrusTime.GetNowMS = lambda: now[0]
    steps = []
    try:
        for i, timestamp in enumerate([1700000000000, 1700000010000, 1700000020000,
                                        1700000030000, 1700000030000, 1700000000000]):
            now[0] = timestamp
            top = ClientGUISession.GUISessionContainerPageNotebook('top', [
                ClientGUISession.GUISessionContainerPageNotebook(f'version {i}')])
            saved = ClientGUISession.GUISessionContainer('backup test', top_notebook_container=top)
            controller.SaveGUISession(saved)
            backups = controller.Read('serialisable_names_to_backup_timestamps_ms', HydrusSerialisable.SERIALISABLE_TYPE_GUI_SESSION_CONTAINER)
            steps.append({'now': timestamp, 'version': i, 'backups': backups['backup test'] if 'backup test' in backups else []})
        backup_timestamp = steps[-1]['backups'][0]
        loaded = controller.Read('gui_session', 'backup test', backup_timestamp)
        label = HydrusTime.TimestampToPrettyTime(HydrusTime.SecondiseMS(backup_timestamp), in_utc=True)
        def append():
            gui._notebook.AppendGUISessionBackup('backup test', backup_timestamp)
            outer = gui._notebook.widget(gui._notebook.count()-1)
            return {'name': outer.GetName(), 'children': [p.GetName() for p in outer.GetPages()]}
        appended = controller.CallBlockingToQt(gui, append)
    finally:
        HydrusTime.GetNowMS = old_now
    return {'default_keep': default, 'keep': 2, 'steps': steps,
            'timestamp': backup_timestamp, 'label_utc': label, 'appended': appended}


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        destination.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work)/'backups.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE/'fixtures'/'session_backups.json').write_text(json.dumps(result, indent=2)+'\n')

if __name__ == '__main__': main()
