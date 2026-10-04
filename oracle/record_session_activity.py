#!/usr/bin/env python3
"""Record real auxiliary-dialog, global mouse and API idle timestamp updates.

Runs the actual Dialog constructor, CheckMouseIdle with a real Qt cursor move,
and ResetIdleTimerFromClientAPI on the live reference controller. Holds its clock
still and records which idle timestamps each action changes.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtCore as QC, QtGui as QG
    from hydrus.core import HydrusTime
    from hydrus.client.gui import ClientGUIDialogs, ClientGUIFunctions
    controller, gui = session.controller, session.controller.gui
    clock = [1700200000000]
    keys = ['last_user_action', 'last_mouse_action', 'last_client_api_action']
    old_now = HydrusTime.GetNowMS
    old_times = {key: controller.GetTimestampMS(key) for key in keys}
    old_mouse = controller._last_mouse_position
    old_cursor = ClientGUIFunctions.GetMousePos()
    HydrusTime.GetNowMS = lambda: clock[0]

    def drive():
        for key in keys:
            controller.SetTimestampMS(key, 0)
        rows = []

        def snapshot(action):
            rows.append({'action': action, 'now': clock[0], 'times': {key: controller.GetTimestampMS(key) for key in keys}})

        dialog = ClientGUIDialogs.Dialog(gui, 'idle activity recording')
        snapshot('auxiliary dialog')
        dialog.close()
        dialog.deleteLater()
        clock[0] += 1000
        controller._last_mouse_position = ClientGUIFunctions.GetMousePos()
        controller.CheckMouseIdle()
        snapshot('unchanged cursor')
        clock[0] += 1000
        QG.QCursor.setPos(old_cursor + QC.QPoint(23, 29))
        controller.CheckMouseIdle()
        snapshot('moved cursor')
        clock[0] += 1000
        controller.ResetIdleTimerFromClientAPI()
        snapshot('API request')
        return {'steps': rows}

    try:
        return controller.CallBlockingToQt(gui, drive)
    finally:
        HydrusTime.GetNowMS = old_now
        for key, timestamp in old_times.items():
            controller.SetTimestampMS(key, timestamp)
        controller._last_mouse_position = old_mouse
        QG.QCursor.setPos(old_cursor)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        destination.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'activity.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'session_activity.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
