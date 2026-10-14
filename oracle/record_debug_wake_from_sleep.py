#!/usr/bin/env python3
"""Help > debug > debug modes > simulate a wake from sleep, on the real client.

The real Help QAction is triggered with the wake delay set to a few seconds.
The popup `_ShowJustWokeToUser` makes (title, whether it can be cancelled, its
text each 100 ms), `JustWokeFromSleep()`, and whether the idle timer was reset
are read back; then the same with the popup cancelled half way, and with
"allow wake-from-system-sleep detection" off.
"""
import json
import sys
import tempfile
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
DELAY = 4


def record(session):
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        controller = session.controller
        gui = controller.gui
        jobs = []
        original_pub = controller.pub

        def pub(topic, *args, **kwargs):
            if topic == 'message':
                jobs.append(args[0])
            return original_pub(topic, *args, **kwargs)

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        menu = None
        out = dict(delay_seconds=DELAY, runs={})
        saved = controller.new_options.GetInteger('wake_delay_period'), controller.new_options.GetBoolean('do_sleep_check')
        try:
            controller.pub = pub
            controller.new_options.SetInteger('wake_delay_period', DELAY)
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'debug modes', 'simulate a wake from sleep'])
            out['menu_path'] = [label.replace('&', ''), 'debug', 'debug modes', action.text()]
            out['tooltip'] = action.toolTip()

            def run(name, detect, cancel_at=None):
                controller.new_options.SetBoolean('do_sleep_check', detect)
                jobs.clear()
                controller.SetTimestampMS('last_user_action', 0)
                started = time.monotonic()
                action.trigger()
                samples = []
                cancelled = False
                cancellable_at_start = None
                while time.monotonic() - started < DELAY + 2.5:
                    T.QTest.qWait(100)
                    t = round((time.monotonic() - started) * 1000)
                    if cancel_at is not None and not cancelled and t >= cancel_at and jobs:
                        jobs[0].Cancel()
                        cancelled = True
                    job = jobs[0] if jobs else None
                    if cancellable_at_start is None and job is not None:
                        cancellable_at_start = job.IsCancellable()
                    samples.append(dict(at_ms=t, just_woke=controller.JustWokeFromSleep(),
                                        text=None if job is None else job.GetStatusText(),
                                        done=None if job is None else job.IsDone(),
                                        dismissed=None if job is None else job.IsDismissed()))
                job = jobs[0] if jobs else None
                out['runs'][name] = dict(
                    detect=detect, cancel_at_ms=cancel_at, jobs=len(jobs),
                    title=None if job is None else job.GetStatusTitle(),
                    cancellable=cancellable_at_start,
                    idle_timer_reset=controller.GetTimestampMS('last_user_action') > 0,
                    samples=samples[::2])
                if job is not None:
                    job.FinishAndDismiss()
                time.sleep(0.3)

            run('detection on', True)
            run('cancelled', True, cancel_at=1500)
            run('detection off', False)
            return out
        finally:
            controller.pub = original_pub
            controller.new_options.SetInteger('wake_delay_period', saved[0])
            controller.new_options.SetBoolean('do_sleep_check', saved[1])
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
    (HERE / 'fixtures/debug_wake_from_sleep.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_wake_from_sleep.json')


if __name__ == '__main__':
    main()
