#!/usr/bin/env python3
"""Record the reference's Database > file maintenance > clear orphan files.

`ClientGUI._ClearOrphanFiles` is run on an empty client with stray files in its file storage,
with the question and the folder picker replaced by scripted answers; the job is the
reference's own `ClientFilesManager.ClearOrphans`. The popup it publishes is sampled while it
works and what is left on disk is recorded, for the move and delete answers, with nothing
to clear, and with the job cancelled while it waits to delete.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller

    def work():
        from qtpy import QtTest as QT
        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        default = next(iter(c.Read('ideal_client_files_locations')[0])).path
        root = tempfile.mkdtemp()
        names = {default: 'default', root: 'root'}

        def norm(text):
            for path, name in sorted(names.items(), key=lambda kv: -len(kv[0])):
                text = text.replace(path, '<' + name + '>')
            return text

        jobs = []
        asked = []
        script = {'choice': None, 'pick': None}
        old = (Q.GetYesYesNo, Q.PickDirectory, c.pub)

        def choose(parent, text, yes_tuples=None, no_label='no', **k):
            asked.append(['choice', text, [t[0] for t in yes_tuples], no_label])
            choice = script['choice']
            if choice is None:
                raise HydrusExceptions.CancelledException()
            assert choice in [t[1] for t in yes_tuples]
            return choice

        def pick(parent, title, *a, **k):
            asked.append(['pick directory', title])
            if script['pick'] is None:
                raise HydrusExceptions.CancelledException()
            return script['pick']

        def pub(topic, *args, **k):
            if topic == 'message' and args and args[0].GetStatusTitle() == 'clearing orphans':
                jobs.append(args[0])
                return None
            return old[2](topic, *args, **k)
        Q.GetYesYesNo, Q.PickDirectory, c.pub = choose, pick, pub

        def tree(base):
            out = []
            for dirpath, _, files in os.walk(base):
                for f in files:
                    out.append(norm(os.path.relpath(os.path.join(dirpath, f), base)))
            return sorted(out)

        def strays():
            for rel in ('fab/' + 'ab' * 32 + '.png', 't00/Thumbs.db'):
                path = os.path.join(default, rel)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                open(path, 'w').write('x')

        steps = []

        def run(name, choice, pick_to=None, make_strays=True, cancel_when=None, wait=240):
            script['choice'], script['pick'] = choice, pick_to
            asked.clear()
            jobs.clear()
            if make_strays:
                strays()
            c.gui._ClearOrphanFiles()
            texts = []
            started = time.monotonic()
            cancelled = False
            while time.monotonic() - started < wait:
                QT.QTest.qWait(50)
                if jobs:
                    job = jobs[0]
                    snapshot = [job.GetStatusText(1) or '', job.GetStatusText(2) or '']
                    snapshot = [norm(t) for t in snapshot]
                    if not texts or texts[-1] != snapshot:
                        texts.append(snapshot)
                    if cancel_when and not cancelled and cancel_when in ' '.join(snapshot):
                        job.Cancel()
                        cancelled = True
                        QT.QTest.qWait(8000)
                    if job.IsDone() or cancelled:
                        break
                elif choice is None or (choice == 'move' and pick_to is None):
                    break
            QT.QTest.qWait(300)
            steps.append(dict(step=name, inputs=dict(choice=choice, pick=None if pick_to is None else norm(pick_to), cancel_when=cancel_when), asked=[[norm(x) if isinstance(x, str) else x for x in a] for a in asked],
                              checked=sorted({t[0] for t in texts if t[0].startswith('checking ')})[:3] + ['...'] + sorted({t[0] for t in texts if t[0].startswith('checking ')})[-2:], num_checked=len({t[0] for t in texts if t[0].startswith('checking ')}), statuses=[t for t in texts if not t[0].startswith('checking ')], done=bool(jobs and jobs[0].IsDone()), cancelled=bool(jobs and jobs[0].IsCancelled()),
                              default_files=tree(default), moved_files=tree(root)))

        try:
            run('forget it', None)
            run('move, cancel the folder picker', 'move', None)
            run('delete', 'delete')
            run('nothing to clear', 'delete', make_strays=False)
            away = os.path.join(root, 'away')
            os.makedirs(away)
            names[away] = 'away'
            run('move to a folder', 'move', away)
            run('delete, cancelled while it waits', 'delete', cancel_when='now deleting')
        finally:
            Q.GetYesYesNo, Q.PickDirectory, c.pub = old
        return dict(steps=steps)
    return c.CallBlockingToQt(c.gui, work)


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
    with open(os.path.join(HERE, 'fixtures', 'clear_orphans.json'), 'w') as f:
        json.dump(value, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded clearing orphans')


if __name__ == '__main__':
    main()
