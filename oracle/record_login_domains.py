#!/usr/bin/env python3
"""Record real Qt domain Add/change-script/delete chains on synthetic domains."""
import json
import os
import sys
import tempfile
import record_string_converter_editor as recorder
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, 'fixtures/login_domains.json')

def record(session):
    c = session.controller
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui import ClientGUIDialogsQuick as D, ClientGUIDialogsMessage as M
        from hydrus.client.gui.networking import ClientGUILogin as G
        from hydrus.client.networking import ClientNetworkingLogin as L
        from hydrus.core import HydrusExceptions as E
        scripts = []
        for i, (name, examples) in enumerate([('a matching', [('login.example', 0, 'original access'), ('unused.example', 2, 'example access')]), ('b matching', [('login.example', 1, 'matched access')]), ('c other', [('other.example', 3, 'other access')]), ('d credentials', [])]):
            step = L.LoginStep('synthetic request', 'http', 'GET', None, '/')
            definitions = []
            if i == 3:
                definitions = [L.LoginCredentialDefinition('username', L.CREDENTIAL_TYPE_TEXT, S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='alice', example_string='alice'))]
                step.SetComplicatedVariables({'username': 'user'}, {}, {}, {}, [])
            script = L.LoginScriptDomain(name, credential_definitions=definitions, login_steps=[step], example_domains_info=examples)
            script.SetLoginScriptKey(bytes([i + 1]) * 32)
            scripts.append(script)
        initial = {'login.example': (scripts[0].GetLoginScriptKeyAndName(), {'username':'alice','legacy':'keep'}, 0, 'original access', True, L.VALIDITY_INVALID, 'old failure', 1900003600, 'old delay')}
        old_select, old_text, old_yes, old_warning, old_dialog = D.SelectFromList, D.EnterText, D.GetYesNo, M.ShowWarning, G.ClientGUITopLevelWindowsPanels.DialogEdit
        steps = []
        prompts = []
        warnings = []
        def select(parent, title, choices, **kwargs):
            op = steps.pop(0)
            selected = kwargs.get('value_to_select')
            prompts.append({'kind':'select','title':title,'choices':[name for name, value in choices], 'selected': selected.GetName() if selected is not None else None, 'sort':kwargs.get('sort_tuples',True),'answer':op})
            if op is None: raise E.CancelledException()
            return choices[op][1]
        def text(parent, message, **kwargs):
            op = steps.pop(0)
            prompts.append({'kind':'text','message':message,'default':kwargs.get('default',''),'placeholder':kwargs.get('placeholder'),'answer':op})
            if op is None: raise E.CancelledException()
            return op
        def yes(parent, message, **kwargs):
            op = steps.pop(0)
            prompts.append({'kind':'question','message':message,'answer':op})
            return QW.QDialog.DialogCode.Accepted if op else QW.QDialog.DialogCode.Rejected
        class CredentialsDialog(QW.QWidget):
            def __init__(self, *args, **kwargs): super().__init__(c.gui)
            def __enter__(self): return self
            def __exit__(self,*args): return False
            def SetPanel(self,panel): self.panel=panel
            def exec(self):
                op=steps.pop(0)
                prompts.append({'kind':'credentials','answer':op,'names':[definition.GetName() for definition,edit,label in self.panel._control_data]})
                if op is None:return QW.QDialog.DialogCode.Rejected
                for definition,edit,label in self.panel._control_data:edit.setText(op.get(definition.GetName(),''))
                return QW.QDialog.DialogCode.Accepted
        D.SelectFromList, D.EnterText, D.GetYesNo = select, text, yes
        M.ShowWarning=lambda parent,message,*args,**kwargs:warnings.append(message)
        G.ClientGUITopLevelWindowsPanels.DialogEdit=CredentialsDialog
        def value(panel):
            return {domain:[[key.hex(),name], credentials, access, description, active, validity, error, delay, reason] for domain, ((key,name), credentials, access, description, active, validity, error, delay, reason) in panel.GetValue().items()}
        cases=[]
        try:
            scenarios=[
                ('no-scripts','add',[],[]),('cancel-script','add',[None],scripts),
                ('example-add','add',[0,0,True],scripts),
                ('custom-description-cancel','add',[0,1,'custom.example',3,None,False],scripts),
                ('duplicate','add',[0,1,'login.example'],scripts),
                ('credentials-cancel','add',[3,'creds.example',0,None,None],scripts),
                ('credentials-invalid','add',[3,'creds.example',0,'dummy access',{'username':'bad'}],scripts),
                ('change-matching','change',[1],scripts),('change-separator','change',[2],scripts),
                ('change-current','change',[0],scripts),('change-cancel','change',[None],scripts),
                ('change-unmatched-description-cancel','change',[3,2,None],scripts),
                ('change-invalid-credentials','change-invalid',[4,0,None],scripts),
                ('delete-decline','delete',[False],scripts),('delete-confirm','delete',[True],scripts),
            ]
            for name, action, answers, available in scenarios:
                steps[:]=answers;prompts.clear();warnings.clear()
                given=dict(initial)
                if action=='change-invalid':
                    old=list(initial['login.example']);old[1]={'username':'bad','legacy':'keep'};given['login.example']=tuple(old)
                panel=G.EditLoginsPanel(c.gui,c.network_engine,available,given)
                panel._domains_and_login_info.SelectDatas(panel._domains_and_login_info.GetData(),deselect_others=True)
                before=value(panel)
                if action=='add':panel._Add()
                elif action=='delete':panel._domains_and_login_info.ProcessDeleteAction()
                else:panel._EditLoginScript()
                cases.append({'name':name,'action':action,'answers':answers,'before':before,'after':value(panel),'prompts':list(prompts),'warnings':list(warnings)})
                assert not steps, (name,steps)
                panel.deleteLater()
            return {'scripts':[script.GetSerialisableTuple() for script in scripts],'cases':cases}
        finally:
            D.SelectFromList,D.EnterText,D.GetYesNo,M.ShowWarning,G.ClientGUITopLevelWindowsPanels.DialogEdit=old_select,old_text,old_yes,old_warning,old_dialog
    return c.CallBlockingToQt(c.gui,qt)

recorder.record=record
if __name__=='__main__':
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path=os.path.join(work,'domains.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
            with open(path) as stream:result=json.load(stream)
        with open(OUT,'w') as stream:json.dump(result,stream,indent=1,ensure_ascii=False);stream.write('\n')
        print(f'wrote {OUT}')
