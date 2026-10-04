#!/usr/bin/env python3
"""Record real Qt parser/formula test-document conversion and fetch callbacks.
Network boundaries return scripted responses on reserved synthetic URLs, while
actual Qt fetch controls, context updates, referral/one-shot settings, errors,
cancellation and recursive child data callbacks remain reference code.
Rust GUI tests separately send real requests to a local HTTP server.
"""
import json
import os
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    def qt():
        from hydrus.client.gui.parsing import ClientGUIParsing as Pages, ClientGUIParsingFormulae as Formulae
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.networking import ClientNetworkingJobs
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client import ClientStrings as S
        from hydrus.core import HydrusExceptions
        root = session.controller.gui
        context = {'url':'https://test-docs.example/original','post_index':'12','token':'preserved'}
        converter = S.StringConverter(conversions=[(S.STRING_CONVERSION_APPEND_TEXT,'<!-- converted -->')])
        page = P.PageParser(name='examples',string_converter=converter,example_urls=['https://test-docs.example/default'])
        panel = Pages.EditPageParserPanel(root,page,test_data=P.ParsingTestData(context,['']))
        out = []
        for index,text in enumerate(['<p>first</p>','<p>second</p>','<p>edited first</p>']):
            panel._test_panel.SetExampleData(text)
            inherited = panel._test_panel.GetTestDataForChild()
            out.append({'case':'converted_example','sequence':index,'raw':text,'child_texts':inherited.texts,'context':dict(inherited.parsing_context)})
        # Keep all observable NetworkJob settings without contacting the network.
        response = {'text':'<p>fetched café</p>','mode':'success'}
        requests = []
        class Job:
            def __init__(self,method,url,referral_url=None,**kwargs):
                self.info = {'method':method,'url':url,'referral':referral_url,'override':False,'one_shot':False}
                requests.append(self.info)
            def OverrideBandwidth(self): self.info['override'] = True
            def OnlyTryConnectionOnce(self): self.info['one_shot'] = True
            def WaitUntilDone(self):
                if response['mode']=='cancel': raise HydrusExceptions.CancelledException()
                if response['mode']=='error': raise RuntimeError('scripted failure')
            def GetContentText(self): return response['text']
            def GetContentBytes(self): return response['text'].encode('utf-8')
        ClientNetworkingJobs.NetworkJob = Job
        controller = session.controller
        original_thread = controller.CallToThread
        original_qt = controller.CallAfterQtSafe
        original_add = controller.network_engine.AddJob
        controller.CallToThread = lambda fn,*args,**kwargs:fn(*args,**kwargs)
        controller.CallAfterQtSafe = lambda widget,fn,*args,**kwargs:fn(*args,**kwargs)
        controller.network_engine.AddJob = lambda job:None
        try:
            formula = Formulae.EditContextVariableFormulaPanel(root,True,P.ParseFormulaContextVariable(),P.ParsingTestData(context,['']))
            formula._test_panel._SetExampleData('old text')
            ClientGUIDialogsQuick.EnterText = lambda *args,**kwargs:' https://test-docs.example/formula '
            for mode in ('success','cancel','error'):
                response['mode'] = mode
                formula._test_panel._FetchFromURL()
                data = formula._test_panel.GetTestData()
                out.append({'case':'formula_fetch','mode':mode,'request':dict(requests[-1]),'text':data.texts[0],'context':dict(data.parsing_context)})
            formula.deleteLater()
            panel._test_url.setText(' https://test-docs.example/a b ')
            panel._test_referral_url.setText('https://referral.example/from')
            # Replace only the progress control's Job callbacks, which would
            # otherwise request unrelated status methods from this boundary.
            panel._test_network_job_control.SetNetworkJob = lambda job:None
            panel._test_network_job_control.ClearNetworkJob = lambda:None
            panel._test_network_job_control.SetError = lambda error:None
            panel._test_network_job_control.ClearError = lambda:None
            for mode in ('success','cancel','error'):
                response['mode'] = mode
                panel._FetchExampleData()
                data = panel._test_panel.GetTestData()
                child = panel._test_panel.GetTestDataForChild()
                out.append({'case':'parser_fetch','mode':mode,'request':dict(requests[-1]),'text':data.texts[0],'child_texts':child.texts,'context':dict(data.parsing_context)})
            panel._test_url.setText('')
            panel._FetchExampleData()
            out.append({'case':'parser_default_url','url_field':panel._test_url.text(),'requests':len(requests)})
        finally:
            controller.CallToThread = original_thread
            controller.CallAfterQtSafe = original_qt
            controller.network_engine.AddJob = original_add
        panel.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui,qt)


def child(path):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(path,'w') as f:json.dump(result,f,indent=2,ensure_ascii=False)


def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        child(sys.argv[2]);return
    import tempfile
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp,'data.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures','parser_test_data.json'),'w') as f:
        json.dump(result,f,indent=2,ensure_ascii=False);f.write('\n')
    print('recorded',len(result),'parser/formula test-data states')


if __name__=='__main__':main()
