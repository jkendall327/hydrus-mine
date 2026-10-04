#!/usr/bin/env python3
"""Drive the real Qt subscription concurrency spin and manager admission policy.

The manager is not started: synthetic running-job membership and due times are
inputs to its actual _GetSubscriptionReadyToGo, including live limit changes,
global pauses, single-flight exclusion and the three-second due-time buffer.
No subscriptions are synced and no site is contacted by this recording.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
NOW = 1700000000


def record(session):
    c = session.controller
    from hydrus.core import HydrusGlobals as HG, HydrusTime
    from hydrus.client.importing import ClientImportSubscriptions as S
    from hydrus.client.gui.panels.options.DownloadingPanel import DownloadingPanel

    def qt():
        panel = DownloadingPanel(c.gui, c.new_options)
        control = panel._max_simultaneous_subscriptions
        states = []
        for given in [1, 2, 100, 0, 101, 3]:
            before = c.new_options.GetInteger('max_simultaneous_subscriptions')
            control.setValue(given)
            panel.UpdateOptions()
            states.append({'given': given, 'value': control.value(), 'saved_before': before,
                           'saved_after': c.new_options.GetInteger('max_simultaneous_subscriptions')})
        result = {'minimum': control.minimum(), 'maximum': control.maximum(), 'states': states}
        panel.deleteLater()
        return result

    options = c.CallBlockingToQt(c.gui, qt)
    subs = [S.Subscription(name) for name in ['sub 10', 'sub 2', 'sub 1']]
    manager = S.SubscriptionsManager(c, subs)
    original_now = HydrusTime.GetNow
    original_shutdown = HG.started_shutdown
    HydrusTime.GetNow = lambda: NOW
    HG.started_shutdown = False
    c.new_options.SetBoolean('process_subs_in_random_order', False)
    cases = []
    try:
        for name, limit, running, paused, traffic_paused, editing, due in [
            ('serial default', 1, [], False, False, False, NOW - 4),
            ('full serial', 1, ['sub 1'], False, False, False, NOW - 4),
            ('second slot natural order', 2, ['sub 1'], False, False, False, NOW - 4),
            ('full two slots', 2, ['sub 1', 'sub 2'], False, False, False, NOW - 4),
            ('raise live limit', 3, ['sub 1', 'sub 2'], False, False, False, NOW - 4),
            ('lower live limit preserves running', 1, ['sub 1', 'sub 2'], False, False, False, NOW - 4),
            ('subscriptions paused', 3, ['sub 1'], True, False, False, NOW - 4),
            ('traffic paused', 3, ['sub 1'], False, True, False, NOW - 4),
            ('editing paused', 3, [], False, False, True, NOW - 4),
            ('due buffer exact boundary', 3, [], False, False, False, NOW - 3),
            ('due buffer elapsed', 3, [], False, False, False, NOW - 4),
        ]:
            c.new_options.SetInteger('max_simultaneous_subscriptions', limit)
            c.new_options.SetBoolean('pause_subs_sync', paused)
            c.new_options.SetBoolean('pause_all_new_network_traffic', traffic_paused)
            manager._pause_subscriptions_for_editing = editing
            manager._names_to_running_subscription_info = {n: (None, None) for n in running}
            manager._names_to_next_work_time = {s.GetName(): due for s in subs}
            chosen = manager._GetSubscriptionReadyToGo()
            cases.append({'case': name, 'limit': limit, 'running': running, 'paused': paused,
                          'traffic_paused': traffic_paused, 'editing': editing, 'due': due,
                          'chosen': None if chosen is None else chosen.GetName(),
                          'running_after': list(manager._names_to_running_subscription_info)})
        manager._names_to_running_subscription_info = {}
        manager._pause_subscriptions_for_editing = False
        c.new_options.SetBoolean('pause_subs_sync', False)
        c.new_options.SetBoolean('pause_all_new_network_traffic', False)
        sub = subs[0]
        sub.IsExpectingToWorkInFuture = lambda: True
        sub.GetBestEarliestNextWorkTime = lambda: NOW - 4
        manager._UpdateSubscriptionInfo(sub, just_finished_work=True)
        cooldown = manager._names_to_next_work_time[sub.GetName()] - NOW
        return {'now': NOW, 'options': options, 'cases': cases, 'finished_run_buffer': cooldown}
    finally:
        HydrusTime.GetNow = original_now
        HG.started_shutdown = original_shutdown


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work, 'concurrency.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as stream:
            result = json.load(stream)
    with open(os.path.join(HERE, 'fixtures', 'subscription_concurrency.json'), 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('recorded actual Qt subscription concurrency and manager admission')


if __name__ == '__main__':
    main()
