#!/usr/bin/env python3
"""Record reference demand-login eligibility and ordinary loopback downloader admission."""
import http.server
import json
import os
import sys
import tempfile
import threading
import time
import record_string_converter_editor as recorder
HERE=os.path.dirname(os.path.abspath(__file__))
OUT=os.path.join(HERE,'fixtures/login_demand.json')
def record(session):
    c=session.controller
    requests=[]
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append({'path':self.path,'cookie':self.headers.get('Cookie')})
            self.send_response(200)
            if self.path=='/login':self.send_header('Set-Cookie','session=ok; Path=/')
            self.end_headers();self.wfile.write(b'synthetic loopback response')
        def log_message(self,*args):pass
    server=http.server.ThreadingHTTPServer(('127.0.0.1',80),Handler)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    domain='127.0.0.1'
    def qt():
        from hydrus.client import ClientStrings as S
        from hydrus.client.networking import ClientNetworkingLogin as L,ClientNetworkingContexts as N,ClientNetworkingSessions as C,ClientNetworkingJobs as J
        from hydrus.core import HydrusTime as T
        fixed=lambda v:S.StringMatch(match_type=S.STRING_MATCH_FIXED,match_value=v,example_string=v)
        step=L.LoginStep('establish session','http','GET',None,'/login')
        script=L.LoginScriptDomain('demand fixture',required_cookies_info={fixed('session'):fixed('ok')},login_steps=[step])
        script.SetLoginScriptKey(bytes(range(32)))
        engine=c.network_engine
        if engine._pause_all_new_network_traffic:engine.PausePlayNewJobs()
        manager=L.NetworkLoginManager();manager.engine=engine;manager.SetLoginScripts([script])
        original=engine.login_manager
        engine.login_manager=manager
        base=(script.GetLoginScriptKeyAndName(),{},0,'synthetic fixture',True,L.VALIDITY_UNTESTED,'',0,'')
        def context(domain):return N.NetworkContext(N.CC.NETWORK_CONTEXT_DOMAIN,domain)
        states=[]
        try:
            now=T.GetNow()
            for name,domain_entries,cookies,requested in [
                ('unconfigured',{},[], 'login.example.net'),
                ('inactive',{'login.example.net':(*base[:4],False,*base[5:])},[],'login.example.net'),
                ('ready',{'login.example.net':base},[],'files.login.example.net'),
                ('www-specific',{'www.login.example.net':base,'login.example.net':(*base[:4],False,*base[5:])},[],'www.login.example.net'),
                ('specific-inactive',{'files.login.example.net':(*base[:4],False,*base[5:]),'login.example.net':base},[],'files.login.example.net'),
                ('invalid',{'login.example.net':(*base[:5],L.VALIDITY_INVALID,'synthetic invalid',0,'')},[],'login.example.net'),
                ('delayed',{'login.example.net':(*base[:7],now+3600,'synthetic delay')},[],'login.example.net'),
                ('already-logged-in',{'login.example.net':base},[('session','ok')],'login.example.net'),
                ('missing-script',{'login.example.net':((bytes([255])*32,'missing'),*base[1:])},[],'login.example.net'),
                ('name-fallback',{'login.example.net':((bytes([255])*32,script.GetName()),*base[1:])},[],'login.example.net')]:
                manager.SetDomainsToLoginInfo(domain_entries)
                ctx=context(requested);engine.session_manager.ClearSession(ctx)
                for key,value in cookies:C.AddCookieToSession(engine.session_manager.GetSession(ctx),key,value,'login.example.net','/',None)
                before=manager.GetSerialisableTuple()
                needs=manager.CurrentlyNeedsLogin(ctx)
                try:manager.CheckCanLogin(ctx);error=None
                except Exception as e:error=str(e)
                states.append({'name':name,'requested':requested,'cookies':cookies,'before':before,'status':list(manager._GetLoginDomainStatus(ctx)),'needs':needs,'error':error,'after':manager.GetSerialisableTuple()})
            runtime=[]
            for name,active,cookie in [('automatic',True,False),('existing-cookie',True,True),('inactive-request',False,False),('cancel-trigger',True,False)]:
                manager.SetDomainsToLoginInfo({domain:(*base[:4],active,*base[5:])})
                ctx=context(domain);engine.session_manager.ClearSession(ctx)
                if cookie:C.AddCookieToSession(engine.session_manager.GetSession(ctx),'session','ok','127.0.0.1','/',None)
                requests.clear()
                job=J.NetworkJob('GET',f'http://{domain}/data')
                job.OverrideBandwidth()
                engine.AddJob(job)
                if name=='cancel-trigger':
                    deadline=time.monotonic()+5
                    while not requests and time.monotonic()<deadline:time.sleep(.02)
                    assert requests and requests[0]['path']=='/login'
                    job.Cancel()
                    assert job.IsDone()
                    job=J.NetworkJob('GET',f'http://{domain}/data');job.OverrideBandwidth();engine.AddJob(job)
                deadline=time.monotonic()+15
                while not job.IsDone() and time.monotonic()<deadline:time.sleep(.02)
                if not job.IsDone():job.Cancel();raise AssertionError(f'reference demand request did not finish: {job.GetStatus()} requests={requests}')
                job.WaitUntilDone()
                runtime.append({'name':name,'active':active,'initial_cookie':cookie,'trigger_cancelled':name=='cancel-trigger','requests':list(requests),'logged_in':script.IsLoggedIn(engine,ctx),'after':manager.GetSerialisableTuple()})
            popup=[]
            original_pub=c.pub
            def capture(topic,*args,**kwargs):
                if topic=='message':popup.append(args[0])
                return original_pub(topic,*args,**kwargs)
            c.pub=capture
            try:
                cancelled=script.Duplicate()
                cancelled._login_steps.append(L.LoginStep('must not run','http','GET',None,'/second'))
                manager.SetLoginScripts([cancelled]);manager.SetDomainsToLoginInfo({domain:base})
                engine.session_manager.ClearSession(context(domain));requests.clear()
                job=J.NetworkJob('GET',f'http://{domain}/data');job.OverrideBandwidth();engine.AddJob(job)
                deadline=time.monotonic()+5
                while (not requests or not popup) and time.monotonic()<deadline:time.sleep(.02)
                assert requests and popup
                title=popup[-1].GetStatusTitle();status=popup[-1].GetStatusText()
                popup[-1].Cancel()
                deadline=time.monotonic()+5
                while 'User cancelled' not in job.GetStatus()[0] and not job.IsDone() and time.monotonic()<deadline:time.sleep(.02)
                queued_status=job.GetStatus()[0]
                assert 'User cancelled' in queued_status
                process_requests=list(requests)
                job.Cancel()
                try:job.WaitUntilDone();queued_error=None
                except Exception as e:queued_error=str(e)
                cancelled_process={'script':cancelled.GetSerialisableTuple(),'title':title,'status':status,'final_status':popup[-1].GetStatusText(),'queued_status':queued_status,'queued_error':queued_error,'requests':process_requests,'after':manager.GetSerialisableTuple()}
            finally:c.pub=original_pub
            blocked=[]
            for name in ['ordinary','subscription']:
                engine.session_manager.ClearSession(context(domain))
                manager.SetDomainsToLoginInfo({domain:(*base[:5],L.VALIDITY_INVALID,'synthetic invalid',0,'')})
                requests.clear()
                job=J.NetworkJob('GET',f'http://{domain}/data') if name=='ordinary' else J.NetworkJobSubscription('synthetic query','GET',f'http://{domain}/data')
                job.OverrideBandwidth();engine.AddJob(job)
                deadline=time.monotonic()+5
                while not job.IsDone() and 'synthetic invalid' not in job.GetStatus()[0] and time.monotonic()<deadline:time.sleep(.02)
                before=job.GetStatus()[0]
                assert 'synthetic invalid' in before
                if name=='ordinary':job.Cancel()
                try:job.WaitUntilDone();error=None
                except Exception as e:error=str(e)
                blocked.append({'name':name,'before_cancel':before,'error':error,'requests':list(requests)})
            return {'now':now,'cancelled_process':cancelled_process,'blocked':blocked,'script':script.GetSerialisableTuple(),'states':states,'runtime':runtime,'ordinary_waits':J.NetworkJob.WILLING_TO_WAIT_ON_INVALID_LOGIN,'subscription_waits':J.NetworkJobSubscription.WILLING_TO_WAIT_ON_INVALID_LOGIN}
        finally:engine.login_manager=original
    try:return c.CallBlockingToQt(c.gui,qt)
    finally:server.shutdown();server.server_close();thread.join()
recorder.record=record
if __name__=='__main__':
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':recorder.child(sys.argv[2])
    else:
        with tempfile.TemporaryDirectory() as work:
            path=os.path.join(work,'login.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
            with open(path) as stream:result=json.load(stream)
        with open(OUT,'w') as stream:json.dump(result,stream,indent=1,ensure_ascii=False);stream.write('\n')
        print(f'wrote {OUT}')
