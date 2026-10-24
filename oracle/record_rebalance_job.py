#!/usr/bin/env python3
"""Record the reference's "move files now" job (`MoveMediaFilesPanel._Rebalance`).

On an empty client with a second media location of the same weight, the real
panel is asked to move files now with its run-time question answered by
script, and the job it hands the client (`do_file_storage_rebalance`) is run
by the real `client_files_manager.Rebalance`, as the GUI's thread would. The
clock (`HydrusTime.GetNow`) is held at a fixed time while the panel makes the
job, so its stop time is recorded as seconds after that, for each run-time
choice ("run for custom time" with its dialog accepted as it opens); then each
job is run with the clock held at its stop time, or one second past it, and the
status texts it sets are recorded with the job's end:

- one second past its stop time, a job moves nothing;
- at its stop time it still moves; the user cancels it as a chosen move
  begins (the fourth, or the first), and it stops after that move;
- "run indefinitely" moves the rest, and again has nothing to move.

Last, a job is handed to the main window's own handler
(`DoFileStorageRebalance`): its popup title, and whether the locations
dialog opens again once the job is done (`after_the_job`).

Every run ends with the status text "done!" and the job finished and
dismissed. Directories the run made are written as `<name>`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_rebalance_job.py
       (writes fixtures/rebalance_job.json)
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def qt(f):
        return c.CallBlockingToQt(c.gui, f)

    from hydrus.core import HydrusTime
    from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q
    from qtpy import QtWidgets as QW
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
    from hydrus.client.gui.panels import ClientGUIFilesPhysicalStoragePanels as P
    from hydrus.client.files import ClientFilesPhysical as FP

    root = tempfile.mkdtemp()
    names = {}
    other = os.path.join(root, 'a')
    os.makedirs(other)
    default = next(iter(c.Read('ideal_client_files_locations')[0])).path
    names[other] = 'a'
    names[default] = 'default'
    names[c.GetDBDir()] = 'db'

    def norm(text):
        for path, name in sorted(names.items(), key=lambda kv: -len(kv[0])):
            text = text.replace(path, '<' + name + '>')
        return text

    c.Write('ideal_client_files_locations', [FP.FilesStorageBaseLocation(default, 1), FP.FilesStorageBaseLocation(other, 1)], None)
    c.client_files_manager.Reinit()

    captured = []
    old_pub = c.pub

    def pub(topic, *args, **k):
        if topic == 'do_file_storage_rebalance':
            captured.append(args[0])
            return None
        return old_pub(topic, *args, **k)

    c.pub = pub
    old_yyn = Q.GetYesYesNo
    old_exec = T.DialogEdit.exec
    old_now = HydrusTime.GetNow
    choice = {'label': None}
    asked = []
    Q.GetYesYesNo = lambda parent, text, yes_tuples=None, no_label='no', **k: dict(yes_tuples)[choice['label']]

    def custom_exec(dialog):
        # "run for custom time": its dialog, accepted as it opens
        asked.append(dialog.windowTitle())
        return QW.QDialog.DialogCode.Accepted

    T.DialogEdit.exec = custom_exec

    MADE = 1_000_000_000

    runs = []
    try:
        def make_job():
            captured.clear()
            panel = P.MoveMediaFilesPanel(c.gui, c)
            panel._OKParent = lambda: None
            HydrusTime.GetNow = lambda: MADE
            try:
                panel._Rebalance()
            finally:
                HydrusTime.GetNow = old_now
            panel.deleteLater()
            return captured[0]

        def run(name, label, at, cancel_at_move=None):
            # the job made with `label` chosen at MADE, run with the clock at MADE + `at`
            # (None: the real clock), the user cancelling it as move `cancel_at_move` begins
            choice['label'] = label
            asked.clear()
            job = qt(make_job)
            stop_in = None if job._stop_time is None else job._stop_time - MADE
            texts = []
            real = job.SetStatusText
            n = {'moves': 0}

            def say(text, *a, **k):
                texts.append(norm(text))
                if text.startswith('Moving'):
                    n['moves'] += 1
                    if n['moves'] == cancel_at_move:
                        job.Cancel()
                return real(text, *a, **k)

            job.SetStatusText = say
            before = c.client_files_manager.RebalanceWorkToDo()
            if at is not None:
                HydrusTime.GetNow = lambda: MADE + at
            try:
                c.client_files_manager.Rebalance(job)
            finally:
                HydrusTime.GetNow = old_now
            runs.append(dict(
                name=name,
                choice=label,
                custom_dialog=list(asked),
                clock_after_made=at,
                work_to_do_before=before,
                stop_in_when_made=stop_in,
                texts=texts,
                moves=[t for t in texts if t.startswith('Moving')],
                last=texts[-1] if texts else None,
                finished=job.IsDone(),
                cancelled=job.IsCancelled(),
                work_to_do_after=c.client_files_manager.RebalanceWorkToDo(),
            ))

        run('10 minutes, one second past its stop time', 'run for 10 minutes', 601)
        run('10 minutes, at its stop time, cancelled at the fourth move', 'run for 10 minutes', 600, 4)
        run('30 minutes, one second past its stop time', 'run for 30 minutes', 1801)
        run('30 minutes, at its stop time, cancelled at the first move', 'run for 30 minutes', 1800, 1)
        run('1 hour, one second past its stop time', 'run for 1 hour', 3601)
        run('1 hour, at its stop time, cancelled at the first move', 'run for 1 hour', 3600, 1)
        run('custom time, one second past its stop time', 'run for custom time', 7201)
        run('custom time, at its stop time, cancelled at the first move', 'run for custom time', 7200, 1)
        run('indefinitely', 'run indefinitely', None)
        run('indefinitely again, nothing left', 'run indefinitely', None)

        # the main window's own handler, as the panel's publication reaches it: the job
        # runs in its dialog, and once it is done the locations dialog opens again
        reopened = []
        old_move = c.gui._MoveMediaFiles
        c.gui._MoveMediaFiles = lambda: reopened.append(True)
        try:
            choice['label'] = 'run indefinitely'
            job = qt(make_job)
            qt(lambda: c.gui.DoFileStorageRebalance(job))
        finally:
            c.gui._MoveMediaFiles = old_move
        after = dict(title=job.GetStatusTitle(), finished=job.IsDone(), reopened_locations=len(reopened))
    finally:
        c.pub = old_pub
        Q.GetYesYesNo = old_yyn
        T.DialogEdit.exec = old_exec
        HydrusTime.GetNow = old_now
    return dict(runs=runs, after_the_job=after)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        value = hydrus_driver.run_client(os.path.join(tempfile.mkdtemp(), 'db'), record)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'rebalance_job.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the rebalance job')


if __name__ == '__main__':
    main()
