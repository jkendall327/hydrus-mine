#!/usr/bin/env python3
"""Record the reference's 'Include the file system in this wait' option
(ClientFilesManager._WaitOnWakeup), on the basic fixture.

For each combination of the option, 'Allow wake-from-system-sleep detection'
and a wake delay, a wake from sleep is simulated (the controller's own
SimulateWakeFromSleepEvent) and a file path is asked of the real files
manager. What is recorded is how long the request blocked, in whole seconds,
and whether the controller still thought it had just woken afterwards.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

DELAY = 3


def record(session):
    c = session.controller

    def prepare():
        results = c.Read('media_results_from_ids', [1])
        media = results[0]
        return media.GetHash(), media.GetMime()

    hash, mime = c.CallBlockingToQt(c.gui, prepare)
    out = {'delay': DELAY, 'cases': []}
    old = (c.new_options.GetBoolean('file_system_waits_on_wakeup'), c.new_options.GetBoolean('do_sleep_check'), c.new_options.GetInteger('wake_delay_period'))
    try:
        c.new_options.SetInteger('wake_delay_period', DELAY)
        for waits in (False, True):
            for detect in (False, True):
                c.new_options.SetBoolean('file_system_waits_on_wakeup', waits)
                c.new_options.SetBoolean('do_sleep_check', detect)
                # settle from any earlier case
                c.new_options.SetBoolean('file_system_waits_on_wakeup', False)
                c.SleepCheck()
                time.sleep(DELAY + 1.2)
                c.SleepCheck()
                c.new_options.SetBoolean('file_system_waits_on_wakeup', waits)
                c.SimulateWakeFromSleepEvent()
                started = time.time()
                c.client_files_manager.GetFilePath(hash, mime)
                elapsed = time.time() - started
                out['cases'].append({'file_system_waits': waits, 'detect': detect, 'blocked_seconds': round(elapsed), 'just_woke_after': c.JustWokeFromSleep()})
    finally:
        c.new_options.SetBoolean('file_system_waits_on_wakeup', old[0])
        c.new_options.SetBoolean('do_sleep_check', old[1])
        c.new_options.SetInteger('wake_delay_period', old[2])
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f:
            result = json.load(f)
        with open(os.path.join(HERE, 'fixtures/file_system_wake_wait.json'), 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
            f.write('\n')


if __name__ == '__main__':
    main()
