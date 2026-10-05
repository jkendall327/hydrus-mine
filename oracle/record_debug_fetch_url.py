#!/usr/bin/env python3
"""Actual Help fetch QAction, normal loopback HTTP jobs and response/save/clipboard.

Only dialog answers and the local byte server are authored. NetworkJob, network
engine, threads, completion callback, JobStatus deadlines and toaster are real.
"""
import json
import sqlite3
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
BODY = b'caf\xe9\x00\xff\n'


def record(session):
    requests = []
    release = threading.Event()
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append(dict(path=self.path, headers=dict(self.headers)))
            self.send_response(404 if self.path == "/error" else 200)
            self.send_header('Content-Type', 'text/plain; charset=iso-8859-1')
            self.send_header('Content-Length', str(len(BODY)))
            self.send_header('Set-Cookie', 'debug-fetch=recorded; Path=/')
            self.end_headers()
            if self.path == "/hold":
                release.wait(12)
                return
            self.wfile.write(BODY)
        def log_message(self, *_):
            pass
    server = ThreadingHTTPServer(('127.0.0.1', 80), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base = f'http://127.0.0.1:{server.server_port}'
    def drive():
        from qtpy import QtTest as T, QtWidgets as W
        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsQuick as D, ClientGUIDialogsFiles as F
        c, gui = session.controller, session.controller.gui
        if c.network_engine._pause_all_new_network_traffic:
            c.network_engine.PausePlayNewJobs()
        old = D.EnterText, D.GetYesYesNo, F.FileDialog, c.pub
        jobs, questions, prompts, clips, picks, results = [], [], [], [], [], []
        choice = ['file']
        menu = None
        path = Path(tempfile.mkdtemp()) / 'output.txt'
        def enter(parent, message, **kwargs):
            prompts.append(dict(message=message, title=kwargs.get('title', 'Enter Text'), parent_main=parent is gui))
            if choice[0] == 'cancel':
                raise HydrusExceptions.CancelledException('scripted dialog cancel')
            return base + ('/error' if choice[0] == 'error' else '/hold' if choice[0] == 'cancel-network' else '/bytes')
        def question(parent, message, **kwargs):
            questions.append(dict(message=message, title=kwargs.get('title', 'Are you sure?'), yes_tuples=kwargs['yes_tuples'], no_label=kwargs['no_label'], parent_main=parent is gui, main_visible=gui.isVisible()))
            if choice[0] == 'forget':
                raise HydrusExceptions.CancelledException('scripted forget')
            return choice[0]
        class Picker:
            def __init__(self, parent, title, **kwargs):
                picks.append(dict(title=title, default_filename=kwargs['default_filename'], parent_main=parent is gui))
            def __enter__(self): return self
            def __exit__(self, *_): return False
            def exec(self): return W.QDialog.DialogCode.Accepted
            def GetPath(self): return str(path)
        def pub(topic, *args, **kwargs):
            if topic == 'message' and args[0].GetStatusTitle() == 'debug network job':
                jobs.append(args[0])
            elif topic == 'clipboard':
                clips.append(list(args))
            return old[3](topic, *args, **kwargs)
        def find(m, labels):
            for action in m.actions():
                if action.text() == labels[0]:
                    return action if len(labels) == 1 else find(action.menu(), labels[1:])
            raise ValueError(labels)
        def wait_until(predicate):
            started = time.monotonic()
            while not predicate():
                if time.monotonic() - started > 15:
                    raise TimeoutError(f'actual debug HTTP completion: requests={requests!r}, statuses={[j.GetNetworkJob().GetStatus() for j in jobs]}')
                T.QTest.qWait(20)
        try:
            D.EnterText, D.GetYesYesNo, F.FileDialog, c.pub = enter, question, Picker, pub
            menu, label = gui._InitialiseMenuInfoHelp()
            action = find(menu, ['debug', 'network actions', 'fetch a url'])
            for name in ['file', 'clipboard', 'forget', 'cancel']:
                choice[0] = name
                before = len(questions)
                action.trigger()
                if name == 'cancel':
                    results.append(dict(case=name, jobs=len(jobs), questions=len(questions)))
                    continue
                if name == 'clipboard': gui.hide()
                wait_until(lambda: len(questions) > before)
                job = jobs[-1]
                wait_until(job.IsDone)
                network = job.GetNetworkJob()
                results.append(dict(case=name, done=job.IsDone(), cancellable=job.IsCancellable(), network_done=network.IsDone(), body_hex=network.GetContentBytes().hex(), text=network.GetContentText(), dismiss_at=job._finish_and_dismiss_time, dismissed_at_completion=job.IsDismissed(), main_visible=gui.isVisible()))
                gui.show()
            manager = gui._message_manager
            manager._CheckPending()
            manager._Update()
            T.QTest.qWait(50)
            manager.grab().save(str(HERE / 'fixtures/debug_fetch_url.png'))
            for name in ['error', 'cancel-network']:
                choice[0] = name
                before = len(questions)
                action.trigger()
                wait_until(lambda: len(jobs) == (4 if name == 'error' else 5))
                job = jobs[-1]
                if name == 'cancel-network':
                    wait_until(lambda: any(r['path'] == '/hold' for r in requests))
                    manager._CheckPending()
                    manager._Update()
                    T.QTest.qWait(100)
                    card = next(manager._message_vbox.itemAt(i).widget() for i in range(manager._message_vbox.count()) if manager._message_vbox.itemAt(i).widget().GetJobStatus() is job)
                    card._network_job_ctrl.Cancel()
                network = job.GetNetworkJob()
                wait_until(network.IsDone)
                wait_until(lambda: job._finish_and_dismiss_time is not None)
                results.append(dict(case=name, done=job.IsDone(), network_done=network.IsDone(), network_error=network.HasError(), network_cancelled=network.IsCancelled(), error_type=type(network.GetErrorException()).__name__ if network.HasError() else None, questions_unchanged=len(questions)==before, dismiss_at=job._finish_and_dismiss_time))
            # Real wall-clock expiry, not a replaced timeout/clock.
            wait_until(lambda: all(job.IsDismissed() for job in jobs))
            return dict(menu_path=[label.replace('&',''), 'debug', 'network actions', action.text()], body_hex=BODY.hex(), prompts=prompts, questions=questions, file_picks=picks, saved_hex=path.read_bytes().hex(), clipboard=clips, results=results, all_dismissed_after_real_deadline=all(job.IsDismissed() for job in jobs), requests=[dict(path=r['path'], cookie=r['headers'].get('Cookie')) for r in requests], scheduler='unmodified NetworkJob/engine/CallToThread/CallAfterQtSafe', authored_transport='loopback-only HTTP server and scripted dialog answers')
        finally:
            D.EnterText, D.GetYesYesNo, F.FileDialog, c.pub = old
            for job in jobs: job.FinishAndDismiss()
            gui.show()
            if menu is not None: menu.deleteLater()
            W.QApplication.processEvents()
    try:
        return session.controller.CallBlockingToQt(session.controller.gui, drive)
    finally:
        release.set()
        server.shutdown()
        thread.join()
        server.server_close()


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        database=record_api.unpack_fixture('basic')
        conn=sqlite3.connect(str(Path(database)/'client.db'))
        raw=json.loads(conn.execute('SELECT dictionary_string FROM services WHERE service_type=18').fetchone()[0])
        for key,value in raw[2]:
            if key==[0,'port']:value[1]=None
        conn.execute('UPDATE services SET dictionary_string=? WHERE service_type=18',(json.dumps(raw),));conn.commit();conn.close()
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database,record,network=True)))
        return
    with tempfile.TemporaryDirectory() as directory:
        output=Path(directory)/'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(output))
        result=json.loads(output.read_text())
    (HERE/'fixtures/debug_fetch_url.json').write_text(json.dumps(result,indent=2)+'\n')
    print('wrote debug_fetch_url.json')
if __name__=='__main__': main()
