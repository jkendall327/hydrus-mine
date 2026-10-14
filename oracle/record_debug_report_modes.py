#!/usr/bin/env python3
"""Help > debug > report modes, on the real client: what each mode says.

The real Help QAction turns each mode on (so the menu path and the
`_SwitchBoolean` semantics are the reference's), a representative run of the
real code that holds the report points follows, and every `HydrusData.ShowText`
(and `Print`, for the silent network mode) the run makes is read back, in
order. Modes are then turned off and the same run is repeated: nothing.

- cache report mode: a real `DataCache` (adding, evicting the oldest, removing
  a specific item).
- subprocess report mode: the real `RunSubprocess` on a few commands, in a
  scripted environment.
- daemon report mode: a real `CallToThread` job (the only place the reference
  reports; its repeating jobs run on the scheduler).
- file import / file / similar files metadata generation report modes: the
  real Client API `add_file` of a PNG, which runs the real `FileImportJob`,
  `ClientFilesManager` and `ClientImagePerceptualHashes`.
- network report mode (and its silent entry): real `NetworkJob`s against a
  local server: a redirect, a plain fetch, a 404.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_debug_report_modes.py
       (writes fixtures/debug_report_modes.json)
"""
import http.server
import json
import os
import sys
import tempfile
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

PORT = 46031
API_PORT = 45899


