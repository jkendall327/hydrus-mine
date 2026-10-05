#!/usr/bin/env python3
"""Real Qt staged preference and authenticated cookie/header API job/toaster effects.

Real access-key establishment, permission checks, API handlers, network managers,
JobStatus and PopupMessage run on a copied basic client. Only message publication
is captured; private popup widgets render those actual jobs. Reserved domains and
owned client options/managers avoid external requests or user data changes.
"""
import json,sys,tempfile,types
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from twisted.web.http_headers import Headers
        from hydrus.client import ClientAPI,ClientConstants as CC
        from hydrus.client.gui import ClientGUIPopupMessages as PM
        from hydrus.client.gui.panels.options.PopupPanel import PopupPanel
        from hydrus.client.networking.api import ClientLocalServerResourcesManageCookies as R
        from hydrus.core import HydrusConstants as HC,HydrusTime
        from hydrus.core.networking.HydrusNetworkVariableHandling import ParsedRequestArguments
        c=session.controller
        option='notify_client_api_cookies'
        old_options=c.new_options.Duplicate();old_api=c.client_api_manager;old_pub=c.pub
        old_domain=c.network_engine.domain_manager;old_sessions=c.network_engine.session_manager
        widgets=[];published=[]
        try:
            c.client_api_manager=ClientAPI.APIManager()
            for key,allowed in [(71,True),(72,False)]:
                c.client_api_manager.AddAccess(ClientAPI.APIPermissions('toast recording',access_key=bytes([key])*32,permits_everything=False,basic_permissions=[ClientAPI.CLIENT_API_PERMISSION_MANAGE_HEADERS] if allowed else []))
            c.network_engine.domain_manager=old_domain.Duplicate()
            c.network_engine.session_manager=old_sessions.Duplicate()
            def pub(topic,*args,**kwargs):
                if topic=='message':published.append(args[0])
                else:old_pub(topic,*args,**kwargs)
            c.pub=pub
            service=c.services_manager.GetService(CC.CLIENT_API_SERVICE_KEY)
            resources={'cookies':R.HydrusResourceClientAPIRestrictedManageCookiesSetCookies(service,None),'headers':R.HydrusResourceClientAPIRestrictedManageCookiesSetHeaders(service,None)}
            def panel():
                p=PopupPanel(c.gui,c.new_options);widgets.append(p);return p
            p=panel();out={'default':{'raw':c.new_options.GetBoolean(option),'checked':p._notify_client_api_cookies.isChecked(),'enabled':p._notify_client_api_cookies.isEnabled()},'controls':[],'requests':[]}
            for before in [False,True]:
                for typed in [False,True]:
                    for apply in [False,True]:
                        c.new_options.SetBoolean(option,before);p=panel();p._notify_client_api_cookies.setChecked(typed)
                        if apply:p.UpdateOptions()
                        reopened=panel();out['controls'].append({'before':before,'typed':typed,'apply':apply,'saved':c.new_options.GetBoolean(option),'reopened':reopened._notify_client_api_cookies.isChecked()})
            c.new_options.SetBoolean(option,True);out['legacy_options']=c.new_options.GetSerialisableTuple()
            p=panel();p.resize(950,700);p.show();QW.QApplication.processEvents();p.grab().save(str(HERE/'fixtures/api_update_toasts_options_qt.png'));p.hide()
            def invoke(name,worker,body,enabled=True,key=71):
                c.new_options.SetBoolean(option,enabled);published.clear()
                request=types.SimpleNamespace(requestHeaders=Headers({'Hydrus-Client-API-Access-Key':[bytes([key]*32).hex()]}),parsed_request_args=ParsedRequestArguments(body),client_api_permissions=None,preferred_mime=HC.APPLICATION_JSON)
                resource=resources[worker]
                result={'name':name,'worker':worker,'body':body,'enabled':enabled,'key':key}
                try:
                    resource._callbackEstablishAccountFromHeader(request)
                    resource._callbackEstablishAccountFromArgs(request)
                    resource._CheckAPIPermissions(request)
                    response=resource._threadDoPOSTJob(request)
                    result['status']=response.GetStatusCode();result['error']=None
                except Exception as e:
                    result['status']=403 if type(e).__name__ in ['InsufficientCredentialsException'] else 400
                    result['error']=type(e).__name__
                result['jobs']=[{'text':j.GetStatusText(),'done':j.IsDone(),'dismissed':j.IsDismissed(),'delay':j._finish_and_dismiss_time-HydrusTime.GetNow(),'pausable':j.IsPausable(),'cancellable':j.IsCancellable()} for j in published]
                out['requests'].append(result)
                return list(published)
            a='a.toast.example.invalid';z='z.toast.example.invalid'
            invoke('disabled_cookie','cookies',{'cookies':[['session','first',a,'/',None]]},False)
            jobs=invoke('cookies_sorted_deduplicated','cookies',{'cookies':[['one','z',z,'/',None],['two','a',a,'/',None],['three','a',a,'/',None]]})
            invoke('same_cookie_still_set','cookies',{'cookies':[['two','a',a,'/',None]]})
            invoke('cookies_clear_and_set','cookies',{'cookies':[['one',None,z,'/',None],['two',None,a,'/',None],['new','value',a,'/',None]]})
            invoke('empty_cookies','cookies',{'cookies':[]})
            invoke('invalid_cookie','cookies',{'cookies':[['bad',1,a,'/',None]]})
            invoke('forbidden_cookie','cookies',{'cookies':[['bad','ignored',a,'/',None]]},key=72)
            domain={'domain':a}
            invoke('disabled_header','headers',{**domain,'headers':{'X-Old':{'value':'old'}}},False)
            invoke('headers_sorted_set','headers',{**domain,'headers':{'Z-New':{'value':'z'},'A-New':{'value':'a'}}})
            invoke('same_header_noop','headers',{**domain,'headers':{'A-New':{'value':'a','approved':'denied','reason':'same value'}}})
            invoke('missing_header_clear_noop','headers',{**domain,'headers':{'X-Missing':{'value':None}}})
            invoke('altered_only_header_quirk','headers',{**domain,'headers':{'A-New':{'value':'a2'}}})
            invoke('approval_only_header_quirk','headers',{**domain,'headers':{'A-New':{'approved':'denied','reason':'changed approval'}}})
            invoke('headers_all_categories','headers',{**domain,'headers':{'Z-New':{'value':None},'A-New':{'value':'a3'},'C-New':{'value':'c'}}})
            invoke('headers_clear_only','headers',{**domain,'headers':{'X-Old':{'value':None},'C-New':{'value':None}}})
            invoke('empty_headers','headers',{**domain,'headers':{}})
            invoke('invalid_header','headers',{**domain,'headers':{'bad':{'approved':'not-valid'}}})
            invoke('missing_approval_header_error','headers',{**domain,'headers':{'No-Entry':{'reason':'missing'}}})
            invoke('forbidden_header','headers',{**domain,'headers':{'blocked':{'value':'value'}}},key=72)
            invoke('late_missing_header_error','headers',{**domain,'headers':{'A-Before-Error':{'value':'persisted'},'Z-Missing-Entry':{'reason':'cannot create without value'}}})
            job=jobs[0];p=PM.PopupMessage(c.gui,job);widgets.append(p);p.UpdateMessage();p.resize(800,130);p.show();QW.QApplication.processEvents();p.grab().save(str(HERE/'fixtures/api_update_toasts_popup_qt.png'));p.hide()
            # Execute the real JobStatus cancel/expiry method with a private
            # HydrusTime namespace, leaving shared client clock globals intact.
            deadline=job._finish_and_dismiss_time;clock=[deadline-5]
            globals_=dict(HydrusTime.TimeHasPassed.__globals__);globals_['GetNow']=lambda:clock[0]
            real_time=types.SimpleNamespace(**vars(HydrusTime));real_time.TimeHasPassed=types.FunctionType(HydrusTime.TimeHasPassed.__code__,globals_)
            globals_float=dict(HydrusTime.TimeHasPassedFloat.__globals__);globals_float['GetNowFloat']=lambda:float(clock[0])
            real_time.GetNowFloat=lambda:float(clock[0]);real_time.TimeHasPassedFloat=types.FunctionType(HydrusTime.TimeHasPassedFloat.__code__,globals_float)
            checker=job._cancel_tests_regular_checker;checker._next_check=clock[0]-0.1
            checker_globals=dict(checker.Due.__func__.__globals__);checker_globals['HydrusTime']=real_time
            checker.Due=types.MethodType(types.FunctionType(checker.Due.__func__.__code__,checker_globals),checker)
            globals_=dict(job._CheckCancelTests.__func__.__globals__);globals_['HydrusTime']=real_time
            job._CheckCancelTests=types.MethodType(types.FunctionType(job._CheckCancelTests.__func__.__code__,globals_),job)
            out['dismissal']=[]
            for relative in [0,4,5,6]:
                clock[0]=deadline-5+relative;out['dismissal'].append({'seconds':relative,'done':job.IsDone(),'dismissed':job.IsDismissed()})
            out['limits']='Real access-key parsing/manager establishment, manage-headers permission checks, API POST handlers, network managers and JobStatus/PopupMessage execute; socket/body/service transport is not exercised. Reserved domains and private copied fixture; no external network request. Shared clock remains unchanged.'
            return out
        finally:
            c.new_options=old_options;c.client_api_manager=old_api;c.pub=old_pub
            c.network_engine.domain_manager=old_domain;c.network_engine.session_manager=old_sessions
            for p in widgets:p.hide();p.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1:
        output=Path(sys.argv[2]);result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record);output.write_text(json.dumps(result));return
    with tempfile.TemporaryDirectory() as d:
        path=Path(d)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(path));result=json.loads(path.read_text())
    path=HERE/'fixtures/api_update_toasts.json';path.write_text(json.dumps(result,indent=2)+'\n');print('wrote '+str(path))
if __name__=='__main__':main()
