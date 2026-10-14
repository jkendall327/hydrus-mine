#!/usr/bin/env python3
"""Record the real review-network-jobs panel's right-click menu and refresh control.

No requests are sent. A synthetic job on example.com is put in the panel through
the engine's own snapshot call; the menu is the panel's `_GetListCtrlMenu`.
The refresh control is the panel's own `NoneableTimeDeltaWidget` and timer.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
NOW = 1770000000


def record(session):
    c = session.controller

    def work():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusTime, HydrusConstants as HC
        from hydrus.core.networking import HydrusNetworking as HN
        from hydrus.client.gui.networking import ClientGUINetwork as G
        from hydrus.client.networking import ClientNetworkingBandwidth as B, ClientNetworkingContexts as C, ClientNetworkingJobs as J, ClientNetworkingDomain as D

        old_now = HydrusTime.GetNow
        HydrusTime.GetNow = lambda: NOW
        old_bw, old_domain = c.network_engine.bandwidth_manager, c.network_engine.domain_manager
        bw, domain = B.NetworkBandwidthManager(), D.NetworkDomainManager()
        c.network_engine.bandwidth_manager, c.network_engine.domain_manager = bw, domain

        def rows(menu):
            return [{'label': a.text(), 'children': rows(a.menu()) if a.menu() else []} for a in menu.actions() if not a.isSeparator()]

        out = {'now': NOW, 'menus': {}}
        jobs = G.ReviewNetworkJobs(c.gui, c)
        out['hint'] = [w.text() for w in jobs.findChildren(QW.QLabel) if 'right-click' in w.text()]
        out['buttons'] = [b.text() for b in jobs.findChildren(QW.QPushButton)]
        timer = jobs._refresh_timer
        widget = jobs._refresh_time_delta
        out['checkbox'] = widget._checkbox.text()
        out['refresh'] = {'initial_value': widget.GetValue(), 'initial_active': timer.isActive()}
        steps = []
        for value in [0.25, 2.0, 0.01, None, 1.5]:
            widget.SetValue(value)
            jobs._RefreshTimeDeltaChanged()
            steps.append({'set': value, 'value': widget.GetValue(), 'active': timer.isActive(), 'interval_ms': timer.interval() if timer.isActive() else None})
        out['refresh']['steps'] = steps

        rules = HN.BandwidthRules()
        rules.AddRule(HC.BANDWIDTH_TYPE_REQUESTS, 60, 1)
        bw.SetRules(C.GLOBAL_NETWORK_CONTEXT, rules)
        bw.ReportRequestUsed([C.GLOBAL_NETWORK_CONTEXT])

        def make(name, setup=None):
            job = J.NetworkJob('GET', 'https://example.com/file')
            job.engine = c.network_engine
            job._network_contexts.extend([C.NetworkContext(2, 'sub.example.com'), C.NetworkContext(4, b'fixture')])
            if setup:
                setup(job)
            jobs._list_ctrl.GetData = lambda only_selected=False, job=job: [(2, job)]
            out['menus'][name] = {'url': job.GetURL(), 'contexts': [n.ToString() for n in job.GetNetworkContexts()], 'rows': rows(jobs._GetListCtrlMenu())}

        def waiting(job):
            job._connection_error_wake_time = NOW + 60
            job._serverside_bandwidth_wake_time = NOW + 120

        def post(job):
            job._method = 'POST'

        def overridden(job):
            job._bandwidth_manual_override = True

        def token(job):
            job._gallery_token_name = 'download page'

        make('plain')
        make('waiting', waiting)
        make('post', post)
        make('overridden', overridden)
        make('gallery_token_needed', token)
        make('gallery_token_blocked', token)
        for _ in range(c.new_options.GetInteger('domain_network_infrastructure_error_number')):
            domain.ReportDomainEvent('https://example.com/file', 0)
        make('domain_errors')
        out['no_selection'] = None
        jobs._list_ctrl.GetData = lambda only_selected=False: []
        try:
            jobs._GetListCtrlMenu()
        except Exception as e:
            out['no_selection'] = type(e).__name__
        jobs.deleteLater()
        c.network_engine.bandwidth_manager, c.network_engine.domain_manager = old_bw, old_domain
        HydrusTime.GetNow = old_now
        return out

    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output_path = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output_path, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/network_jobs_panel.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote network_jobs_panel.json')


if __name__ == '__main__':
    main()
