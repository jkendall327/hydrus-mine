#!/usr/bin/env python3
"""Help > debug > data actions > subscription manager snapshot.

The reference's subscriptions manager is given the four subscriptions of
`record_subscriptions_list.py` (time held at its `NOW`, subscription syncing
paused so nothing runs), the real Help menu entry is triggered, and the popup
it makes (`SubscriptionsManager.ShowSnapshot`) is recorded with the manager's
own sets and next work times behind each line. A second snapshot is taken with
no subscriptions.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_subscription_manager_snapshot.py
       (writes fixtures/subscription_manager_snapshot.json)
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    import record_subscriptions_list as base

    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        from hydrus.core import HydrusTime
        controller = session.controller
        gui = controller.gui
        manager = controller.subscriptions_manager
        shown = []
        original_pub, get_now = controller.pub, HydrusTime.GetNow

        def pub(topic, *args, **kwargs):
            if topic == 'message':
                shown.append(args[0].GetStatusText())
            return original_pub(topic, *args, **kwargs)

        def find(menu, labels):
            for action in menu.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)

        menu = None
        try:
            controller.pub = pub
            HydrusTime.GetNow = lambda: base.NOW
            controller.new_options.SetBoolean('pause_subs_sync', True)
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'data actions', 'subscription manager snapshot'])
            out = dict(menu_path=[label.replace('&', ''), 'debug', 'data actions', action.text()],
                       tooltip=action.toolTip(), now=base.NOW)
            cases = []
            from hydrus.client import ClientThreading
            for when in ('with subscriptions', 'with one running', 'with none'):
                if when == 'with none':
                    subs = []
                else:
                    subs = [base.make_subscription(case)[0] for case in base.SUBSCRIPTIONS]
                manager.SetSubscriptions(subs)
                running_job = None
                if when == 'with one running':
                    # (a running job is not made here: the manager's own record of one is, as the
                    # snapshot reads only its names)
                    running_job = ClientThreading.JobStatus()
                    with manager._lock:
                        manager._names_to_running_subscription_info['artist one'] = (running_job, subs[0])
                shown.clear()
                action.trigger()
                T.QTest.qWait(100)
                with manager._lock:
                    cases.append(dict(
                        when=when, messages=list(shown),
                        subs=sorted(manager._names_to_subscriptions),
                        not_runnable=sorted(manager._names_that_cannot_run),
                        next_work_times={n: t - base.NOW for n, t in manager._names_to_next_work_time.items()},
                        running=sorted(manager._names_to_running_subscription_info)))
                if running_job is not None:
                    with manager._lock:
                        manager._names_to_running_subscription_info.pop('artist one', None)
                    running_job.FinishAndDismiss()
            out['cases'] = cases
            return out
        finally:
            controller.pub = original_pub
            HydrusTime.GetNow = get_now
            controller.new_options.SetBoolean('pause_subs_sync', False)
            manager.SetSubscriptions([])
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
    (HERE / 'fixtures/subscription_manager_snapshot.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote subscription_manager_snapshot.json')


if __name__ == '__main__':
    main()
