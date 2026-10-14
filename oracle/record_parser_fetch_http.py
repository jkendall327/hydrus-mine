#!/usr/bin/env python3
"""Record the parser test panels' "fetch from url" over real HTTP.

On the `basic` fixture, with a small local server (`/a b` answers 200 with an
ISO-8859-1 page and sets a cookie; `/error` answers 404 "missing"), the real
reference engine runs the fetches the parser test panels make (new traffic
unpaused; nothing stubbed):

- the page parser editor's "fetch test data from url" (`_FetchExampleData`,
  with a referral url): the page `/a b` (the url encoded as the request),
  then `/error` (which must send the cookie the first set);
- the generic test panel's fetch (a context variable formula editor,
  `TestPanel._FetchFromURL`, its url prompt answered by script) of the same.

Recorded for each fetch: the text the panel holds as its example data, its
example parsing context (url and post index, other keys kept) and the
server's request (the request line and the Referer and Cookie headers).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_parser_fetch_http.py
       (writes fixtures/parser_fetch_http.json)
"""
import http.server, json, os, sys, tempfile, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
PORT = 46017


def serve(log):
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            log.append({'line': self.requestline, 'referer': self.headers.get('Referer'), 'cookie': self.headers.get('Cookie')})
            if self.path.startswith('/error'):
                status, body = 404, b'missing'
            else:
                status, body = 200, '<p>fetched café</p>'.encode('iso-8859-1')
            self.send_response(status)
            self.send_header('Content-Type', 'text/html; charset=iso-8859-1')
            self.send_header('Content-Length', str(len(body)))
            self.send_header('Set-Cookie', 'test-document=saved; Path=/')
            self.send_header('Connection', 'close')
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', PORT), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def record(session):
    import flows
    controller = session.controller
    gui = controller.gui
    log = []
    server = serve(log)
    base = f'http://127.0.0.1:{PORT}'

    def qt(f):
        return controller.CallBlockingToQt(gui, f)

    from hydrus.client.gui.parsing import ClientGUIParsing as Pages, ClientGUIParsingFormulae as Formulae
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.parsing import ClientParsing as P

    if controller.network_engine._pause_all_new_network_traffic:
        qt(controller.network_engine.PausePlayNewJobs)

    out = {'page': [], 'formula': [], 'loopback_ok': flows.loopback_bypasses_proxies()}
    context = {'url': 'https://test-docs.example/original', 'post_index': '12', 'token': 'preserved'}

    def wait_for(fn, timeout=30):
        end = time.time() + timeout
        while time.time() < end:
            if qt(fn):
                return True
            time.sleep(0.1)
        return False

    page = P.PageParser(name='fetch', example_urls=[f'{base}/a b'])
    panel = qt(lambda: Pages.EditPageParserPanel(gui, page, test_data=P.ParsingTestData(dict(context), [''])))
    for raw, referral in ((f' {base}/a b ', 'https://referral.example/from'), (f'{base}/error', '')):
        before = len(log)
        qt(lambda: panel._test_url.setText(raw))
        qt(lambda: panel._test_referral_url.setText(referral))
        qt(panel._FetchExampleData)
        wait_for(lambda: len(log) > before)
        time.sleep(1.5)
        data = qt(lambda: panel._test_panel.GetTestData())
        out['page'].append({'typed': raw.replace(base, '<base>'), 'referral': referral, 'text': data.texts[0], 'context': dict(data.parsing_context),
                            'test_url_after': qt(lambda: panel._test_url.text()).replace(base, '<base>'), 'request': dict(log[before]) if len(log) > before else None,
                            'error_menu': qt(lambda: panel._test_network_job_control._error_button.isEnabled()) if hasattr(panel._test_network_job_control, '_error_button') else None})
    qt(panel.deleteLater)

    formula = qt(lambda: Formulae.EditContextVariableFormulaPanel(gui, True, P.ParseFormulaContextVariable(), P.ParsingTestData(dict(context), [''])))
    for typed in (f' {base}/a b ', f'{base}/error'):
        before = len(log)
        ClientGUIDialogsQuick.EnterText = lambda *a, **kw: typed
        qt(formula._test_panel._FetchFromURL)
        wait_for(lambda: len(log) > before)
        time.sleep(1.5)
        data = qt(lambda: formula._test_panel.GetTestData())
        out['formula'].append({'typed': typed.replace(base, '<base>'), 'text': data.texts[0], 'context': dict(data.parsing_context), 'request': dict(log[before]) if len(log) > before else None})
    qt(formula.deleteLater)
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


def child(out):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f, ensure_ascii=False)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'parser_fetch_http.json'), 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=1)
        f.write('\n')
    print(json.dumps(result, indent=1, ensure_ascii=False))


if __name__ == '__main__':
    main()
