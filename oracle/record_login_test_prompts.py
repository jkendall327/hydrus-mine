#!/usr/bin/env python3
"""Record actual Qt script-test domain/credentials prompts, remembered values and clear timing."""
import json
import os
import sys
import tempfile
import record_string_converter_editor as recorder
HERE=os.path.dirname(os.path.abspath(__file__))
OUT=os.path.join(HERE,'fixtures/login_test_prompts.json')
def record(session):
    c=session.controller
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui import ClientGUIDialogsQuick as D
        from hydrus.client.gui.networking import ClientGUILogin as G
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsTextEntry as T
        from hydrus.client.networking import ClientNetworkingLogin as L
        from hydrus.core import HydrusExceptions as E
        step=L.LoginStep('synthetic request','http','GET',None,'/')
        step.SetComplicatedVariables({'username':'user'},{},{},{},[])
        script=L.LoginScriptDomain('test prompt',credential_definitions=[L.LoginCredentialDefinition('username',L.CREDENTIAL_TYPE_TEXT,S.StringMatch())],login_steps=[step],example_domains_info=[('z.example',0,'last'),('a.example',1,'first')])
        script.SetLoginScriptKey(bytes(range(32)))
        panel=G.EditLoginScriptPanel(c.gui,script)
        answer=[None,None]
        prompts=[];started=[]
        old_text,old_dialog,old_thread=D.EnterText,G.ClientGUITopLevelWindowsPanels.DialogEdit,c.CallToThread
        def text(parent,message,**kwargs):
            prompts.append({'kind':'domain','message':message,'default':kwargs['default'],'answer':answer[0]})
            if answer[0] is None:raise E.CancelledException()
            entry=T.EditTextPanel(c.gui,message,default=kwargs['default'])
            entry._text.setText(answer[0])
            try:return entry.GetValue()
            finally:entry.deleteLater()
        class CredentialsDialog(QW.QWidget):
            def __init__(self,*args,**kwargs):super().__init__(c.gui)
            def __enter__(self):return self
            def __exit__(self,*args):return False
            def SetPanel(self,panel):self.panel=panel
            def exec(self):
                prompts.append({'kind':'credentials','initial':self.panel.GetValue(),'answer':answer[1]})
                if answer[1] is None:return QW.QDialog.DialogCode.Rejected
                for definition,edit,label in self.panel._control_data:edit.setText(answer[1][definition.GetName()])
                return QW.QDialog.DialogCode.Accepted
        def thread(fn,*args,**kwargs):
            started.append({'domain':args[1],'credentials':args[2]})
        D.EnterText=text;G.ClientGUITopLevelWindowsPanels.DialogEdit=CredentialsDialog;c.CallToThread=thread
        def state():
            return {'domain':panel._test_domain,'credentials':panel._test_credentials,'running':panel._currently_testing,'run_enabled':panel._test_button.isEnabled(),'result_count':len(panel._test_listctrl.GetData())}
        states=[]
        try:
            for name,given in [('cancel-first-domain',[None,None]),('blank-domain',['',None]),('start',['runtime.example',{'username':'alice'}]),('cancel-credentials',['remember.example',None]),('cancel-remembered-domain',[None,None]),('start-again',['next.example',{'username':'bob'}])]:
                answer[:]=given;prompts.clear();started.clear()
                panel._currently_testing=False;panel._test_button.setEnabled(True)
                if len(panel._test_listctrl.GetData())==0:panel._test_listctrl.AddData(('retained result','http://synthetic.example/',None,'retained data',[],[],'OK!'))
                before=state()
                panel._DoTest()
                states.append({'name':name,'before':before,'prompts':list(prompts),'started':list(started),'after':state()})
            return {'script':script.GetSerialisableTuple(),'button':panel._test_button.text(),'states':states}
        finally:
            D.EnterText,G.ClientGUITopLevelWindowsPanels.DialogEdit,c.CallToThread=old_text,old_dialog,old_thread
            panel._currently_testing=False;panel.deleteLater()
    return c.CallBlockingToQt(c.gui,qt)
recorder.record=record
if __name__=='__main__':
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path=os.path.join(work,'prompts.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
            with open(path)as stream:result=json.load(stream)
        with open(OUT,'w')as stream:json.dump(result,stream,indent=1,ensure_ascii=False);stream.write('\n')
        print(f'wrote {OUT}')
