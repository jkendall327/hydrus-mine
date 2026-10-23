#!/usr/bin/env python3
"""Record the page parser editor's "fetch example data" controls over real HTTP.

On the `basic` fixture, with new traffic unpaused, the real page parser
editor (`EditPageParserPanel`, one example url `<base>/a b`) fetches from a
small local server through the real engine:

- `blank`: the url box empty: the box takes the first example url, and
  nothing is asked of the server;
- `progress`: `/slow` sends its headers (4096 bytes long), then holds
  (the reference reads its body in large chunks, so a part sent would not
  show yet): the network job control's left and right texts, gauge and
  cancel button while it holds;
- `cancel`: the control's cancel button pressed while `/hold` holds: the
  test document, its context and the control afterwards;
- `once`: `/drop` closes each connection without an answer: how often the
  server was asked (the editor's job tries its connection once) and the
  test document;
- `invalid`: the url box 'invalid';
- `close`: `/hold` again, and the editor closed while it holds: whether
  the job is cancelled and still in the engine two seconds on.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_parser_fetch_controls.py
       (writes fixtures/parser_fetch_controls.json)
"""
import http.server
import json
import os
import socket
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def serve(log, release):
    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = 'HTTP/1.1'

        def do_GET(self):
            log.append(self.path)
            if self.path.startswith('/hold'):
                release.wait(60)
                body = b'late'
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            elif self.path.startswith('/slow'):
                self.send_response(200)
                self.send_header('Content-Type', 'text/html')
                self.send_header('Content-Length', '4096')
                self.end_headers()
                self.wfile.flush()
                release.wait(60)
            elif self.path.startswith('/drop'):
                self.close_connection = True
                self.connection.shutdown(socket.SHUT_RDWR)
            else:
                body = b'<p>ok</p>'
                self.send_response(200)
                self.send_header('Content-Type', 'text/html')
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def record(session):
    for key in [k for k in os.environ if k.lower().endswith('_proxy')]:
        del os.environ[key]
    controller = session.controller
    gui = controller.gui
    engine = controller.network_engine
    if engine._pause_all_new_network_traffic:
        engine.PausePlayNewJobs()
    log = []
    release = threading.Event()
    server = serve(log, release)
    base = 'http://127.0.0.1:{}'.format(server.server_address[1])

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    def wait_for(fn, timeout=30):
        end = time.time() + timeout
        while time.time() < end:
            if qt(fn):
                return True
            time.sleep(0.05)
        return False

    from hydrus.client.gui.parsing import ClientGUIParsing as Pages
    from hydrus.client.parsing import ClientParsing as P

    context = {'url': 'https://test-docs.example/original', 'post_index': '12'}

    def make():
        page = P.PageParser(name='fetch', example_urls=[f'{base}/a b'])
        return qt(lambda: Pages.EditPageParserPanel(gui, page, test_data=P.ParsingTestData(dict(context), ['previous'])))

    def control(panel):
        c = panel._test_network_job_control
        return {
            'left': c._left_text.text(),
            'right': c._right_text.text() if c._right_text.isVisible() or c._right_text.text() else '',
            'gauge': [c._gauge.value(), c._gauge.maximum()],
            'can_cancel': c._cancel_button.isEnabled(),
        }

    def document(panel):
        data = panel._test_panel.GetTestData()
        return {'text': data.texts[0], 'context': dict(data.parsing_context), 'test_url': panel._test_url.text()}

    out = {}
    panel = make()

    # blank
    before = len(log)
    qt(lambda: panel._test_url.setText(''))
    qt(panel._FetchExampleData)
    time.sleep(1)
    out['blank'] = dict(document(panel), asked=len(log) - before)

    # progress, then cancel from the control
    for name, path in (('progress', '/slow'), ('cancel', '/hold')):
        release.clear()
        before = len(log)
        qt(lambda: panel._test_url.setText(base + path))
        qt(panel._FetchExampleData)
        wait_for(lambda: len(log) > before)
        time.sleep(1.5)
        qt(panel._test_network_job_control._Update)
        held = qt(lambda: control(panel))
        qt(panel._test_network_job_control.Cancel)
        time.sleep(1.5)
        release.set()
        out[name] = dict(document(panel), holding=held, after=qt(lambda: control(panel)))

    # one connection attempt
    before = len(log)
    started = time.time()
    qt(lambda: panel._test_url.setText(base + '/drop'))
    qt(panel._FetchExampleData)
    wait_for(lambda: panel._test_panel.GetTestData().texts[0].startswith('fetch failed'))
    time.sleep(1)
    out['once'] = dict(document(panel), asked=len(log) - before, error_shown=qt(lambda: panel._test_network_job_control._error_text is not None), after=qt(lambda: control(panel)))
    out['once']['text'] = out['once']['text'].split('\n')[0]

    # invalid url
    before = len(log)
    qt(lambda: panel._test_url.setText('invalid'))
    try:
        qt(panel._FetchExampleData)
        out['invalid'] = {'raised': None}
    except Exception as e:
        out['invalid'] = {'raised': type(e).__name__, 'message': str(e)}
    time.sleep(1.5)
    out['invalid'].update(document(panel))
    out['invalid']['asked'] = len(log) - before
    out['invalid']['text'] = out['invalid']['text'].split('\n')[0]

    # close the editor while it fetches
    release.clear()
    before = len(log)
    qt(lambda: panel._test_url.setText(base + '/hold'))
    qt(panel._FetchExampleData)
    wait_for(lambda: len(log) > before)
    job = qt(lambda: panel._test_network_job_control._network_job)
    qt(panel.deleteLater)
    time.sleep(2)
    in_engine = any(j is job for (_, j) in engine.GetJobsSnapshot())
    out['close'] = {'cancelled': job.IsCancelled(), 'done': job.IsDone(), 'in_engine': in_engine}
    release.set()
    time.sleep(1)
    server.shutdown()

    def normal(v):
        if isinstance(v, str):
            return v.replace(base, '<base>')
        if isinstance(v, dict):
            return {k: normal(x) for k, x in v.items()}
        if isinstance(v, list):
            return [normal(x) for x in v]
        return v

    return normal(out)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output_path = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record, network=True)
        with open(output_path, 'w') as f:
            json.dump(result, f, ensure_ascii=False)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/parser_fetch_controls.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(json.dumps(result, indent=1, ensure_ascii=False))


if __name__ == '__main__':
    main()
