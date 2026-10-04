#!/usr/bin/env python3
"""Record real Qt credential definitions and credential entry validation.

Also record typed login script/step serialization, version upgrades and script
validation against synthetic domains and harmless dummy credentials only.
"""
import json
import os
import sys
import tempfile
import record_string_converter_editor as recorder
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures/login_editors.json')

def record(session):
    controller = session.controller
    gui = controller.gui
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientStrings as S
        from hydrus.client.networking import ClientNetworkingLogin as L
        from hydrus.client.gui.networking import ClientGUILogin as G
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable
        user_match = S.StringMatch(match_type=S.STRING_MATCH_REGEX, match_value=r'^[a-z]+$', min_chars=3, example_string='alice')
        user = L.LoginCredentialDefinition('username', L.CREDENTIAL_TYPE_TEXT, user_match)
        password = L.LoginCredentialDefinition('password', L.CREDENTIAL_TYPE_PASS, S.StringMatch(min_chars=4))
        definition = G.EditLoginCredentialDefinitionPanel(gui, user)
        before = definition.GetValue().GetSerialisableTuple()
        definition._name.setText('account')
        definition._credential_type.SetValue(L.CREDENTIAL_TYPE_PASS)
        definition._string_match.SetValue(S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='token', example_string='token'))
        after = definition.GetValue().GetSerialisableTuple()
        definition.deleteLater()
        panel = G.EditLoginCredentialsPanel(gui, [password, user], {'username': 'a', 'password': ''})
        questions = []
        answer = [False]
        def yes_no(parent, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted if answer[0] else QW.QDialog.DialogCode.Rejected
        ClientGUIDialogsQuick.GetYesNo = yes_no
        def state():
            return [{'name': cd.GetName(), 'value': edit.text(), 'hidden': edit.echoMode() == QW.QLineEdit.EchoMode.Password,
                     'label': label.text(), 'valid': label.property('hydrus_text') == 'valid'} for cd, edit, label in panel._control_data]
        states = [{'state': state()}]
        for values, allow in [({'username': 'alice', 'password': 'dummy-pass'}, False), ({'username': '1?!', 'password': 'x'}, False), ({'username': '', 'password': 'x'}, True)]:
            for cd, edit, label in panel._control_data: edit.setText(values[cd.GetName()])
            answer[0] = allow; questions.clear()
            accepted = panel.UserIsOKToOK()
            states.append({'do': values, 'answer': allow, 'accepted': accepted, 'questions': list(questions), 'state': state(), 'value': panel.GetValue()})
        panel.deleteLater()
        cookie = {S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='session', example_string='session'): S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='ok', example_string='ok')}
        variable = P.ContentParser(name='csrf variable', content_type=HC.CONTENT_TYPE_VARIABLE, formula=P.ParseFormulaStatic(static_text='dummy-csrf'), additional_info='csrf')
        first = L.LoginStep('establish session', 'http', 'GET', 'WWW-API7.', 'start')
        first.SetComplicatedVariables({}, {'lang': 'en'}, {}, {}, [variable])
        second = L.LoginStep('send credentials', 'http', 'POST', None, '/login')
        second.SetComplicatedVariables({'username': 'user', 'password': 'pass'}, {'mode': 'login'}, {'csrf': 'token'}, cookie, [])
        script = L.LoginScriptDomain('synthetic login', required_cookies_info=cookie, credential_definitions=[user, password], login_steps=[first, second], example_domains_info=[('login.example', 0, 'Login required to access any content.')])
        script.SetLoginScriptKey(bytes(range(32)))
        checks = []
        for given in [{}, {'username': 'a', 'password': 'dummy-pass'}, {'username': 'alice', 'password': 'dummy-pass', 'ignored': 'value'}]:
            try: script.CheckCanLogin(given); error = None
            except Exception as e: error = str(e)
            checks.append({'given': given, 'error': error})
        bad = script.Duplicate(); bad._credential_definitions.clear()
        try: bad.CheckIsValid(); missing_definitions = None
        except Exception as e: missing_definitions = str(e)
        bad = script.Duplicate(); bad._login_steps.reverse()
        try: bad.CheckIsValid(); missing_variables = None
        except Exception as e: missing_variables = str(e)
        old = list(script.GetSerialisableTuple()); old[2] = 1
        old_info = list(old[3]); old_info[1] = HydrusSerialisable.SerialisableDictionary({'session': S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='ok', example_string='ok')}).GetSerialisableTuple(); old[3] = old_info
        upgraded = HydrusSerialisable.CreateFromSerialisableTuple(old).GetSerialisableTuple()
        bundle = HydrusSerialisable.SerialisableList([script, script.Duplicate()]).GetSerialisableTuple()
        manager = L.NetworkLoginManager()
        manager._login_scripts = HydrusSerialisable.SerialisableList([script])
        manager._domains_to_login_info = {'login.example': ((script.GetLoginScriptKey(), script.GetName()), {'username': 'alice', 'password': 'dummy-pass'}, 0, 'Login required to access any content.', True, 1, '', 0, '')}
        original_manager = manager.GetSerialisableTuple()
        script_panel = G.EditLoginScriptPanel(gui, script)
        def table(control):
            model = control.model()
            return [[model.data(model.index(row, col)) for col in range(model.columnCount())] for row in range(model.rowCount())]
        script_rows = {'credentials': table(script_panel._credential_definitions), 'examples': table(script_panel._example_domains_info)}
        script_panel.deleteLater()
        scripts_panel = G.EditLoginScriptsPanel(gui, [script])
        def script_list_state():
            return {'rows': table(scripts_panel._login_scripts), 'keys': [item.GetLoginScriptKey().hex() for item in scripts_panel.GetValue()], 'names': [item.GetName() for item in scripts_panel.GetValue()]}
        script_list = [{'state': script_list_state()}]
        from hydrus.core import HydrusData
        HydrusData.GenerateKey = lambda: bytes([119]) * 32
        scripts_panel._AddLoginScript(script.Duplicate())
        script_list.append({'do': 'import duplicate', 'state': script_list_state()})
        scripts_panel._login_scripts.SelectDatas([scripts_panel.GetValue()[-1]], deselect_others=True)
        class ScriptDialog(QW.QWidget):
            def __init__(self, *args, **kwargs): super().__init__(gui)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, panel): self.panel = panel
            def exec(self): self.panel._name.setText('renamed script'); return QW.QDialog.DialogCode.Accepted
        G.ClientGUITopLevelWindowsPanels.DialogEdit = ScriptDialog
        scripts_panel._Edit()
        script_list.append({'do': 'rename duplicate', 'state': script_list_state()})
        scripts_panel.deleteLater()

        from hydrus.client.gui.parsing import ClientGUIParsing as PG
        step_panel = G.EditLoginStepPanel(gui, first)
        content_list = step_panel._content_parsers
        def step_state():
            return {'value': step_panel.GetValue().GetSerialisableTuple(), 'content_rows': table(content_list._content_parsers)}
        step_states = [{'state': step_state()}]
        content_action = ['renamed response', HC.CONTENT_TYPE_VARIABLE, 'token', True]
        permitted = []
        class ContentDialog(QW.QWidget):
            def __init__(self, *args, **kwargs): super().__init__(gui)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, panel): self.panel = panel
            def exec(self):
                permitted.append([self.panel._content_type.itemText(i) for i in range(self.panel._content_type.count())])
                self.panel._name.setText(content_action[0])
                self.panel._content_type.SetValue(content_action[1])
                if content_action[1] == HC.CONTENT_TYPE_VARIABLE: self.panel._temp_variable_name.setText(content_action[2])
                return QW.QDialog.DialogCode.Accepted if content_action[3] else QW.QDialog.DialogCode.Rejected
        PG.ClientGUITopLevelWindowsPanels.DialogEdit = ContentDialog
        content_list._content_parsers.SelectDatas([content_list.GetData()[0]])
        content_list._Edit()
        step_states.append({'do': 'edit variable', 'state': step_state()})
        content_list._AddContentParser(content_list.GetData()[0].Duplicate())
        step_states.append({'do': 'import duplicate', 'state': step_state()})
        content_action[:] = ['cancelled parser', HC.CONTENT_TYPE_VARIABLE, 'cancelled', False]
        content_list._Add()
        step_states.append({'do': 'cancel add', 'state': step_state()})
        content_action[:] = ['abort response', HC.CONTENT_TYPE_VETO, '', True]
        content_list._Add()
        step_states.append({'do': 'add veto', 'state': step_state()})
        step_panel._name.setText('edited request')
        step_panel._scheme.SetValue('https'); step_panel._method.SetValue('POST')
        step_panel._subdomain.SetValue(''); step_panel._path.setText('signin')
        step_states.append({'do': 'request fields', 'state': step_state()})
        step_panel.deleteLater()
        arguments_panel = G.EditLoginStepPanel(gui, first)
        def argument_rows():
            return {'credential': table(arguments_panel._required_credentials._listctrl), 'static': table(arguments_panel._static_args._listctrl), 'temporary': table(arguments_panel._temp_args._listctrl)}
        argument_states = [{'state': arguments_panel.GetValue().GetSerialisableTuple(), 'rows': argument_rows()}]
        prompts = []
        input_answers = []
        def enter(parent, message, **kwargs):
            prompts.append({'message': message, 'options': kwargs})
            value = input_answers.pop(0)
            if value is None: raise G.HydrusExceptions.CancelledException('scripted cancel')
            return value
        ClientGUIDialogsQuick.EnterText = enter
        G.ClientGUIDialogsMessage.ShowWarning = lambda parent, text: prompts.append({'warning': text})
        for kind, action, values in [('credential', 'add', ['username', 'user']), ('credential', 'add', ['username']), ('credential', 'edit', ['account', 'account_param']), ('static', 'add', ['empty', '']), ('temporary', 'add', ['cancelled', None]), ('temporary', 'add', ['csrf', 'token'])]:
            control = {'credential': arguments_panel._required_credentials, 'static': arguments_panel._static_args, 'temporary': arguments_panel._temp_args}[kind]
            if action == 'edit': control._listctrl.SelectDatas(control._listctrl.GetData(), deselect_others=True)
            input_answers[:] = values; prompts.clear()
            if action == 'add': control._Add()
            else: control._Edit()
            argument_states.append({'kind': kind, 'action': action, 'answers': values, 'prompts': list(prompts), 'state': arguments_panel.GetValue().GetSerialisableTuple(), 'rows': argument_rows()})
        arguments_panel.deleteLater()

        cookie_panel = G.EditLoginScriptPanel(gui, script)
        cookie_control = cookie_panel._required_cookies_info
        cookie_states = []
        cookie_answers = []
        cookie_dialogs = []
        class CookieDialog(QW.QWidget):
            def __init__(self, parent, title, *args, **kwargs):
                super().__init__(gui); cookie_dialogs.append(title)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, panel): self.panel = panel
            def exec(self):
                text = cookie_answers.pop(0)
                if text is None: return QW.QDialog.DialogCode.Rejected
                self.panel._match_type.SetValue(S.STRING_MATCH_FIXED)
                self.panel._match_value_fixed_input.setText(text)
                self.panel.GetValue()
                return QW.QDialog.DialogCode.Accepted
        G.ClientGUITopLevelWindowsPanels.DialogEdit = CookieDialog
        def cookie_state():
            value = HydrusSerialisable.SerialisableDictionary(cookie_control.GetValue()).GetSerialisableTuple()
            return {'value': value, 'rows': table(cookie_control._listctrl)}
        cookie_states.append({'state': cookie_state()})
        for action, values in [('add', ['token', 'ready']), ('edit', ['edited', 'changed']), ('add', ['discarded', None]), ('add', ['session', 'ok'])]:
            if action == 'edit': cookie_control._listctrl.SelectDatas([cookie_control._listctrl.GetData()[-1]], deselect_others=True)
            cookie_answers[:] = values; cookie_dialogs.clear()
            if action == 'add': cookie_control._Add()
            else: cookie_control._Edit()
            cookie_states.append({'action': action, 'answers': values, 'dialogs': list(cookie_dialogs), 'state': cookie_state()})
        cookie_panel.deleteLater()

        domains_panel = G.EditLoginsPanel(gui, controller.network_engine, [script], manager._domains_to_login_info)
        domains_control = domains_panel._domains_and_login_info
        def domain_state():
            manager._domains_to_login_info = domains_panel.GetValue()
            return {'value': manager.GetSerialisableTuple()[2][1], 'rows': table(domains_control)}
        domain_states = [{'state': domain_state()}]
        domain_action = [{'username': '1?!', 'password': 'x'}, True, True]
        class CredentialsDialog(QW.QWidget):
            def __init__(self, *args, **kwargs): super().__init__(gui)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, panel): self.panel = panel
            def exec(self):
                for cd, edit, label in self.panel._control_data: edit.setText(domain_action[0][cd.GetName()])
                if not domain_action[2]: return QW.QDialog.DialogCode.Rejected
                return QW.QDialog.DialogCode.Accepted if self.panel.UserIsOKToOK() else QW.QDialog.DialogCode.Rejected
        G.ClientGUITopLevelWindowsPanels.DialogEdit = CredentialsDialog
        for values, allow, accept in [({'username': '1?!', 'password': 'x'}, True, True), ({'username': 'alice', 'password': 'dummy-pass'}, True, True), ({'username': 'cancelled', 'password': 'cancelled-pass'}, True, False), ({'username': '', 'password': ''}, True, True)]:
            domain_action[:] = [values, allow, accept]; answer[0] = allow; questions.clear()
            domains_control.SelectDatas([domains_control.GetData()[0]], deselect_others=True)
            domains_panel._EditCredentials()
            domain_states.append({'do': values, 'accepted': accept, 'questions': list(questions), 'state': domain_state()})
        domains_panel.deleteLater()
        login_actions = []
        warnings = []
        G.ClientGUIDialogsMessage.ShowWarning = lambda parent, text: warnings.append(text)
        pristine = HydrusSerialisable.CreateFromSerialisableTuple(original_manager)._domains_to_login_info
        original_info = next(iter(pristine.values()))
        for mode, allow in [('eligible', False), ('eligible', True), ('ineligible', True)]:
            info = list(original_info)
            if mode == 'ineligible': info[4] = False
            route = G.EditLoginsPanel(gui, controller.network_engine, [script], {'login.example': tuple(info)})
            route._domains_and_login_info.SelectDatas(route._domains_and_login_info.GetData(), deselect_others=True)
            okayed = []
            route._OKParent = lambda: okayed.append(True)
            questions.clear(); warnings.clear(); answer[0] = allow
            route._DoLogin()
            login_actions.append({'mode': mode, 'accepted': allow, 'questions': list(questions), 'warnings': list(warnings), 'okayed': list(okayed), 'domains': route._domains_to_login_after_ok})
            route.deleteLater()

        return {'cookie_states': cookie_states, 'argument_states': argument_states, 'domain_login_actions': login_actions, 'domain_states': domain_states, 'step_states': step_states, 'permitted_content_types': permitted, 'manager': original_manager, 'script_rows': script_rows, 'script_list': script_list, 'definition': {'before': before, 'after': after}, 'credentials': states, 'script': script.GetSerialisableTuple(), 'legacy_script': old, 'upgraded_script': upgraded, 'bundle': bundle, 'checks': checks, 'missing_definitions': missing_definitions, 'missing_variables': missing_variables,
                'credential_types': [[i, L.credential_type_str_lookup[i]] for i in [0, 1]], 'access_types': [[i, L.login_access_type_str_lookup[i], L.login_access_type_default_description_lookup[i]] for i in range(4)]}
    return controller.CallBlockingToQt(gui, qt)
recorder.record = record
if __name__ == '__main__':
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child': recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path = os.path.join(work, 'login.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
            with open(path) as stream: result = json.load(stream)
        with open(OUT, 'w') as stream:
            json.dump(result, stream, indent=1, ensure_ascii=False); stream.write('\n')
        print(f'wrote {OUT}')
