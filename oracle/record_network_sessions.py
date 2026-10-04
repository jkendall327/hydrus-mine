#!/usr/bin/env python3
"""Record live Qt session/cookie rows, cookie/header validation and delete/clear questions.
Includes show-empty filtering and accepted cookie writes/delete/session-clear.
All conversions and edits are performed by actual reference panels on the Qt thread.
"""
import json
import os
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    def qt():
        from hydrus.client.gui.networking import ClientGUINetwork as G
        from hydrus.client.networking import ClientNetworkingContexts as C
        from hydrus.client.networking import ClientNetworkingSessions as S
        from hydrus.client.networking import ClientNetworkingDomain as D
        from hydrus.client import ClientConstants as CC
        from hydrus.core import HydrusTime
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from qtpy import QtWidgets as W
        HydrusTime.GetNow = lambda: 1700000000
        root = session.controller.gui
        manager = S.NetworkSessionManager()
        context = C.NetworkContext(CC.NETWORK_CONTEXT_DOMAIN, 'example.com')
        manager.GetSession(context)
        sess = manager.GetSession(context)
        S.AddCookieToSession(sess, 'session', 'abc', '.example.com', '/', None)
        S.AddCookieToSession(sess, 'token', '123', '.example.com', '/private', 4102444800)
        sessions = G.ReviewNetworkSessionsPanel(root, manager)
        cookies = G.ReviewNetworkSessionPanel(root, manager, context)
        out = {'session_row': sessions._ConvertNetworkContextToDisplayTuple(context),
               'cookie_rows': [cookies._ConvertCookieToDisplayTuple(c) for c in cookies._listctrl.GetData()],
               'cookie_cases': [], 'header_cases': [], 'questions': []}
        for fields in [(' name ', ' value ', ' .example.com ', ' / ', None), ('bad\nname', 'value', '.example.com', '/', None), ('name', 'bad\nvalue', '.example.com', '/', None)]:
            panel = G.EditCookiePanel(root, *fields)
            try: value, error = panel.GetValue(), None
            except Exception as e: value, error = None, str(e)
            out['cookie_cases'].append({'input': fields, 'value': value, 'error': error,
                                        'expiry': panel._expires_st.text(), 'timestamp': panel._expires_st_utc.text()})
            panel.deleteLater()
        for key, value, approved in [(' Authorization ', ' token ', D.VALID_APPROVED), ('X-Denied', 'no', D.VALID_DENIED), ('X-Pending', 'later', D.VALID_UNKNOWN), ('Bad\nKey', 'token', D.VALID_DENIED), ('X-Test', 'bad\nvalue', D.VALID_UNKNOWN)]:
            panel = G.EditNetworkContextCustomHeadersPanel._EditPanel(root, context, key, value, approved, 'login reason')
            try:
                c, k, v, a, r = panel.GetValue()
                result, error = [c.ToString(), k, v, D.valid_desc_lookup[a], r], None
            except Exception as e: result, error = None, str(e)
            out['header_cases'].append({'input': [key, value, approved], 'value': result, 'error': error})
            panel.deleteLater()
        headers = G.EditNetworkContextCustomHeadersPanel(root, {context: {'Authorization': ('token', D.VALID_APPROVED, 'login reason')}})
        out['header_row'] = headers._ConvertDataToDisplayTuple(headers._list_ctrl.GetData()[0])
        def no(*args, **kwargs):
            out['questions'].append(args[1] if len(args) > 1 else kwargs.get('message'))
            return W.QDialog.DialogCode.Rejected
        Q.GetYesNo = no
        cookies._listctrl.SelectDatas(cookies._listctrl.GetData())
        cookies._Delete()
        sessions._listctrl.SelectDatas([context])
        sessions._Clear()
        out['after_cancel'] = len(S.GetRequestsSessionCookieJar(sess))
        empty = C.NetworkContext(CC.NETWORK_CONTEXT_DOMAIN, 'empty.example.org')
        manager.GetSession(empty)
        sessions._Update()
        out['show_empty_counts'] = [len(sessions._listctrl.GetData())]
        sessions._show_empty.setChecked(True)
        sessions._Update()
        out['show_empty_counts'].append(len(sessions._listctrl.GetData()))
        # These are the real review panel's accepted-cookie write and deletion paths.
        cookies._SetCookie('renamed', 'edited', '.example.com', '/private', None)
        cookies._Update()
        out['after_renamed_write'] = [c.name for c in cookies._listctrl.GetData()]
        Q.GetYesNo = lambda *args, **kwargs: W.QDialog.DialogCode.Accepted
        selected = [c for c in cookies._listctrl.GetData() if c.name == 'renamed']
        cookies._listctrl.SelectDatas(selected, deselect_others=True)
        cookies._Delete()
        out['after_delete'] = [c.name for c in cookies._listctrl.GetData()]
        sessions._listctrl.SelectDatas([context])
        sessions._Clear()
        out['after_clear'] = sorted(c.ToString() for c in manager.GetNetworkContexts())
        for panel in [sessions, cookies, headers]: panel.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)

def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f: json.dump(result, f, ensure_ascii=False, indent=2)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import tempfile
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'network_sessions.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'network_sessions.json'), 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=2)
        f.write('\n')
    print('recorded', len(result), 'network panel states')

if __name__ == '__main__': main()
