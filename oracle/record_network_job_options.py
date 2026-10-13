#!/usr/bin/env python3
"""Record the reference's real NetworkJob under the Options > connection settings.

Real `NetworkJob`s are queued on the running client's own network engine and
run to the end against a loopback server (and a raw loopback proxy) that count
what they are asked. Only the server is authored. Each scenario sets the
options the way the reference stores them, then records what the job did: how
often the server was asked, every status text the job showed, how long it took,
how it ended, how many ran at once, and where proxied requests went.
"""
import json
import os
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


class Server:
    def __init__(self, tls=None):
        self.hits = {}
        self.paths = []
        self.active = 0
        self.max_active = 0
        self.lock = threading.Lock()
        outer = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = 'HTTP/1.1'

            def log_message(self, *_):
                pass

            def reply(self, code, body=b'no'):
                self.send_response(code)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                with outer.lock:
                    outer.hits[self.path] = outer.hits.get(self.path, 0) + 1
                    outer.paths.append(self.path)
                if self.path.startswith('/status/'):
                    self.reply(int(self.path.split('/')[2]))
                elif self.path == '/hold':
                    with outer.lock:
                        outer.active += 1
                        outer.max_active = max(outer.max_active, outer.active)
                    time.sleep(1.5)
                    with outer.lock:
                        outer.active -= 1
                    self.reply(200, b'held')
                elif self.path == '/drop':
                    self.close_connection = True
                    self.connection.shutdown(socket.SHUT_RDWR)
                elif self.path == '/stall':
                    time.sleep(25)
                    self.reply(200, b'late')
                else:
                    self.reply(200, self.path.encode())

        self.http = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.http.daemon_threads = True
        if tls:
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(*tls)
            self.http.socket = context.wrap_socket(self.http.socket, server_side=True)
        threading.Thread(target=self.http.serve_forever, daemon=True).start()
        self.port = self.http.server_port
        self.base = f'{"https" if tls else "http"}://127.0.0.1:{self.port}'

    def count(self, path):
        return self.hits.get(path, 0)


