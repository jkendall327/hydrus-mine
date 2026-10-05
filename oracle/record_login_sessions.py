#!/usr/bin/env python3
"""Record real Qt login-session rows, eligibility and immediate cookie reset."""
import json
import os
import sys
import tempfile
import record_string_converter_editor as recorder
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures/login_sessions.json')

def record(session):
    c = session.controller
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui import ClientGUIDialogsQuick as D
        from hydrus.client.gui.networking import ClientGUILogin as G
        from hydrus.client.networking import ClientNetworkingLogin as L, ClientNetworkingContexts as N, ClientNetworkingSessions as C
        from hydrus.core import HydrusTime as T
        now = 1_900_000_000
        fixed = lambda v: S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value=v, example_string=v)
        requirements = {fixed('session'): fixed('ok'), fixed('token'): fixed('ready')}
        script = L.LoginScriptDomain('session fixture', required_cookies_info=requirements)
        script.SetLoginScriptKey(bytes(range(32)))
        engine = c.network_engine
        domain = 'login.example'
        context = N.NetworkContext(N.CC.NETWORK_CONTEXT_DOMAIN, domain)
        other = N.NetworkContext(N.CC.NETWORK_CONTEXT_DOMAIN, 'other.example.net')
        info = (script.GetLoginScriptKeyAndName(), {}, 0, 'synthetic fixture', True, L.VALIDITY_UNTESTED, '', now + 3600, 'synthetic delay')
        original_now = T.GetNow
        T.GetNow = lambda: now
        questions = []
        answer = [False]
        old_yes = D.GetYesNo
        D.GetYesNo = lambda parent, message, **kwargs: (questions.append(message) or (QW.QDialog.DialogCode.Accepted if answer[0] else QW.QDialog.DialogCode.Rejected))
        panel = G.EditLoginsPanel(c.gui, engine, [script], {domain: info})
        panel._domains_and_login_info.SelectDatas(panel._domains_and_login_info.GetData(), deselect_others=True)
        def state():
            row = panel._domains_and_login_info.GetData()[0]
            return {'cells': list(panel._ConvertDomainAndLoginInfoToDisplayTuple(row)), 'can_login': panel._CanDoLogin(), 'can_scrub_delay': panel._CanScrubDelays(), 'can_scrub_invalidity': panel._CanScrubInvalidity(), 'logged_in': script.IsLoggedIn(engine, context), 'expiry': script.GetLoginExpiry(engine, context), 'cookies': [cookie.name for cookie in C.GetRequestsSessionCookieJar(engine.session_manager.GetSession(context))]}
        states = []
        try:
            for name, values in [('missing', []), ('persistent', [('session', 'ok', now + 7200), ('token', 'ready', now + 3600)]), ('session', [('session', 'ok', None), ('token', 'ready', now + 3600)]), ('invalid-value', [('session', 'wrong', None), ('token', 'ready', now + 3600)])]:
                engine.session_manager.ClearSession(context)
                for key, value, expiry in values:
                    C.AddCookieToSession(engine.session_manager.GetSession(context), key, value, domain, '/', expiry)
                states.append({'action': name, 'input': values, 'state': state()})
            engine.session_manager.ClearSession(context)
            C.AddCookieToSession(engine.session_manager.GetSession(context), 'session', 'ok', domain, '/', None)
            C.AddCookieToSession(engine.session_manager.GetSession(context), 'token', 'ready', domain, '/', None)
            C.AddCookieToSession(engine.session_manager.GetSession(other), 'keep', 'untouched', 'other.example.net', '/', None)
            for accepted in [False, True]:
                answer[0] = accepted
                questions.clear()
                panel._ClearSessions()
                states.append({'action': 'reset', 'accepted': accepted, 'questions': list(questions), 'state': state(), 'other_cookies': [cookie.name for cookie in C.GetRequestsSessionCookieJar(engine.session_manager.GetSession(other))]})
            return {'now': now, 'script': script.GetSerialisableTuple(), 'domain': domain, 'other_domain': 'other.example.net', 'resolved_session': engine.session_manager._GetSessionNetworkContext(context).context_data, 'other_resolved_session': engine.session_manager._GetSessionNetworkContext(other).context_data, 'info': [[info[0][0].hex(), info[0][1]], *info[1:]], 'states': states}
        finally:
            panel.deleteLater()
            T.GetNow = original_now
            D.GetYesNo = old_yes
    return c.CallBlockingToQt(c.gui, qt)

recorder.record = record
if __name__ == '__main__':
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path = os.path.join(work, 'sessions.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
            with open(path) as stream:
                result = json.load(stream)
        with open(OUT, 'w') as stream:
            json.dump(result, stream, indent=1, ensure_ascii=False)
            stream.write('\n')
        print(f'wrote {OUT}')
