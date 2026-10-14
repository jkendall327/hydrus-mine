#!/usr/bin/env python3
"""Help > debug > report modes > idle report mode, on the real client.

The real Help QAction turns the mode on; then `ClientController.CurrentlyIdle`
is run in each scenario (boot age, last user/mouse/Client API action ages, the
options, shutting down, forced idle, whether it was idle before), with the
clock held at NOW, and the popups it makes are read back, in order.
"""
import json
import sys
import tempfile
import threading
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

NOW_MS = 1_800_000_000_000
S = 1000

# boot age, user age, mouse age, api age (seconds before now), then options
SCENARIOS = [
    dict(name='booted a minute ago', boot=60, user=9999, mouse=9999, api=9999),
    dict(name='all quiet', boot=9999, user=9999, mouse=9999, api=9999),
    dict(name='user acted 5 seconds ago', boot=9999, user=5, mouse=9999, api=9999),
    dict(name='mouse moved 90 seconds ago', boot=9999, user=9999, mouse=90, api=9999),
    dict(name='api used 7 seconds ago', boot=9999, user=9999, mouse=9999, api=7, api_timeout=60),
    dict(name='all three recent', boot=9999, user=5, mouse=90, api=7, api_timeout=60),
    dict(name='user period none', boot=9999, user=5, mouse=9999, api=9999, user_period=None),
    dict(name='mouse period none', boot=9999, user=9999, mouse=5, api=9999, mouse_period=None),
    dict(name='idle disabled', boot=9999, user=9999, mouse=9999, api=9999, normal=False),
    dict(name='forced', boot=60, user=5, mouse=5, api=5, force=True),
    dict(name='shutting down', boot=9999, user=9999, mouse=9999, api=9999, shutting_down=True),
    dict(name='was idle, now active', boot=9999, user=5, mouse=9999, api=9999, was_idle=True),
    dict(name='was idle, still idle', boot=9999, user=9999, mouse=9999, api=9999, was_idle=True),
    dict(name='was active, now idle', boot=9999, user=9999, mouse=9999, api=9999, was_idle=False),
    dict(name='was active, still active', boot=9999, user=5, mouse=9999, api=9999, was_idle=False),
    dict(name='exactly at the user period', boot=9999, user=1800, mouse=9999, api=9999),
    dict(name='just past the user period', boot=9999, user=1801, mouse=9999, api=9999),
    dict(name='exactly 120s since boot', boot=120, user=9999, mouse=9999, api=9999),
    dict(name='121s since boot', boot=121, user=9999, mouse=9999, api=9999),
]


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        from hydrus.core import HydrusTime
        from hydrus.core import HydrusGlobals as HG
        controller = session.controller
        gui = controller.gui
        shown = []
        mine = [threading.get_ident()]
        original_pub, now_ms, now = controller.pub, HydrusTime.GetNowMS, HydrusTime.GetNow
        saved = dict(options=dict(controller.options), normal=controller.new_options.GetNoneableInteger('idle_mode_client_api_timeout'))

        def pub(topic, *args, **kwargs):
            # (the client's own daemons check idle too: only this thread's reports are read)
            if topic == 'message' and threading.get_ident() == mine[0]:
                shown.append(args[0].GetStatusText())
            return original_pub(topic, *args, **kwargs)

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        menu = None
        out = dict(now_ms=NOW_MS, scenarios=[])
        try:
            controller.pub = pub
            HydrusTime.GetNowMS = lambda: NOW_MS
            HydrusTime.GetNow = lambda: NOW_MS // 1000
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'report modes', 'idle report mode'])
            out['menu_path'] = [label.replace('&', ''), 'debug', 'report modes', action.text()]
            action.trigger()
            assert HG.idle_report_mode
            for sc in SCENARIOS:
                controller.SetTimestampMS('boot', NOW_MS - sc['boot'] * S)
                controller.SetTimestampMS('last_user_action', NOW_MS - sc['user'] * S)
                controller.SetTimestampMS('last_mouse_action', NOW_MS - sc['mouse'] * S)
                controller.SetTimestampMS('last_client_api_action', NOW_MS - sc['api'] * S)
                controller.options['idle_normal'] = sc.get('normal', True)
                controller.options['idle_period'] = sc.get('user_period', 1800)
                controller.options['idle_mouse_period'] = sc.get('mouse_period', 600)
                controller.new_options.SetNoneableInteger('idle_mode_client_api_timeout', sc.get('api_timeout'))
                HG.force_idle_mode = sc.get('force', False)
                controller._program_is_shutting_down = sc.get('shutting_down', False)
                controller._previously_idle = sc.get('was_idle', False)
                shown.clear()
                result = controller.CurrentlyIdle()
                T.QTest.qWait(30)
                controller._program_is_shutting_down = False
                out['scenarios'].append(dict(sc, result=result, messages=list(shown)))
            action.trigger()
            shown.clear()
            controller.CurrentlyIdle()
            T.QTest.qWait(30)
            out['silent_when_off'] = list(shown)
            return out
        finally:
            controller.pub = original_pub
            HydrusTime.GetNowMS, HydrusTime.GetNow = now_ms, now
            HG.force_idle_mode = False
            HG.idle_report_mode = False
            controller._previously_idle = False
            controller.options.update(saved['options'])
            controller.new_options.SetNoneableInteger('idle_mode_client_api_timeout', saved['normal'])
            if menu is not None:
                menu.deleteLater()
            W.QApplication.processEvents()

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        database = record_api.unpack_fixture('basic')
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/debug_idle_report.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_idle_report.json')


if __name__ == '__main__':
    main()