def record(session):
    # the sandbox names proxies in the environment, which requests prefers over a session's
    for key in [k for k in os.environ if k.lower().endswith('_proxy')]:
        del os.environ[key]
    c = session.controller
    from hydrus.client.networking import ClientNetworkingJobs as J
    options = c.new_options
    engine = c.network_engine
    if engine._pause_all_new_network_traffic:
        engine.PausePlayNewJobs()
    options.SetBoolean('pause_all_new_network_traffic', False)
    server = Server()
    certs = tempfile.mkdtemp()
    subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', f'{certs}/k.pem', '-out', f'{certs}/c.pem',
                    '-days', '2', '-subj', '/CN=127.0.0.1', '-addext', 'subjectAltName=IP:127.0.0.1'], check=True, capture_output=True)
    secure = Server(tls=(f'{certs}/c.pem', f'{certs}/k.pem'))
    held = socket.socket()
    held.bind(('127.0.0.1', 0))
    closed = f'http://127.0.0.1:{held.getsockname()[1]}/x'

    def clear_errors():
        with engine.domain_manager._lock:
            engine.domain_manager._domains_to_network_infrastructure_errors.clear()

    def settings(**kw):
        for key, value in kw.items():
            if isinstance(value, bool):
                options.SetBoolean(key, value)
            elif isinstance(value, str) or value is None:
                options.SetNoneableString(key, value)
            else:
                options.SetInteger(key, value)
        engine.RefreshOptions()
        engine.session_manager.ReinitialiseProxies()

    def run(url, limit=60, hold=None):
        job = J.NetworkJob('GET', url)
        job.OverrideBandwidth()
        statuses = []
        wakes = []
        started = time.monotonic()
        start_second = int(time.time())
        engine.AddJob(job)
        done = threading.Event()

        def sample():
            while not done.is_set():
                s = job.GetStatus()[0]
                if not statuses or statuses[-1] != s:
                    statuses.append(s)
                for name, wake in (('connection', job._connection_error_wake_time), ('serverside', job._serverside_bandwidth_wake_time)):
                    if wake and (not wakes or not any(w[1] == name and w[3] == wake for w in wakes[-2:])):
                        wakes.append([round(time.monotonic() - started, 2), name, wake - int(time.time()), wake])
                time.sleep(0.02)
        threading.Thread(target=sample, daemon=True).start()
        while not job.IsDone() and time.monotonic() - started < limit:
            if hold and time.monotonic() - started > hold:
                job.Cancel()
            time.sleep(0.02)
        done.set()
        error = job.GetErrorException() if job.HasError() else None
        return dict(statuses=statuses, wakes=wakes, connection_attempts=job._current_connection_attempt_number, request_attempts=job._current_request_attempt_number, seconds=time.monotonic() - started, error=type(error).__name__ if error else None,
                    done=job.IsDone(), text=job.GetContentText() if job.IsDone() and not error and not job.IsCancelled() else None)

    def many(url, n=6):
        jobs = [J.NetworkJob('GET', url) for _ in range(n)]
        for job in jobs:
            job.OverrideBandwidth()
            engine.AddJob(job)
        started = time.monotonic()
        while not all(j.IsDone() for j in jobs) and time.monotonic() - started < 60:
            time.sleep(0.02)

    base_settings = dict(max_connection_attempts_allowed=3, connection_error_wait_time=1, max_request_attempts_allowed_get=2,
                         serverside_bandwidth_wait_time=1, network_timeout=10, max_network_jobs=15, max_network_jobs_per_domain=5,
                         domain_network_infrastructure_error_number=0, domain_network_infrastructure_error_time_delta=600,
                         http_proxy=None, https_proxy=None, no_proxy='127.0.0.1', verify_regular_https=True)
    out = {'defaults': {k: (options.GetNoneableString(k) if k in ('http_proxy', 'https_proxy', 'no_proxy') else options.GetBoolean(k) if k == 'verify_regular_https' else options.GetInteger(k)) for k in base_settings}}
    settings(**base_settings)

    out['connection'] = []
    for attempts, wait in ((3, 1), (2, 1), (2, 2)):
        settings(max_connection_attempts_allowed=attempts, connection_error_wait_time=wait)
        clear_errors()
        out['connection'].append(dict(attempts=attempts, wait=wait, **run(closed)))
    settings(max_connection_attempts_allowed=3, connection_error_wait_time=1)

    out['retries'] = []
    for retries in (2, 4):
        settings(max_request_attempts_allowed_get=retries)
        before = server.count('/status/503')
        clear_errors()
        out['retries'].append(dict(retries=retries, **run(server.base + '/status/503'), asked=server.count('/status/503') - before))

    out['bandwidth'] = []
    for wait in (1, 3):
        settings(max_request_attempts_allowed_get=2, serverside_bandwidth_wait_time=wait)
        before = server.count('/status/429')
        out['bandwidth'].append(dict(wait=wait, **run(server.base + '/status/429'), asked=server.count('/status/429') - before))
    settings(serverside_bandwidth_wait_time=1)

    out['simultaneous'] = []
    for overall, per_domain in ((2, 5), (5, 3)):
        settings(max_network_jobs=overall, max_network_jobs_per_domain=per_domain)
        server.max_active = 0
        many(server.base + '/hold')
        out['simultaneous'].append(dict(overall=overall, per_domain=per_domain, max_active=server.max_active))
    settings(max_network_jobs=15, max_network_jobs_per_domain=5)

    out['domain_halt'] = []
    for number, delta in ((2, 1200), (3, 1200), (0, 1200)):
        clear_errors()
        settings(domain_network_infrastructure_error_number=number, domain_network_infrastructure_error_time_delta=delta, max_connection_attempts_allowed=3, connection_error_wait_time=1)
        failed = run(server.base + '/drop')
        ok = engine.domain_manager.DomainOK(server.base)
        probe = run(server.base + '/uri', limit=20, hold=1.5)
        # server errors (500) do not count as domain events
        clear_errors()
        settings(max_request_attempts_allowed_get=1)
        for _ in range(3):
            run(server.base + '/status/500')
        server_errors_ok = engine.domain_manager.DomainOK(server.base)
        settings(max_request_attempts_allowed_get=2)
        out['domain_halt'].append(dict(number=number, delta=delta, failed=failed, domain_ok=ok, probe=probe, ok_after_three_500s=server_errors_ok))
    clear_errors()
    settings(domain_network_infrastructure_error_number=0, max_connection_attempts_allowed=3, connection_error_wait_time=1)

    settings(network_timeout=1, max_request_attempts_allowed_get=1)
    out['timeout'] = run(server.base + '/stall', limit=40)
    settings(network_timeout=10, max_request_attempts_allowed_get=2)

    out['proxy'] = []
    settings(http_proxy=server.base)
    before = server.count('http://booru.invalid/uri')
    out['proxy'].append(dict(case='http proxy, other host', **run('http://booru.invalid/uri'), proxy_asked=server.count('http://booru.invalid/uri') - before, paths=server.paths[-1:]))
    before = server.count('/uri')
    out['proxy'].append(dict(case='http proxy, host in default no_proxy', **run(server.base + '/uri'), direct_asked=server.count('/uri') - before))
    settings(http_proxy=server.base, no_proxy='booru.invalid,127.0.0.1', max_connection_attempts_allowed=1)
    before = server.count('http://booru.invalid/uri')
    out['proxy'].append(dict(case='http proxy, host in no_proxy', **run('http://booru.invalid/uri', limit=30), proxy_asked=server.count('http://booru.invalid/uri') - before))
    settings(http_proxy=None, no_proxy=None)
    before = server.count('http://booru.invalid/uri')
    out['proxy'].append(dict(case='proxy cleared', **run('http://booru.invalid/uri', limit=30), proxy_asked=server.count('http://booru.invalid/uri') - before))
    settings(max_connection_attempts_allowed=3, no_proxy='127.0.0.1')

    # an https proxy that notes what it is asked and refuses
    listener = socket.socket()
    listener.bind(('127.0.0.1', 0))
    listener.listen()
    asked = []

    def proxy():
        while True:
            try:
                conn, _ = listener.accept()
            except OSError:
                return
            line = conn.makefile('rb').readline().decode().strip()
            asked.append(line)
            conn.sendall(b'HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n')
            conn.close()
    threading.Thread(target=proxy, daemon=True).start()
    settings(https_proxy=f'http://127.0.0.1:{listener.getsockname()[1]}', max_connection_attempts_allowed=1, max_request_attempts_allowed_get=1)
    result = run('https://booru.invalid/x', limit=30)
    time.sleep(0.5)
    out['https_proxy'] = dict(**result, connect_lines=list(asked))
    settings(https_proxy=None)
    asked.clear()
    result = run('https://booru.invalid/x', limit=30)
    time.sleep(0.5)
    out['https_proxy_cleared'] = dict(**result, connect_lines=list(asked))
    listener.close()
    settings(max_connection_attempts_allowed=1, max_request_attempts_allowed_get=1)

    # certificates: a self-signed server fails to verify, and goes through when told not to
    out['verify'] = []
    for verify in (True, False):
        settings(verify_regular_https=verify)
        out['verify'].append(dict(verify=verify, **run(secure.base + '/uri', limit=30), asked=secure.count('/uri')))
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record, network=True)
        with open(output, 'w') as f:
            json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'network_job_options.json'), 'w') as f:
        json.dump(value, f, indent=1)
        f.write('\n')
    print('recorded the reference network job under the connection options')


if __name__ == '__main__':
    main()
