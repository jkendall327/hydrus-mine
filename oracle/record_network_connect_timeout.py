#!/usr/bin/env python3
"""Record the reference's "network timeout (seconds)" on a connection that never opens.

A loopback listener with a backlog of nothing never accepts; once its queue
is full (filled here by plain sockets) the kernel drops new connection
requests, so a connect to it hangs, as to an address that never answers. On
the `basic` fixture, with new traffic unpaused and one connection attempt
allowed, a real `NetworkJob` is queued on the client's own engine against it,
with the option at 1 and then at 3 seconds. Recorded for each: how long the
job took, every status text it showed, and how it ended (its exception and its text).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_network_connect_timeout.py
       (writes fixtures/network_connect_timeout.json)
"""
import json
import os
import socket
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def full_listener():
    """A listener whose accept queue is full: the next connect hangs."""
    listener = socket.socket()
    listener.bind(('127.0.0.1', 0))
    listener.listen(0)
    port = listener.getsockname()[1]
    fillers = []
    while True:
        s = socket.socket()
        s.settimeout(0.5)
        try:
            s.connect(('127.0.0.1', port))
        except (socket.timeout, OSError):
            s.close()
            break
        fillers.append(s)
    return listener, fillers, port


def record(session):
    for key in [k for k in os.environ if k.lower().endswith('_proxy')]:
        del os.environ[key]
    c = session.controller
    from hydrus.client.networking import ClientNetworkingJobs as J
    options = c.new_options
    engine = c.network_engine
    if engine._pause_all_new_network_traffic:
        engine.PausePlayNewJobs()
    options.SetBoolean('pause_all_new_network_traffic', False)

    def settings(**kw):
        for key, value in kw.items():
            options.SetInteger(key, value)
        engine.RefreshOptions()
        engine.session_manager.ReinitialiseProxies()

    listener, fillers, port = full_listener()
    url = f'http://127.0.0.1:{port}/x'
    out = {'filled': len(fillers), 'runs': []}
    for timeout in (1, 3):
        settings(network_timeout=timeout, max_connection_attempts_allowed=1, max_request_attempts_allowed_get=1)
        job = J.NetworkJob('GET', url)
        job.OverrideBandwidth()
        statuses = []
        started = time.monotonic()
        engine.AddJob(job)
        done = threading.Event()

        def sample():
            while not done.is_set():
                s = job.GetStatus()[0]
                if not statuses or statuses[-1] != s:
                    statuses.append(s)
                time.sleep(0.02)

        threading.Thread(target=sample, daemon=True).start()
        while not job.IsDone() and time.monotonic() - started < 60:
            time.sleep(0.02)
        took = time.monotonic() - started
        done.set()
        error = str(job.GetErrorException()) if job.HasError() else None
        out['runs'].append({
            'timeout': timeout,
            'seconds': round(took, 2),
            'statuses': statuses,
            'error': error,
            'exception': type(job.GetErrorException()).__name__ if job.HasError() else None,
        })
    listener.close()
    for s in fillers:
        s.close()
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output_path = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record, network=True)
        with open(output_path, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/network_connect_timeout.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote network_connect_timeout.json')


if __name__ == '__main__':
    main()
