#!/usr/bin/env python3
"""Record actual NetworkJobControl menus, gate actions, auto timing and error ownership.

No requests are sent. Real jobs/managers use synthetic example.com contexts;
Qt actions invoke the reference handlers, including accepted/cancelled rules.
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
        from hydrus.client.gui import ClientGUICore as CGC, ClientGUIDialogsMessage as M
        from hydrus.client.gui.networking import ClientGUINetworkJobControl as W
        from hydrus.client.networking import ClientNetworkingJobs as J, ClientNetworkingBandwidth as B
        from hydrus.client.networking import ClientNetworkingContexts as C, ClientNetworkingDomain as D
        now = [NOW]
        old_now, old_popup, old_pub, old_critical = HydrusTime.GetNow, CGC.core().PopupMenu, c.pub, M.ShowCritical
        old_bw, old_domain = c.network_engine.bandwidth_manager, c.network_engine.domain_manager
        HydrusTime.GetNow = lambda: now[0]
        bw, domain = B.NetworkBandwidthManager(), D.NetworkDomainManager()
        c.network_engine.bandwidth_manager, c.network_engine.domain_manager = bw, domain
        out = {'now': NOW, 'menus': {}, 'actions': {}, 'clipboard': [], 'messages': []}
        captured = []
        def rows(menu):
            return [{'label': a.text(), 'separator': a.isSeparator(), 'checked': a.isChecked() if a.isCheckable() else None,
                     'children': rows(a.menu()) if a.menu() else []} for a in menu.actions()]
        def popup(owner, menu):
            captured[:] = [menu]
        CGC.core().PopupMenu = popup
        c.pub = lambda *args, **kw: out['clipboard'].append(list(args)) if args and args[0] == 'clipboard' else None
        M.ShowCritical = lambda parent, title, text: out['messages'].append([title, text])
        widget = W.NetworkJobControl(c.gui)
        widget._ShowCogMenu()
        out['menus']['empty'] = rows(captured[0])
        job = J.NetworkJob('GET', 'https://example.com/file')
        job.engine = c.network_engine
        job._network_contexts.extend([C.NetworkContext(2, 'sub.example.com'), C.NetworkContext(4, b'fixture')])
        rules = HN.BandwidthRules()
        rules.AddRule(HC.BANDWIDTH_TYPE_REQUESTS, 60, 1)
        bw.SetRules(C.GLOBAL_NETWORK_CONTEXT, rules)
        bw.ReportRequestUsed([C.GLOBAL_NETWORK_CONTEXT])
        job._connection_error_wake_time = NOW + 60
        job._serverside_bandwidth_wake_time = NOW + 120
        job._gallery_token_name = 'download page'
        for _ in range(c.new_options.GetInteger('domain_network_infrastructure_error_number')):
            domain.ReportDomainEvent(job.GetURL(), 0)
        bw.TryToConsumeAGalleryToken('example.com', 'download page')
        widget.SetNetworkJob(job)
        widget._ShowCogMenu()
        out['menus']['waiting'] = rows(captured[0])
        def trigger(label):
            for action in captured[0].actions():
                if action.text() == label:
                    action.trigger()
                    return
            raise AssertionError(label)
        trigger('reattempt connection now')
        trigger('reattempt request now (server reports low bandwidth)')
        trigger('scrub domain errors')
        trigger('override forced gallery wait times for this job')
        out['actions']['retry'] = [job.CurrentlyWaitingOnConnectionError(), job.CurrentlyWaitingOnServersideBandwidth(), job.DomainOK(), job.TokensOK()]
        trigger('override bandwidth rules for this job')
        out['actions']['obeys_after_override'] = job.ObeysBandwidth()
        job2 = J.NetworkJob('GET', 'https://example.com/second')
        job2.engine = c.network_engine
        widget.SetNetworkJob(job2)
        widget.FlipAutoOverrideBandwidth()
        now[0] = NOW + 5
        widget._OverrideBandwidthIfAppropriate()
        out['actions']['auto_at_five'] = job2.ObeysBandwidth()
        now[0] = NOW + 6
        widget._OverrideBandwidthIfAppropriate()
        out['actions']['auto_after_five'] = job2.ObeysBandwidth()
        widget.SetError('synthetic failure\nserver detail')
        widget._ShowErrorMenu()
        out['menus']['error'] = rows(captured[0])
        trigger('show error')
        trigger('copy error')
        widget.ClearNetworkJob()
        out['actions']['clear_job_keeps_error'] = widget._error_text
        widget.ClearError()
        widget.CopyError()
        widget.ShowError()
        out['actions']['owner_clear_error'] = widget._error_text
        out['actions']['error_button_hidden'] = widget._error_button.isHidden()
        widget.deleteLater()
        HydrusTime.GetNow, CGC.core().PopupMenu, c.pub, M.ShowCritical = old_now, old_popup, old_pub, old_critical
        c.network_engine.bandwidth_manager, c.network_engine.domain_manager = old_bw, old_domain
        return out
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/network_job_control.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote network_job_control.json')

if __name__ == '__main__':
    main()
