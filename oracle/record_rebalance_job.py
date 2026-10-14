#!/usr/bin/env python3
"""Record the reference's "move files now" job (`MoveMediaFilesPanel._Rebalance`).

On an empty client with a second media location of the same weight, the real
panel is asked to move files now with its run-time question answered by
script, and the job it hands the client (`do_file_storage_rebalance`) is run
by the real `client_files_manager.Rebalance`, as the GUI's thread would. The
status text it sets before each move is recorded, with the job's end:

- a job whose stop time has passed (the job as the panel makes it for "run
  for 10 minutes", its stop time then moved back): it moves nothing;
- a job the user cancels after its third move: it stops there;
- "run for 10 minutes" (its stop time in the future): it moves the rest;
- again, with nothing left: it moves nothing.

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
    Q.GetYesYesNo = lambda parent, text, yes_tuples=None, no_label='no', **k: dict(yes_tuples)['run for 10 minutes']

    runs = []
    try:
        def make_job():
            captured.clear()
            panel = P.MoveMediaFilesPanel(c.gui, c)
            panel._OKParent = lambda: None
            panel._Rebalance()
            panel.deleteLater()
            return captured[0]

        def run(name, tweak):
            job = qt(make_job)
            stop_in = None if job._stop_time is None else job._stop_time - HydrusTime.GetNow()
            texts = []
            tweak(job, texts)
            real = job.SetStatusText

            def say(text, *a, **k):
                texts.append(norm(text))
                return real(text, *a, **k)

            job.SetStatusText = say
            before = c.client_files_manager.RebalanceWorkToDo()
            c.client_files_manager.Rebalance(job)
            runs.append(dict(
                name=name,
                work_to_do_before=before,
                stop_in_when_made=round(stop_in) if stop_in is not None else None,
                texts=texts,
                moves=[t for t in texts if t.startswith('Moving')],
                last=texts[-1] if texts else None,
                finished=job.IsDone(),
                cancelled=job.IsCancelled(),
                work_to_do_after=c.client_files_manager.RebalanceWorkToDo(),
            ))

        def stop_passed(job, texts):
            job._stop_time = HydrusTime.GetNow() - 1

        def cancel_after_three(job, texts):
            real = job.SetStatusText
            n = {'moves': 0}

            def wrapped(text, *a, **k):
                if text.startswith('Moving'):
                    n['moves'] += 1
                    if n['moves'] == 4:
                        job.Cancel()
                return real(text, *a, **k)

            job.SetStatusText = wrapped

        run('stop time passed', stop_passed)
        run('cancelled at the fourth move', cancel_after_three)
        run('ten minutes', lambda job, texts: None)
        run('ten minutes again, nothing left', lambda job, texts: None)
    finally:
        c.pub = old_pub
        Q.GetYesYesNo = old_yyn
    return dict(runs=runs)


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
    for run in value['runs']:
        run.pop('texts', None)
    with open(os.path.join(HERE, 'fixtures', 'rebalance_job.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded the rebalance job')


if __name__ == '__main__':
    main()
