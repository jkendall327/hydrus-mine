#!/usr/bin/env python3
"""Record the main window status bar's database field and its tooltip.

On the `basic` fixture, the real database thread runs jobs held still by
the recorder (the database's `_Read`/`_Write` wait for it on two actions
of the recorder's own, `status_test` and `status_commit`), and the main
window's real `_RefreshStatusBarDB` fills the field (`GetDBStatus`):

- `idle`: no job;
- `reading`: a read job held in its work;
- `writing`: a write job held in its work;
- `committing`: a write job whose transaction commits after it, held in
  the commit (`TimeToCommit` said yes);
- `locked`: the database paused and disconnected (`PauseAndDisconnect`,
  as a backup does) after a job.

Recorded for each: the field's text and its tooltip.

Then the application-busy field (`GetThreadPoolBusyStatus`), with the
thread pool's working threads stood in for and the clock held: `app_busy`
lists, for each step, the second, how many threads work, and the field's
text and tooltip after the main window's real `_RefreshStatusBar`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_status_db.py
       (writes fixtures/status_db.json)
"""
import json
import os
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    controller = session.controller
    gui = controller.gui
    db = controller.db

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    def field():
        def read():
            gui._RefreshStatusBarDB()
            cell = gui._statusbar._labels[5]
            return {'text': cell.text(), 'tooltip': cell.toolTip()}
        return qt(read)

    def until(fn, timeout=20):
        end = time.time() + timeout
        while time.time() < end:
            if fn():
                return True
            time.sleep(0.05)
        raise AssertionError('timed out')

    hold = threading.Event()
    working = threading.Event()
    real_read, real_write = db._Read, db._Write

    def held_read(action, *args, **kwargs):
        if action == 'status_test':
            working.set()
            hold.wait(30)
            return None
        return real_read(action, *args, **kwargs)

    def held_write(action, *args, **kwargs):
        if action == 'status_test':
            working.set()
            hold.wait(30)
            return None
        if action == 'status_commit':
            return None
        return real_write(action, *args, **kwargs)

    db._Read, db._Write = held_read, held_write
    out = {}
    time.sleep(1)
    out['idle'] = field()

    for name, call in (('reading', lambda: controller.Read('status_test')), ('writing', lambda: controller.WriteSynchronous('status_test'))):
        hold.clear()
        working.clear()
        thread = threading.Thread(target=call, daemon=True)
        thread.start()
        until(working.is_set)
        time.sleep(0.2)
        out[name] = field()
        hold.set()
        thread.join(10)

    # committing: the next commit is held
    wrapper = db._cursor_transaction_wrapper
    real_time, real_commit = wrapper.TimeToCommit, wrapper.CommitAndBegin
    committing = threading.Event()

    def held_commit():
        committing.set()
        hold.wait(30)
        return real_commit()

    hold.clear()
    wrapper.TimeToCommit = lambda: True
    wrapper.CommitAndBegin = held_commit
    thread = threading.Thread(target=lambda: controller.WriteSynchronous('status_commit'), daemon=True)
    thread.start()
    until(committing.is_set)
    time.sleep(0.2)
    out['committing'] = field()
    hold.set()
    thread.join(10)
    wrapper.TimeToCommit, wrapper.CommitAndBegin = real_time, real_commit

    # locked: paused and disconnected after the next job
    db.PauseAndDisconnect(True)
    thread = threading.Thread(target=lambda: controller.Read('status_test'), daemon=True)
    hold.set()
    thread.start()
    thread.join(10)
    until(lambda: db.GetStatus()[0] == 'db locked')
    out['locked'] = field()
    db.PauseAndDisconnect(False)
    until(lambda: db.GetStatus()[0] == '')
    out['after'] = field()

    db._Read, db._Write = real_read, real_write

    from hydrus.core import HydrusTime

    class Thread:
        def __init__(self, working):
            self.working = working

        def CurrentlyWorking(self):
            return self.working

    pool = controller._thread_worker_pool
    real_now = HydrusTime.GetNow
    now = [1770000000]
    HydrusTime.GetNow = lambda: now[0]
    pool._thread_pool_busy_status_text_last_check_time = 0
    out['app_busy'] = []
    for (second, working) in ((0, 2), (5, 9), (10, 9), (11, 9), (100, 3), (120, 4), (140, 8), (160, 9), (180, 1)):
        now[0] = 1770000000 + second

        def step():
            with pool._lock:
                real_threads = pool._call_to_threads
                pool._call_to_threads = [Thread(True) for _ in range(working)] + [Thread(False)]
            try:
                gui._RefreshStatusBar()
            finally:
                with pool._lock:
                    pool._call_to_threads = real_threads
            cell = gui._statusbar._labels[3]
            return {'second': second, 'working': working, 'text': cell.text(), 'tooltip': cell.toolTip()}

        out['app_busy'].append(qt(step))
    HydrusTime.GetNow = real_now
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output_path = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output_path, 'w') as f:
            json.dump(result, f, ensure_ascii=False)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/status_db.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(json.dumps(result, indent=1, ensure_ascii=False))


if __name__ == '__main__':
    main()
