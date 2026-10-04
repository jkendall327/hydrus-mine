#!/usr/bin/env python3
"""Record the real SystemPanel controls and controller wake detector.

Pinned Qt-thread clock cases cover the one-minute threshold, configured grace
period, zero delay, disabled detection and disabling a pending wake. Only the
user notification and idle reset are intercepted; controller.SleepCheck is real.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.core import HydrusTime
        from hydrus.client.gui.panels.options import SystemPanel
        controller = session.controller
        options = controller.new_options
        panel = SystemPanel.SystemPanel(controller.gui, options)
        controls = {'enabled': panel._do_sleep_check.isChecked(), 'delay': panel._wake_delay_period.value(), 'min': panel._wake_delay_period.minimum(), 'max': panel._wake_delay_period.maximum()}
        original_now = HydrusTime.GetNowMS
        original_show = controller._ShowJustWokeToUser
        original_idle = controller.ResetIdleTimer
        old_enabled = options.GetBoolean('do_sleep_check')
        old_delay = options.GetInteger('wake_delay_period')
        old_last = controller.GetTimestampMS('last_sleep_check')
        old_awake = controller.GetTimestampMS('now_awake')
        old_woke = controller._just_woke_from_sleep
        cases = []
        now = [1700000000000]
        try:
            HydrusTime.GetNowMS = lambda: now[0]
            controller._ShowJustWokeToUser = lambda: None
            controller.ResetIdleTimer = lambda: None
            for enabled, delay, gap in [(True, 15, 60000), (True, 15, 61000), (True, 0, 61000), (True, 60, 61000), (False, 15, 61000)]:
                now[0] = 1700000000000
                panel._do_sleep_check.setChecked(enabled)
                panel._wake_delay_period.setValue(delay)
                panel.UpdateOptions()
                controller.SetTimestampMS('last_sleep_check', now[0] - gap)
                controller.SetTimestampMS('now_awake', 0)
                controller._just_woke_from_sleep = False
                controller.SleepCheck()
                detected = controller._just_woke_from_sleep
                deadline = controller.GetTimestampMS('now_awake')
                touched = controller.GetTimestampMS('last_sleep_check') == now[0]
                detected_at = now[0]
                for elapsed in range(15000, delay * 1000 + 1, 15000):
                    now[0] = detected_at + elapsed
                    controller.SleepCheck()
                now[0] = detected_at + delay * 1000 + 1
                controller.SleepCheck()
                cases.append({'enabled': enabled, 'delay': delay, 'gap_ms': gap, 'detected': detected, 'deadline_ms': deadline, 'last_check_touched': touched, 'after_delay': controller._just_woke_from_sleep})
            panel._do_sleep_check.setChecked(True)
            panel._wake_delay_period.setValue(60)
            panel.UpdateOptions()
            now[0] = 1700000000000
            controller.SetTimestampMS('last_sleep_check', now[0] - 61000)
            controller.SleepCheck()
            pending = controller._just_woke_from_sleep
            panel._do_sleep_check.setChecked(False)
            panel.UpdateOptions()
            controller.SleepCheck()
            disabled_pending = {'before': pending, 'after': controller._just_woke_from_sleep}
            clamps = []
            for value in [-1, 61]:
                panel._wake_delay_period.setValue(value)
                panel.UpdateOptions()
                clamps.append({'typed': value, 'saved': options.GetInteger('wake_delay_period')})
        finally:
            HydrusTime.GetNowMS = original_now
            controller._ShowJustWokeToUser = original_show
            controller.ResetIdleTimer = original_idle
            options.SetBoolean('do_sleep_check', old_enabled)
            options.SetInteger('wake_delay_period', old_delay)
            controller.SetTimestampMS('last_sleep_check', old_last)
            controller.SetTimestampMS('now_awake', old_awake)
            controller._just_woke_from_sleep = old_woke
            panel.deleteLater()
        return {'controls': controls, 'cases': cases, 'disabled_pending': disabled_pending, 'clamps': clamps}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    output = os.path.join(HERE, 'fixtures', 'system_sleep_options.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()
