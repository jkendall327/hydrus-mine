#!/usr/bin/env python3
"""Record actual login-script cog/error ownership, completion popup and help menu.

No HTTP requests run. Real Qt panels, jobs, managers, context presentation,
menu actions, worker cleanup and QMessageBox information handlers are driven.
The queued executor's Start return/exception is scripted only at the presentation
boundary; actual HTTP/stream/two-second behavior is in login_execution.json.
Documentation launch is captured, with the missing-HTML chooser cancelled.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures/login_script_controls.json')

def record(session):
    c=session.controller
    def qt():
        from qtpy import QtWidgets as QW, QtCore as QC
        from hydrus.core import HydrusTime, HydrusExceptions
        from hydrus.client.gui import ClientGUICore as Core, ClientGUIDialogsQuick as Quick, ClientGUIDialogsMessage as Message
        from hydrus.client.gui.networking import ClientGUILogin as G
        from hydrus.client.gui.widgets import ClientGUIMenuButton as Menus
        from hydrus.client.gui.executables import ClientGUIExecutableActions as Executables
        from hydrus.client.networking import ClientNetworkingLogin as L, ClientNetworkingJobs as Jobs
        now=[1770000000];old_now=HydrusTime.GetNow;HydrusTime.GetNow=lambda:now[0]
        script=L.LoginScriptDomain('controls fixture',login_steps=[L.LoginStep('synthetic step','http','GET',None,'/')],example_domains_info=[('example.com',0,'synthetic fixture')])
        panel=G.EditLoginScriptPanel(c.gui,script)
        control=panel._test_network_job_control
        captured=[];clipboard=[];errors=[];launches=[];help_choices=[];queued=[];after=[]
        old_popup=Core.core().PopupMenu;old_pub=c.pub;old_critical=Message.ShowCritical
        old_text=Quick.EnterText;old_thread=c.CallToThread;old_long=c.CallToThreadLongRunning;old_after=c.CallAfterQtSafe
        old_start=L.LoginScriptDomain.Start;old_choice=Quick.GetYesYesNo
        old_path=Executables.OpenExternallyPathAsURL;old_url=Executables.OpenExternallyURLDefault
        Core.core().PopupMenu=lambda owner,menu:captured.append(menu)
        c.pub=lambda *args,**kwargs:clipboard.append(list(args)) if args and args[0]=='clipboard' else None
        Message.ShowCritical=lambda parent,title,text:errors.append(dict(title=title,message=text,parent_is_control=parent is control))
        def rows(menu):
            return [dict(label=a.text(),separator=a.isSeparator(),checked=a.isChecked() if a.isCheckable() else None,children=rows(a.menu()) if a.menu() else []) for a in menu.actions()]
        def menu():
            control._ShowCogMenu();return rows(captured[-1])
        def trigger(label):
            action=next(a for a in captured[-1].actions() if a.text()==label);action.trigger()
        def state():
            return dict(error=control._error_text,error_hidden=control._error_button.isHidden(),has_job=control.HasNetworkJob(),cancel_enabled=control._cancel_button.isEnabled(),left=control._left_text.text(),running=panel._currently_testing,run_enabled=panel._test_button.isEnabled(),final=panel._final_test_result.text())
        try:
            out=dict(now=now[0],menus={'empty':menu()},actions={},completion=[])
            job=Jobs.NetworkJob('GET','https://example.com/login%20step');job.engine=c.network_engine
            factory=G.GenerateTestNetworkJobPresentationContextFactory(panel,control)
            c.CallAfterQtSafe=lambda owner,fn,*args,**kwargs:fn(*args,**kwargs)
            with factory(job):
                out['menus']['running']=menu()
                job.SetForLogin(True);out['menus']['login']=menu();out['actions']['login_obeys_bandwidth']=job.ObeysBandwidth()
                job.SetForLogin(False);control._ShowCogMenu()
                trigger('override bandwidth rules for this job');out['actions']['obeys_after_override']=job.ObeysBandwidth()
            out['actions']['after_context_exit']=state()
            job2=Jobs.NetworkJob('GET','https://example.com/second');job2.engine=c.network_engine
            with factory(job2):
                control.FlipAutoOverrideBandwidth();now[0]+=5;control._OverrideBandwidthIfAppropriate();out['actions']['auto_at_five']=job2.ObeysBandwidth()
                now[0]+=1;control._OverrideBandwidthIfAppropriate();out['actions']['auto_after_five']=job2.ObeysBandwidth()
                out['menus']['auto']=menu()
                control.Cancel();out['actions']['cancelled']=job2.IsCancelled()
            failing=Jobs.NetworkJob('GET','https://example.com/failure');failing.engine=c.network_engine
            failing._error_exception=RuntimeError('synthetic network failure');failing._error_text='server detail';failing.SetStatus('error!')
            with factory(failing):control._Update();out['actions']['failed_job']=state()
            control.SetError('explicit owner error\nserver detail');control._ShowErrorMenu();out['menus']['error']=rows(captured[-1]);trigger('show error');trigger('copy error')
            out['actions']['after_explicit_error']=state();control.ClearNetworkJob();out['actions']['clear_job_keeps_error']=state();control.ClearError();out['actions']['clear_error']=state()
            out['clipboard']=clipboard;out['errors']=errors
            help_button=next(button for button in panel.findChildren(Menus.MenuIconButton) if any(item.GetTitle()=='open the login scripts help' for item in button._menu_template_items))
            help_button.DoMenu();out['help_menu']=rows(captured[-1])
            Executables.OpenExternallyPathAsURL=lambda owner,path:launches.append(dict(kind='local',path=os.path.basename(path),parent_is_panel=owner is panel))
            Executables.OpenExternallyURLDefault=lambda owner,url:launches.append(dict(kind='web',parent_is_panel=owner is panel))
            def choose(owner,message,**kwargs):
                help_choices.append(dict(message=message,choices=[label for label,value in kwargs['yes_tuples']],cancel=kwargs['no_label'],parent_is_panel=owner is panel));raise HydrusExceptions.CancelledException()
            Quick.GetYesYesNo=choose;trigger('open the login scripts help');out['help_launches']=launches;out['help_missing_html']=help_choices
            Quick.EnterText=lambda *args,**kwargs:'example.com'
            c.CallToThread=lambda fn,*args,**kwargs:queued.append((fn,args,kwargs))
            c.CallToThreadLongRunning=lambda *args,**kwargs:None
            c.CallAfterQtSafe=lambda owner,fn,*args,**kwargs:after.append((fn,args,kwargs))
            for text,raises in [('Login OK!',False),('User cancelled the login process.',False),('synthetic unexpected failure',True)]:
                panel._DoTest();before=state();worker,args,kwargs=queued.pop()
                def start(self,*args,**kwargs):
                    if raises:raise RuntimeError(text)
                    return text
                L.LoginScriptDomain.Start=start;worker(*args,**kwargs)
                visible=[]
                def acknowledge():
                    dialog=QW.QApplication.activeModalWidget();assert isinstance(dialog,QW.QMessageBox)
                    visible.append(dict(title=dialog.windowTitle(),message=dialog.text(),buttons=[button.text() for button in dialog.buttons()],parent_is_panel=dialog.parent() is panel,state=state()))
                    dialog.accept()
                QC.QTimer.singleShot(0,acknowledge)
                cleanup,values,keywords=after.pop();cleanup(*values,**keywords)
                out['completion'].append(dict(input=text,raises=raises,before=before,visible=visible[0],after=state()))
            return out
        finally:
            Core.core().PopupMenu=old_popup;c.pub=old_pub;Message.ShowCritical=old_critical
            Quick.EnterText=old_text;c.CallToThread=old_thread;c.CallToThreadLongRunning=old_long;c.CallAfterQtSafe=old_after;L.LoginScriptDomain.Start=old_start;Quick.GetYesYesNo=old_choice
            Executables.OpenExternallyPathAsURL=old_path;Executables.OpenExternallyURLDefault=old_url;HydrusTime.GetNow=old_now
            panel._currently_testing=False;panel.deleteLater()
    return c.CallBlockingToQt(c.gui,qt)
def child(path):
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(path,'w')as stream:json.dump(result,stream)
if __name__=='__main__':
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2])
    else:
        import hydrus_driver
        with tempfile.TemporaryDirectory()as work:
            path=os.path.join(work,'controls.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
            with open(path)as stream:result=json.load(stream)
        with open(OUT,'w')as stream:json.dump(result,stream,indent=2,ensure_ascii=False);stream.write('\n')
        print('wrote',OUT)