def serve(log):
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            log.append(self.path)
            if self.path == '/bounce':
                self.send_response(302)
                self.send_header('Location', '/ok')
                self.send_header('Content-Length', '0')
            elif self.path == '/bad':
                body = b'no such thing'
                self.send_response(404)
                self.send_header('Content-Length', str(len(body)))
            else:
                body = b'fine'
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
            self.send_header('Connection', 'close')
            self.end_headers()
            if self.path not in ('/bounce',):
                self.wfile.write(b'no such thing' if self.path == '/bad' else b'fine')

        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', PORT), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def record(session):
    from qtpy import QtTest as T, QtWidgets as W
    from hydrus.core import HydrusData
    from hydrus.core import HydrusGlobals as HG
    controller = session.controller
    gui = controller.gui
    seen = []

    def show_text(text, *args, **kwargs):
        seen.append(('show', threading.current_thread().name, str(text)))

    def print_(text, *args, **kwargs):
        seen.append(('print', threading.current_thread().name, str(text)))

    def show_exception(e, *args, **kwargs):
        seen.append(('exception', threading.current_thread().name, type(e).__name__ + ': ' + str(e)))

    originals = (HydrusData.ShowText, HydrusData.Print, HydrusData.ShowException, HydrusData.PrintException)
    HydrusData.ShowText, HydrusData.Print, HydrusData.ShowException = show_text, print_, show_exception
    HydrusData.PrintException = lambda e, *a, **k: seen.append(('print_exception', threading.current_thread().name, type(e).__name__ + ': ' + str(e)))

    def find(menu, labels):
        for action in menu.actions():
            if action.text() == labels[0]:
                return action if len(labels) == 1 else find(action.menu(), labels[1:])
        raise ValueError(labels)

    menu_holder = []

    def switch(label):
        """Click the real Help > debug > report modes entry."""
        def go():
            if not menu_holder:
                menu_holder.append(gui._InitialiseMenuInfoHelp())
            menu, _ = menu_holder[0]
            find(menu, ['debug', 'report modes', label]).trigger()
        controller.CallBlockingToQt(gui, go)

    import re

    def tidy(text):
        # (the run's own paths, clock and server make these differ run to run)
        text = re.sub(r'/tmp/hydrus_record_[^/]+/db', '<db>', text)
        text = re.sub(r'/tmp/hydrus\w+/client_api_import_\w+', '<temp>', text)
        text = re.sub(r'Imported at .+?, which was .+? before this check\.', 'Imported at <time>, which was <ago> before this check.', text)
        text = re.sub(r'\nDate: [^\n]*', '\nDate: <date>', text)
        return re.sub(r'\nServer: [^\n]*', '\nServer: <server>', text)

    def taken(thread=None):
        got = [m for m in seen if thread is None or m[1] == thread]
        seen.clear()
        return [{'sink': k, 'text': tidy(t)} for (k, _, t) in got]

    out = {'menu_path': ['help', 'debug', 'report modes']}
    server = serve([])
    base = f'http://127.0.0.1:{PORT}'
    try:
        # ---- cache report mode
        from hydrus.client.caches import ClientCachesBase as Base

        class Item(Base.CacheableObject):
            def __init__(self, size):
                self.size = size

            def GetEstimatedMemoryFootprint(self):
                return self.size

            def IsFinishedLoading(self):
                return True

        def cache_run():
            cache = Base.DataCache(controller, 'oracle cache', 1000)
            steps = []
            for name, size in (('a', 400), ('b', 400), ('c', 400), ('d', 100)):
                seen.clear()
                cache.AddData(name, Item(size))
                steps.append({'step': f'add {name} ({size} bytes)', 'messages': taken()})
            cache.DeleteData('c')
            steps.append({'step': 'delete c', 'messages': taken()})
            return steps

        out['cache'] = {'size_limit': 1000, 'off': cache_run()}
        taken()
        switch('cache report mode')
        assert HG.cache_report_mode
        out['cache']['on'] = cache_run()
        switch('cache report mode')

        # ---- subprocess report mode
        from hydrus.core.processes import HydrusSubprocess as Sub
        from hydrus.core import HydrusBoot
        saved_env = dict(os.environ)
        saved_path = HydrusBoot.ORIGINAL_PATH
        os.environ.clear()
        os.environ.update({'HOME': '/home/oracle', 'LANG': 'C', 'PATH': '/usr/bin:/bin'})
        HydrusBoot.ORIGINAL_PATH = None
        out['subprocess'] = {'environment': dict(os.environ), 'commands': []}
        commands = [['echo', 'hello'], ['sh', '-c', 'echo oops 1>&2'], ['true']]

        def sub_run(off):
            for cmd in commands:
                taken()
                Sub.RunSubprocess(cmd, text=True)
                out['subprocess']['commands'].append({'cmd': cmd, 'on': not off, 'messages': taken()})
        try:
            sub_run(True)
            switch('subprocess report mode')
            assert HG.subprocess_report_mode
            sub_run(False)
            switch('subprocess report mode')
        finally:
            os.environ.clear()
            os.environ.update(saved_env)
            HydrusBoot.ORIGINAL_PATH = saved_path

        # ---- daemon report mode: the pool the reference reports from
        def pool_job(label):
            done = threading.Event()
            taken()
            controller.CallToThread(done.set)
            assert done.wait(10)
            time.sleep(0.3)
            return [m for m in taken() if True]
        out['daemon'] = {'off': pool_job('off')}
        switch('daemon report mode')
        assert HG.daemon_report_mode
        out['daemon']['on'] = pool_job('on')
        switch('daemon report mode')

        # ---- network report mode, loud then silent
        from hydrus.client.networking import ClientNetworkingJobs as J
        engine = controller.network_engine
        if engine._pause_all_new_network_traffic:
            controller.CallBlockingToQt(gui, engine.PausePlayNewJobs)

        def fetch(path):
            job = J.NetworkJob('GET', base + path)
            job.OverrideBandwidth()
            engine.AddJob(job)
            end = time.time() + 20
            while not job.IsDone() and time.time() < end:
                time.sleep(0.05)
            time.sleep(1.5)  # (the engine says 'done' after the job is)

        def network_run(label):
            runs = []
            for path in ('/bounce', '/ok', '/bad'):
                taken()
                fetch(path)
                runs.append({'path': path, 'messages': [dict(m, text=m['text'].replace(base, '<base>')) for m in taken()]})
            return runs
        out['network'] = {'off': network_run('off')}
        switch('network report mode')
        assert HG.network_report_mode and not HG.network_report_mode_silent
        out['network']['on'] = network_run('on')
        # (the two entries share the mode: the plain one flips it, so a second
        # click turns it off, and the silent one then turns it on, silently)
        switch('network report mode')
        assert not HG.network_report_mode
        switch('network report mode (silent)')
        assert HG.network_report_mode and HG.network_report_mode_silent
        out['network']['silent'] = network_run('silent')
        switch('network report mode (silent)')
        assert not HG.network_report_mode

        # ---- file import, file and phash modes: a real add_file
        import hydrus_driver
        from PIL import Image
        scratch = Path(tempfile.mkdtemp(prefix='report_modes_'))

        def make_png(name, shade):
            image = Image.new('RGB', (64, 48))
            image.putdata([((x * 4 + shade) % 256, (y * 5) % 256, shade) for y in range(48) for x in range(64)])
            image.save(scratch / name)
            return str(scratch / name)
        path = make_png('report_modes_new.png', 17)
        import shutil
        shutil.copy(path, HERE / 'fixtures/debug_report_modes.png')

        def import_run(path):
            taken()
            status, _, body = session.api.request('POST', '/add_files/add_file', json_body={'path': path})
            time.sleep(1.0)
            return {'api_status': status, 'messages': taken()}

        out['import'] = {'path': 'debug_report_modes.png'}
        for label in ('file import report mode', 'file report mode', 'similar files metadata generation report mode'):
            switch(label)
        assert HG.file_import_report_mode and HG.file_report_mode and HG.phash_generation_report_mode
        out['import']['on'] = import_run(path)
        # a second import of the same file (now already in the database)
        out['import']['again'] = import_run(path)
        for label in ('file import report mode', 'file report mode', 'similar files metadata generation report mode'):
            switch(label)
        assert not (HG.file_import_report_mode or HG.file_report_mode or HG.phash_generation_report_mode)
        out['import']['off'] = import_run(make_png('report_modes_other.png', 90))
        return out
    finally:
        HydrusData.ShowText, HydrusData.Print, HydrusData.ShowException, HydrusData.PrintException = originals
        for name in ('cache', 'subprocess', 'daemon', 'file_import', 'file', 'gui', 'phash_generation'):
            setattr(HG, name + '_report_mode', False)
        HG.network_report_mode = HG.network_report_mode_silent = False
        server.shutdown()
        W.QApplication.processEvents()


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        destination = sys.argv[2]  # (run_client rewrites sys.argv)
        database = record_api.unpack_fixture('basic')
        result = hydrus_driver.run_client(database, lambda s: session_record(s), port=API_PORT, network=True)
        Path(destination).write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/debug_report_modes.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote debug_report_modes.json')


def session_record(session):
    return record(session)


if __name__ == '__main__':
    main()
