#!/usr/bin/env python3
"""Record actual Qt bandwidth review/rule widgets and current-job rows at fixed time.

The running reference uses an isolated bandwidth manager. No request is sent.
Includes inherited/custom rules, data/request/month rules and revert prompts.
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
        from hydrus.client.networking import ClientNetworkingBandwidth as B, ClientNetworkingContexts as C, ClientNetworkingJobs as J, ClientNetworking as N
        from hydrus.client.gui.networking import ClientGUINetwork as G
        from hydrus.client.gui.widgets import ClientGUIBandwidth as W
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.lists import ClientGUIListConstants as L

        old_now = HydrusTime.GetNow
        HydrusTime.GetNow = lambda: NOW
        old_manager = c.network_engine.bandwidth_manager
        manager = B.NetworkBandwidthManager()
        c.network_engine.bandwidth_manager = manager
        contexts = [C.GLOBAL_NETWORK_CONTEXT, C.NetworkContext(2, 'example.com'), C.NetworkContext(2, 'empty.example')]
        rules = HN.BandwidthRules()
        values = [(HC.BANDWIDTH_TYPE_DATA, None, 104857600), (HC.BANDWIDTH_TYPE_DATA, 86400, 4096), (HC.BANDWIDTH_TYPE_REQUESTS, 60, 20)]
        for rule in values:
            rules.AddRule(*rule)
        manager.SetRules(contexts[0], rules)
        manager.SetRules(contexts[1], rules)
        manager.ReportDataUsed([contexts[1]], 2048)
        manager.ReportRequestUsed([contexts[1]])
        panel = G.ReviewAllBandwidthPanel(c.gui, c)
        detail = G.ReviewNetworkContextBandwidthPanel(c.gui, c, contexts[1])
        detail._Update()
        detail._UpdateRules()
        widget = W.BandwidthRulesCtrl(c.gui, rules)
        jobs = G.ReviewNetworkJobs(c.gui, c)
        job = J.NetworkJob('GET', 'https://example.com/file')
        job._status_text = 'downloading' + HC.UNICODE_ELLIPSIS
        job._num_bytes_read = 2048
        job._num_bytes_to_read = 4096
        job._bandwidth_tracker.ReportDataUsed(2048)
        out = {
            'now': NOW,
            'contexts': [[n.context_type, n.context_data] for n in contexts],
            'usage_rows': [panel._ConvertNetworkContextsToDisplayTuple(n) for n in contexts],
            'rule_values': values,
            'rule_rows': [widget._ConvertRuleToDisplayTuple(r) for r in values],
            'detail': {'current': detail._current_usage_st.text(), 'all_time': detail._all_time_usage.text(), 'rules': detail._uses_default_rules_st.text(), 'edit': detail._edit_rules_button.text()},
            'job_columns': list(L.column_list_column_name_lookup[L.COLUMN_LIST_NETWORK_JOBS_REVIEW.ID].values()),
            'job_rows': [jobs._ConvertDataToDisplayTuple((p, job)) for p in sorted(N.job_status_str_lookup)],
            'job_phases': N.job_status_str_lookup,
            'questions': [],
        }
        old_question = Q.GetYesNo
        Q.GetYesNo = lambda parent, text, **kw: (out['questions'].append(text), QW.QDialog.DialogCode.Rejected)[1]
        detail._UseDefaultRules()
        panel._ResetDefaultBandwidthRules()
        Q.GetYesNo = old_question
        for w in [panel, detail, widget, jobs]:
            w.deleteLater()
        c.network_engine.bandwidth_manager = old_manager
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
    with open(os.path.join(HERE, 'fixtures/network_data.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote network_data.json')


if __name__ == '__main__':
    main()
