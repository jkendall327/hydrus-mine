#!/usr/bin/env python3
"""Execute real reference login steps against a dummy loopback HTTP site only."""
import json
import os
import sys
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import parse_qs
import record_string_converter_editor as recorder
HERE=os.path.dirname(os.path.abspath(__file__))
OUT=os.path.join(HERE,'fixtures/login_execution.json')

def record(session):
    controller=session.controller
    requests=[]
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*args): pass
        def serve(self):
            body=self.rfile.read(int(self.headers.get('Content-Length','0'))).decode()
            requests.append({'method':self.command,'path':self.path,'body':body,'referer':self.headers.get('Referer'),'origin':self.headers.get('Origin'),'cookie':self.headers.get('Cookie')})
            if self.path=='/unauthorised': code=401; data=b'credentials denied'; cookie=None
            elif self.command=='GET': code=200; data=b'<input name="csrf" value="loop-token"><p>start</p>'; cookie='preflight=ready; Path=/'
            elif self.path=='/missing-cookie': code=200; data=b'no cookie'; cookie=None
            else:
                values=parse_qs(body,keep_blank_values=True)
                ok=values=={'mode':['login'],'pass':['dummy + pass&='],'token':['loop-token'],'user':['alice']}
                code=200 if ok else 403; data=b'login response' if ok else b'bad form'; cookie='session=ok; Path=/' if ok else None
            self.send_response(code);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(data)))
            if cookie:self.send_header('Set-Cookie',cookie)
            self.end_headers();self.wfile.write(data)
        do_GET=serve
        do_POST=serve
    # Port 80 keeps the reference's domain cookie lookup free of its port mismatch.
    server=HTTPServer(('127.0.0.1',80),Handler)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    def qt():
        from hydrus.client import ClientStrings as S, ClientThreading
        from hydrus.client.gui.networking import ClientGUILogin as G
        from hydrus.client.networking import ClientNetworkingLogin as L, ClientNetworkingContexts as NC
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable
        engine=controller.network_engine
        if engine._pause_all_new_network_traffic: engine.PausePlayNewJobs()
        context=NC.NetworkContext(NC.CC.NETWORK_CONTEXT_DOMAIN,'127.0.0.1')
        fixed=lambda value:S.StringMatch(match_type=S.STRING_MATCH_FIXED,match_value=value,example_string=value)
        required={fixed('session'):fixed('ok')}
        formula=P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='input',tag_attributes={'name':'csrf'})],content_to_fetch=P.HTML_CONTENT_ATTRIBUTE,attribute_to_fetch='value')
        variable=P.ContentParser(name='csrf variable',content_type=HC.CONTENT_TYPE_VARIABLE,formula=formula,additional_info='csrf')
        first=L.LoginStep('establish session','http','GET',None,'/start')
        first.SetComplicatedVariables({}, {'lang':'en'}, {}, {}, [variable])
        second=L.LoginStep('send credentials','http','POST',None,'/login')
        second.SetComplicatedVariables({'username':'user','password':'pass'}, {'mode':'login'}, {'csrf':'token'}, required, [])
        credentials={'username':'alice','password':'dummy + pass&='}
        definitions=[L.LoginCredentialDefinition('username',L.CREDENTIAL_TYPE_TEXT,fixed('alice')),L.LoginCredentialDefinition('password',L.CREDENTIAL_TYPE_PASS,fixed(credentials['password']))]
        base=L.LoginScriptDomain('loopback login',required_cookies_info=required,credential_definitions=definitions,login_steps=[first,second],example_domains_info=[('127.0.0.1',0,'synthetic local fixture')])
        base.SetLoginScriptKey(bytes(range(32)))
        cases=[]
        for name in ['success','missing_cookie','veto','final_cookie','network_401','missing_variable','cancel_before_start']:
            script=base.Duplicate()
            if name=='missing_cookie':script._login_steps[1]._path='/missing-cookie'
            elif name=='veto':
                veto=P.ContentParser(name='blocked response',content_type=HC.CONTENT_TYPE_VETO,formula=P.ParseFormulaStatic(static_text='blocked'),additional_info=(True,fixed('blocked')))
                script._login_steps[0]._content_parsers.append(veto)
            elif name=='final_cookie':script._required_cookies_info=HydrusSerialisable.SerialisableDictionary({fixed('session'):fixed('wrong')})
            elif name=='network_401':script._login_steps=HydrusSerialisable.SerialisableList([L.LoginStep('denied request','http','GET',None,'/unauthorised')])
            elif name=='missing_variable':script._login_steps[0]._temp_args={'missing':'token'}
            engine.session_manager.ClearSession(context)
            results=[];requests.clear()
            status=ClientThreading.JobStatus(cancellable=True)
            if name=='cancel_before_start':status.Cancel()
            outcome=script.Start(engine,context,credentials,test_result_callable=results.append,job_status=status)
            cases.append({'name':name,'script':script.GetSerialisableTuple(),'credentials':credentials,'results':results,'outcome':outcome,'requests':list(requests),'logged_in':script.IsLoggedIn(engine,context)})
        copied=[]
        original_pub=controller.pub
        def capture(topic,*args,**kwargs):
            if topic=='clipboard':copied.append(list(args))
            return original_pub(topic,*args,**kwargs)
        controller.pub=capture
        def review(result):
            copied.clear();panel=G.ReviewTestResultPanel(controller.gui,result)
            state={'name':panel._name.text(),'url':panel._url.text(),'body':panel._body.toPlainText(),'data':panel._data_preview.toPlainText(),'variables':panel._temp_variables.toPlainText(),'cookies':panel._cookies.toPlainText(),'result':panel._result.text(),'read_only':[panel._url.isReadOnly(),panel._body.isReadOnly(),panel._data_preview.isReadOnly(),panel._temp_variables.isReadOnly(),panel._cookies.isReadOnly()]}
            panel._CopyData();state['copied']=list(copied);panel.deleteLater();return state
        try:
            for case in cases:case['reviews']=[review(result) for result in case['results']]
            long=list(cases[0]['results'][0]);long[3]='α🙂'*700
            cases[0]['long_review']={'input':long,'state':review(long)}
        finally:controller.pub=original_pub
        return cases
    try:return controller.CallBlockingToQt(controller.gui,qt)
    finally:server.shutdown();server.server_close();thread.join()
recorder.record=record
if __name__=='__main__':
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record,network=True)
        with open(output,'w') as stream:json.dump(result,stream,ensure_ascii=False)
    else:
        with tempfile.TemporaryDirectory() as work:
            path=os.path.join(work,'execution.json')
            hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
            with open(path) as stream:result=json.load(stream)
        with open(OUT,'w') as stream:json.dump(result,stream,indent=1,ensure_ascii=False);stream.write('\n')
        print(f'wrote {OUT}')
