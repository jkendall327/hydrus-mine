#!/usr/bin/env python3
"""Record real bandwidth review age/rules filters, monthly tracker data and history deletion.
Uses an isolated manager and real Qt controls; no requests are sent.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
NOW = 1770000000

def record(session):
    def qt():
        from qtpy import QtWidgets as W
        from hydrus.core import HydrusTime
        from hydrus.core.networking import HydrusNetworking as HN
        from hydrus.client.networking import ClientNetworkingBandwidth as B, ClientNetworkingContexts as C
        from hydrus.client.gui.networking import ClientGUINetwork as G
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        controller = session.controller
        old_manager = controller.network_engine.bandwidth_manager
        old_now = HydrusTime.GetNow
        manager = B.NetworkBandwidthManager()
        controller.network_engine.bandwidth_manager = manager
        contexts = {name: C.NetworkContext(2, name + '.example.com') for name in ['recent', 'old', 'rules', 'bytes']}
        contexts['page'] = C.NetworkContext(4, b'\x91' * 16)
        events = [(1735689600, 'recent', 1024, 1), (1767225600, 'recent', 2048, 1),
                  (NOW - 10 * 86400, 'old', 512, 1), (NOW, 'recent', 4096, 1),
                  (NOW, 'bytes', 123, 0), (NOW, 'page', 55, 1)]
        for at, name, data, requests in events:
            HydrusTime.GetNow = lambda at=at: at
            manager.ReportDataUsed([contexts[name]], data)
            for _ in range(requests): manager.ReportRequestUsed([contexts[name]])
        HydrusTime.GetNow = lambda: NOW
        rules = HN.BandwidthRules()
        rules.AddRule(0, 86400, 100000)
        manager.SetRules(contexts['rules'], rules)
        panel = G.ReviewAllBandwidthPanel(controller.gui, controller)
        out = {'now': NOW, 'events': events, 'filters': [], 'questions': []}
        for span, include_rules in [(86400, False), (86400, True), (86400 * 14, False), (None, False)]:
            panel._history_time_delta_none.setChecked(span is None)
            if span is not None: panel._history_time_delta_threshold.SetValue(span)
            panel._show_those_with_bandwidth_rules.setChecked(include_rules)
            panel._Update()
            out['filters'].append({'span': span, 'include_rules': include_rules,
                'contexts': sorted(c.ToString() for c in panel._bandwidths.GetData())})
        out['months'] = manager.GetTracker(contexts['recent']).GetMonthlyDataUsage()
        out['rules_before'] = list(manager.GetRules(contexts['rules']).GetRules())
        panel._bandwidths.SelectDatas([contexts['recent'], contexts['rules']], deselect_others=True)
        Q.GetYesNo = lambda parent, text, **kw: (out['questions'].append(text), W.QDialog.DialogCode.Rejected)[1]
        panel._DeleteNetworkContexts()
        out['months_after_cancel'] = manager.GetTracker(contexts['recent']).GetMonthlyDataUsage()
        Q.GetYesNo = lambda *a, **kw: W.QDialog.DialogCode.Accepted
        panel._DeleteNetworkContexts()
        panel._Update()
        out['months_after_delete'] = manager.GetTracker(contexts['recent']).GetMonthlyDataUsage()
        out['rules_after_delete'] = list(manager.GetRules(contexts['rules']).GetRules())
        out['contexts_after_delete'] = sorted(c.ToString() for c in panel._bandwidths.GetData())
        panel.deleteLater()
        controller.network_engine.bandwidth_manager = old_manager
        HydrusTime.GetNow = old_now
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output_path = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output_path, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'bandwidth_history.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'bandwidth_history.json'), 'w') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print('recorded bandwidth filters, chart totals and delete-history')

if __name__ == '__main__': main()
