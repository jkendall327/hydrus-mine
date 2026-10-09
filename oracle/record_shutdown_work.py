#!/usr/bin/env python3
"""Record what the reference's exit does about shutdown maintenance.

Boots the `basic` fixture and, on the Qt thread, runs the real main window's
`TryToExit` under each set of the Options > maintenance and processing >
shutdown options ("Run jobs on shutdown", "Only run shutdown jobs once per",
"Max number of minutes to run shutdown jobs") and each time since the last
shutdown work (registered through the real `register_shutdown_work` write at
a held time). The "Maintenance is due" question is answered by script (yes,
no, or cancelled), and the controller's `Exit` is replaced by a note that it
was called. When the exit would do the work, the real `DoIdleShutdownWork`
runs with its database maintenance (`MaintainDB`) noted rather than run
(except in the last case, where it runs for real), time moving on by three
minutes inside it; the record says when it was told to stop and what was
registered after.
"""
import json, os, sys, tempfile, threading
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

NOW = 1767225600  # 2026-01-01T00:00:00Z, held still
DAY = 86400

# name, action (0 never, 1 run, 2 ask first), period seconds, max minutes,
# seconds since the last shutdown work, the answer to "Maintenance is due"
# (if asked), how the exit was asked for
CASES = [
    ('ask first, due, no', 2, DAY, 5, 1_000_000, 'no', 'exit'),
    ('ask first, due, yes', 2, DAY, 5, 1_000_000, 'yes', 'exit'),
    ('ask first, due, cancelled', 2, DAY, 5, 1_000_000, 'cancel', 'exit'),
    ('ask first, exactly one period ago', 2, DAY, 5, DAY, 'no', 'exit'),
    ('ask first, one period and a second ago', 2, DAY, 5, DAY + 1, 'no', 'exit'),
    ('ask first, never done', 2, DAY, 5, None, 'no', 'exit'),
    ('run if needed, due', 1, DAY, 5, 1_000_000, None, 'exit'),
    ('run if needed, not due', 1, DAY, 5, 600, None, 'exit'),
    ('never, due', 0, DAY, 5, 1_000_000, None, 'exit'),
    ('five-minute period, ten minutes ago', 2, 300, 5, 600, 'no', 'exit'),
    ('two-hour period, ten minutes ago', 2, 7200, 5, 600, 'no', 'exit'),
    ('seven minutes', 2, DAY, 7, 1_000_000, 'yes', 'exit'),
    ('ninety minutes', 2, DAY, 90, 1_000_000, 'yes', 'exit'),
    ('a day of minutes', 2, DAY, 1440, 1_000_000, 'yes', 'exit'),
    ('force maintenance, never, not due', 0, DAY, 5, 600, None, 'force'),
    ('force maintenance, ask first, not due', 2, DAY, 20, 600, None, 'force'),
    ('run if needed, due, the work done for real', 1, DAY, 5, 1_000_000, None, 'exit'),
    ('ask first, due, but nothing left to do', 2, DAY, 5, 1_000_000, 'no', 'exit'),
]


def record(session):
    from hydrus.core import HydrusConstants as HC, HydrusGlobals as HG, HydrusTime
    from hydrus.client.gui import ClientGUIDialogsQuick
    from qtpy import QtWidgets as QW
    c = session.controller
    real_time = HydrusTime.time
    held = [None]

    class HeldTime:
        def __getattr__(self, name):
            return getattr(real_time, name)

        def time(self):
            return held[0] if held[0] is not None else real_time.time()

    HydrusTime.time = HeldTime()
    out = []
    for (name, action, period, minutes, ago, answer, how) in CASES:
        row = {'name': name, 'action': action, 'period': period, 'minutes': minutes, 'ago': ago,
               'answer': answer, 'how': how, 'questions': []}
        HC.options['idle_shutdown'] = action
        HC.options['idle_shutdown_max_minutes'] = minutes
        c.new_options.SetInteger('shutdown_work_period', period)
        # registered "ago" seconds back; "never" is registered at 0, which
        # is what the reference reads from its empty table
        held[0] = NOW - (ago if ago is not None else NOW)
        c.WriteSynchronous('register_shutdown_work')
        held[0] = NOW
        row['last_before'] = c.Read('last_shutdown_work_time') - NOW
        HG.do_idle_shutdown_work = False
        HG.restart = False
        exits = []
        real_exit = c.Exit
        c.Exit = lambda: exits.append(True)

        def scripted(win, message, title='Are you sure?', **kwargs):
            row['questions'].append({'title': title, 'message': message,
                                     'kwargs': {k: v for k, v in kwargs.items()}})
            if answer == 'yes':
                result = QW.QDialog.DialogCode.Accepted
            else:
                result = QW.QDialog.DialogCode.Rejected
            if kwargs.get('check_for_cancelled'):
                return (result, answer == 'cancel')
            return result

        real_yes_no = ClientGUIDialogsQuick.GetYesNo
        ClientGUIDialogsQuick.GetYesNo = scripted
        try:
            c.CallBlockingToQt(c.gui, lambda: c.gui.TryToExit(restart=how == 'restart', force_shutdown_maintenance=how == 'force'))
            # the exit is queued after TryToExit returns
            c.CallBlockingToQt(c.gui, lambda: None)
            import time as real
            real.sleep(0.2)
        finally:
            ClientGUIDialogsQuick.GetYesNo = real_yes_no
            c.Exit = real_exit
        row['exited'] = len(exits) > 0
        row['do_shutdown_work'] = bool(HG.do_idle_shutdown_work)
        row['restart'] = bool(HG.restart)
        row['last_after_exit_question'] = c.Read('last_shutdown_work_time') - NOW
        if HG.do_idle_shutdown_work and row['exited']:
            maintain = []
            real_maintain = c.MaintainDB
            for_real = 'for real' in name

            def noted(maintenance_mode=HC.MAINTENANCE_IDLE, stop_time=None):
                maintain.append({'mode': maintenance_mode, 'stop_time': stop_time - NOW})
                if for_real:
                    real_maintain(maintenance_mode=maintenance_mode, stop_time=stop_time)
                held[0] = NOW + 180

            c.MaintainDB = noted
            try:
                c.DoIdleShutdownWork()
                # (its register write is queued; this read follows it)
                row['last_after_work'] = c.Read('last_shutdown_work_time') - NOW
            finally:
                c.MaintainDB = real_maintain
                held[0] = NOW
            row['maintain'] = maintain
        HG.do_idle_shutdown_work = False
        HG.restart = False
        row['work_due_now'] = c.GetIdleShutdownWorkDue(NOW + 300)
        out.append(row)
    held[0] = None
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'shutdown_work.json'), 'w') as f:
        json.dump({'now': NOW, 'cases': result}, f, indent=1, ensure_ascii=False)
        f.write('\n')
    for r in result:
        print(r['name'], r['exited'], r['do_shutdown_work'], [q['title'] for q in r['questions']], r.get('maintain'), r['last_after_exit_question'], r.get('last_after_work'), r['work_due_now'])


if __name__ == '__main__':
    main()
