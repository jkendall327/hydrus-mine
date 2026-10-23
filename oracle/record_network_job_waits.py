#!/usr/bin/env python3
"""Record where the real network engine holds a job: its five positions.

On the `basic` fixture with new traffic unpaused, real `NetworkJob`s are added
to the client's own running `NetworkEngine` (`AddJob`) one scenario at a
time, and `GetJobsSnapshot` (what the current network jobs window lists) is
read once the job has settled:

- `headers`: a custom header on its domain awaits approval (the engine's
  validation popup slot is held by a stand-in, so no dialog opens);
- `paused`: all new network traffic paused;
- `bandwidth`: the global context's rules used up;
- `domain_slot`: a second job on a domain whose one slot is taken (by
  `running`, a request to a local server that never answers);
- `gallery_token`: a gallery job whose site's token was just consumed;
- `domain_errors`: a job on a domain with recent serious errors;
- `woke`: the computer just woke from sleep;
- `connection`: a job whose connection was refused, waiting to retry.

The checks for a settled job read only its position and status text
(calling `TokensOK` or `DomainOK` would put it to sleep where it is).
Recorded for each: the position (`job_status_str_lookup`) and the job's
status text. Each job is cancelled before the next scenario.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_network_job_waits.py
       (writes fixtures/network_job_waits.json)
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


def hold_server():
    """A local server that accepts and never answers."""
    listener = socket.socket()
    listener.bind(('127.0.0.1', 0))
    listener.listen(8)
    held = []

    def accept():
        while True:
            try:
                conn, _ = listener.accept()
            except OSError:
                return
            held.append(conn)

    threading.Thread(target=accept, daemon=True).start()
    return listener, held


def refused_port():
    s = socket.socket()
    s.bind(('127.0.0.1', 0))
    port = s.getsockname()[1]
    s.close()
    return port


def record(session):
    c = session.controller
    engine = c.network_engine
    from hydrus.core import HydrusConstants as HC
    from hydrus.core.networking import HydrusNetworking as HN
    from hydrus.client.networking import ClientNetworking as N, ClientNetworkingJobs as J
    from hydrus.client.networking import ClientNetworkingContexts as C, ClientNetworkingDomain as D

    if engine._pause_all_new_network_traffic:
        engine.PausePlayNewJobs()
    bw = engine.bandwidth_manager
    domain = engine.domain_manager
    old_global_rules = bw.GetRules(C.GLOBAL_NETWORK_CONTEXT)
    bw.SetRules(C.GLOBAL_NETWORK_CONTEXT, HN.BandwidthRules())

    listener, held = hold_server()
    hold_url = 'http://127.0.0.1:{}/held'.format(listener.getsockname()[1])

    def position(job):
        for (status, j) in engine.GetJobsSnapshot():
            if j is job:
                return status
        return None

    def settle(job, ok, timeout=20):
        end = time.time() + timeout
        while time.time() < end:
            if position(job) is not None and ok(job):
                break
            time.sleep(0.1)
        time.sleep(0.5)
        return position(job)

    def gone(job):
        job.Cancel()
        end = time.time() + 30
        while time.time() < end and position(job) is not None:
            time.sleep(0.1)

    out = {'positions': N.job_status_str_lookup, 'scenarios': {}}

    def note(name, job, pos):
        out['scenarios'][name] = {
            'url': job.GetURL(),
            'position': N.job_status_str_lookup[pos] if pos is not None else None,
            'status': job.GetStatus()[0],
        }

    def run(name, url, ok, setup=None, teardown=None, gallery_token=None):
        job = J.NetworkJob('GET', url)
        if gallery_token is not None:
            job._gallery_token_name = gallery_token
        if setup:
            setup()
        engine.AddJob(job)
        pos = settle(job, ok)
        note(name, job, pos)
        gone(job)
        if teardown:
            teardown()

    # headers: a pending custom header holds the job before validation
    class Waiting:
        def IsDone(self):
            return False

    old_headers = domain._network_contexts_to_custom_header_dicts
    def headers_on():
        domain._network_contexts_to_custom_header_dicts = {
            C.NetworkContext(CC_DOMAIN, 'headers.example.com'): {'X-Test': ('value', D.VALID_UNKNOWN, 'fixture')}
        }
        engine._current_validation_process = Waiting()
    def headers_off():
        domain._network_contexts_to_custom_header_dicts = old_headers
        engine._current_validation_process = None
    from hydrus.client import ClientConstants as CC
    CC_DOMAIN = CC.NETWORK_CONTEXT_DOMAIN
    run('headers', 'https://headers.example.com/file', lambda j: 'validation' in j.GetStatus()[0], headers_on, headers_off)

    # paused
    run('paused', 'https://paused.example.com/file', lambda j: 'paused' in j.GetStatus()[0],
        engine.PausePlayNewJobs, engine.PausePlayNewJobs)

    # bandwidth
    def bandwidth_on():
        rules = HN.BandwidthRules()
        rules.AddRule(HC.BANDWIDTH_TYPE_REQUESTS, 3600, 1)
        bw.SetRules(C.GLOBAL_NETWORK_CONTEXT, rules)
        bw.ReportRequestUsed([C.GLOBAL_NETWORK_CONTEXT])
    def bandwidth_off():
        bw.SetRules(C.GLOBAL_NETWORK_CONTEXT, HN.BandwidthRules())
    run('bandwidth', 'https://bandwidth.example.com/file', lambda j: position(j) == N.JOB_STATUS_AWAITING_BANDWIDTH,
        bandwidth_on, bandwidth_off)

    # running (held) and a second job on its domain waiting for its slot
    old_per_domain = engine.MAX_JOBS_PER_DOMAIN
    engine.MAX_JOBS_PER_DOMAIN = 1
    first = J.NetworkJob('GET', hold_url)
    engine.AddJob(first)
    note('running', first, settle(first, lambda j: position(j) == N.JOB_STATUS_RUNNING and len(held) > 0))
    run('domain_slot', hold_url + '2', lambda j: 'other jobs on this domain' in j.GetStatus()[0])
    gone(first)
    engine.MAX_JOBS_PER_DOMAIN = old_per_domain

    # gallery token
    bw.TryToConsumeAGalleryToken('example.org', 'download page')
    run('gallery_token', 'https://gallery.example.org/page', lambda j: 'waiting to start' in j.GetStatus()[0], gallery_token='download page')

    # domain errors
    def errors_on():
        for _ in range(c.new_options.GetInteger('domain_network_infrastructure_error_number')):
            domain.ReportDomainEvent('https://errors.example.net/file', 0)
    def errors_off():
        domain.ScrubDomainErrors('https://errors.example.net/file')
    run('domain_errors', 'https://errors.example.net/file', lambda j: 'serious errors' in j.GetStatus()[0], errors_on, errors_off)

    # just woke
    old_woke = c.JustWokeFromSleep
    def woke_on():
        c.JustWokeFromSleep = lambda: True
    def woke_off():
        c.JustWokeFromSleep = old_woke
    run('woke', 'https://woke.example.com/file', lambda j: 'woke' in j.GetStatus()[0], woke_on, woke_off)

    # connection refused, waiting to retry
    port = refused_port()
    run('connection', 'http://127.0.0.1:{}/refused'.format(port), lambda j: 'retrying' in j.GetStatus()[0])

    bw.SetRules(C.GLOBAL_NETWORK_CONTEXT, old_global_rules)
    listener.close()
    for conn in held:
        conn.close()
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
    with open(os.path.join(HERE, 'fixtures/network_job_waits.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote network_job_waits.json')


if __name__ == '__main__':
    main()
