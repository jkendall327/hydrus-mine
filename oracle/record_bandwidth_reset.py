#!/usr/bin/env python3
"""Record the reference's "reset default bandwidth rules" button.

Runs the real `ReviewAllBandwidthPanel._ResetDefaultBandwidthRules` (its
question answered by script: no, then yes) on a real
`NetworkBandwidthManager` holding custom global and default rules, a rule on
two specific domains and rules on a default context the reset does not touch.
Records the question and every context's rules before and after each answer.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_bandwidth_reset.py
"""
import json, os, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, '..'))

from qtpy import QtWidgets as QW
from hydrus.core import HydrusConstants as HC
from hydrus.core.networking import HydrusNetworking
from hydrus.client import ClientConstants as CC
from hydrus.client.gui import ClientGUIDialogsQuick
from hydrus.client.gui.networking import ClientGUINetwork
from hydrus.client.networking import ClientNetworkingBandwidth, ClientNetworkingContexts


def dump(manager):
    out = []
    for (context, rules) in manager.GetDefaultRules() + [
        (c, r) for (c, r) in manager._network_contexts_to_bandwidth_rules.items() if not c.IsDefault()
    ]:
        data = context.context_data
        if isinstance(data, bytes):
            data = data.hex()
        out.append({'context': [context.context_type, data],
                    'rules': sorted([list(rule) for rule in rules.GetRules()])})
    return sorted(out, key=lambda row: json.dumps(row['context']))


def main():
    manager = ClientNetworkingBandwidth.NetworkBandwidthManager()

    def rules(*triples):
        r = HydrusNetworking.BandwidthRules()
        for t in triples:
            r.AddRule(*t)
        return r

    N = ClientNetworkingContexts.NetworkContext
    manager.SetRules(ClientNetworkingContexts.GLOBAL_NETWORK_CONTEXT, rules((HC.BANDWIDTH_TYPE_REQUESTS, 60, 1)))
    manager.SetRules(N(CC.NETWORK_CONTEXT_DOMAIN), rules((HC.BANDWIDTH_TYPE_DATA, 3600, 100)))
    manager.SetRules(N(CC.NETWORK_CONTEXT_WATCHER_PAGE), rules((HC.BANDWIDTH_TYPE_REQUESTS, 600, 3)))
    manager.SetRules(N(CC.NETWORK_CONTEXT_DOWNLOADER), rules((HC.BANDWIDTH_TYPE_REQUESTS, 600, 7)))
    manager.SetRules(N(CC.NETWORK_CONTEXT_DOMAIN, 'a.example.com'), rules((HC.BANDWIDTH_TYPE_REQUESTS, 60, 5)))
    manager.SetRules(N(CC.NETWORK_CONTEXT_DOMAIN, 'b.example.com'), rules((HC.BANDWIDTH_TYPE_DATA, 86400, 123)))

    controller = types.SimpleNamespace(network_engine=types.SimpleNamespace(bandwidth_manager=manager))
    panel = types.SimpleNamespace(_controller=controller)
    asked = []
    out = {'before': dump(manager), 'cases': []}
    for answer in ('no', 'yes'):
        def scripted(win, message, **kwargs):
            asked.append({'message': message, 'kwargs': kwargs})
            return QW.QDialog.DialogCode.Accepted if answer == 'yes' else QW.QDialog.DialogCode.Rejected
        real = ClientGUIDialogsQuick.GetYesNo
        ClientGUIDialogsQuick.GetYesNo = scripted
        try:
            ClientGUINetwork.ReviewAllBandwidthPanel._ResetDefaultBandwidthRules(panel)
        finally:
            ClientGUIDialogsQuick.GetYesNo = real
        out['cases'].append({'answer': answer, 'questions': list(asked), 'after': dump(manager)})
        asked.clear()
    with open(os.path.join(HERE, 'fixtures', 'bandwidth_reset.json'), 'w') as f:
        json.dump(out, f, indent=1)
        f.write('\n')
    print(json.dumps(out, indent=1)[:3000])


if __name__ == '__main__':
    main()
