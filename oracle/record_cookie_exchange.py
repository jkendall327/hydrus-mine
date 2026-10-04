#!/usr/bin/env python3
"""Record actual Qt cookie clipboard export/import, mismatch choices and Netscape import.
Uses synthetic domains and the reference panels' real handlers and session jars.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

NETSCAPE = """# Netscape HTTP Cookie File
#HttpOnly_.example.com\tTRUE\t/private\tTRUE\t4102444800\tsid\tsecret
example.com\tFALSE\t/\tFALSE\t0\tsession\tvalue
.example.com\tTRUE\t/\tFALSE\t1\texpired\told
example.com\tFALSE\t/\tFALSE\t\t\tvalueless
"""

def record(session):
    def qt():
        from hydrus.client.gui.networking import ClientGUINetwork as G
        from hydrus.client.networking import ClientNetworkingContexts as C
        from hydrus.client.networking import ClientNetworkingSessions as S
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui import ClientGUIDialogsMessage as M
        from qtpy import QtWidgets as W
        root = session.controller.gui
        manager = S.NetworkSessionManager()
        context = C.NetworkContext(CC.NETWORK_CONTEXT_DOMAIN, 'example.com')
        panel = G.ReviewNetworkSessionPanel(root, manager, context)
        browser = G.ReviewNetworkSessionsPanel(root, manager)
        out = {'netscape_text': NETSCAPE, 'questions': [], 'messages': [], 'exports': []}
        old_pub = session.controller.pub
        def pub(*args, **kwargs):
            if args[:2] == ('clipboard', 'text'):
                out['exports'].append(json.loads(args[2]))
            else:
                old_pub(*args, **kwargs)
        session.controller.pub = pub
        M.ShowInformation = lambda parent, text, *a, **kw: out['messages'].append(text)
        M.ShowCritical = lambda parent, title, text, *a, **kw: out['messages'].append([title, text])
        answers = []
        def question(*args, **kw):
            out['questions'].append({'text': args[1] if len(args) > 1 else kw['message'],
                                     'yes': kw.get('yes_label'), 'no': kw.get('no_label')})
            result = answers.pop(0) if answers else True
            code = W.QDialog.DialogCode.Accepted if result else W.QDialog.DialogCode.Rejected
            return (code, False) if kw.get('check_for_cancelled') else code
        Q.GetYesNo = question
        def cookies():
            return sorted([{'name': c.name, 'value': c.value, 'domain': c.domain, 'path': c.path,
                            'expires': c.expires, 'secure': c.secure, 'rest': sorted(c._rest.items())}
                           for c in S.GetRequestsSessionCookieJar(manager.GetSession(context))], key=lambda c: (c['name'], c['domain'], c['path']))
        with tempfile.TemporaryDirectory() as tmp:
            path = os.path.join(tmp, 'cookies.txt')
            with open(path, 'w') as f: f.write(NETSCAPE)
            panel._ImportCookiesTXTPaths([path])
        out['netscape_cookies'] = cookies()
        panel._listctrl.SelectDatas(panel._listctrl.GetData())
        panel._ExportToClipboard()
        raw = json.dumps([['sid', 'replacement', '.example.com', '/private', 0],
                          ['other', 'different', '.other.example', '/', None]])
        out['clipboard_text'] = raw
        session.controller.GetClipboardText = lambda: raw
        panel._ImportFromClipboard()
        out['after_matching_import'] = cookies()
        answers.extend([False, True])
        panel._ImportFromClipboard()
        out['after_all_import'] = cookies()
        session.controller.GetClipboardText = lambda: '[]'
        panel._ImportFromClipboard()
        session.controller.GetClipboardText = lambda: 'not json'
        panel._ImportFromClipboard()
        raw = json.dumps([['browser', 'v', '.sub.example.com', '/', 0],
                          ['elsewhere', None, 'other.example', '/', None]])
        session.controller.GetClipboardText = lambda: raw
        browser._ImportFromClipboard()
        out['browser_contexts'] = sorted(c.ToString() for c in manager.GetNetworkContexts())
        browser._listctrl.SelectDatas(browser._listctrl.GetData())
        browser._ExportToClipboard()
        panel.deleteLater()
        browser.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)

def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f: json.dump(result, f, indent=2)

def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'cookie_exchange.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'cookie_exchange.json'), 'w') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print('recorded cookie clipboard and Netscape workflows')

if __name__ == '__main__': main()
