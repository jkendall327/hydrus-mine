#!/usr/bin/env python3
"""Record the system:urls exact-url, domain and url-class panels on the real reference.

Builds each panel (PanelPredicateSystemKnownURLs*) as the search page does,
types a range of texts (including empty and padded ones, which the reference
does not refuse), and with the client's URL classes (some not going with files)
records the classes offered, the default, and what each has / does not have
choice makes.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    c=session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.search import ClientGUIPredicatesSingle as P
        from hydrus.client.search import ClientSearchPredicate as SP
        from hydrus.client.networking import ClientNetworkingURLClass as U
        manager=c.network_engine.domain_manager
        old=manager.GetURLClasses()
        def one(name,files,key):
            k=U.URLClass(name=name);k.SetClassKey(bytes([key])*32)
            k.SetURLBooleans(False,False,files,False);return k
        manager.SetURLClasses([one('posts a',True,201),one('not for files',False,202),one('posts b',True,203)])
        parent=QW.QWidget()
        pred=lambda kind,rule,desc='':SP.Predicate(SP.PREDICATE_TYPE_SYSTEM_KNOWN_URLS,(True,kind,rule,desc))
        def made(panel):
            out=[]
            for p in panel.GetPredicates():
                value=p.GetValue()
                out.append({'text':p.ToString(),'operator':value[0],'rule_type':value[1],'rule':value[2] if isinstance(value[2],str) else value[2].GetName(),'description':value[3]})
            return out
        out={'exact':[],'domain':[]}
        texts=['','x','  padded  ','https://Example.com/a b?c=d#frag','EXAMPLE.com','http://[bad']
        for kind,key,cls,line in (('exact_match','exact',P.PanelPredicateSystemKnownURLsExactURL,'_exact_url'),('domain','domain',P.PanelPredicateSystemKnownURLsDomain,'_domain')):
            for text in texts:
                for operator in (0,1):
                    panel=cls(parent,pred(kind,''))
                    getattr(panel,line).setText(text);panel._operator.setCurrentIndex(operator)
                    try:
                        panel.CheckValid();valid=True
                    except Exception as e:valid=str(e)
                    try:out[key].append({'text':text,'operator_index':operator,'valid':valid,'made':made(panel)})
                    except Exception as e:out[key].append({'text':text,'operator_index':operator,'error':str(e)})
                    panel.deleteLater()
            panel=cls(parent,pred(kind,''))
            out[key+'_default']={'placeholder':getattr(panel,line).placeholderText(),'operators':[panel._operator.itemText(i) for i in range(panel._operator.count())],'default':[(v if isinstance(v,(bool,str)) else str(v)) for v in panel.GetDefaultPredicate().GetValue()]}
        panel=P.PanelPredicateSystemKnownURLsURLClass(parent,pred('url_class',U.URLClass(name='posts b')))
        out['classes_offered']=[panel._url_classes.itemText(i) for i in range(panel._url_classes.count())]
        out['class_initial']=panel._url_classes.GetValue().GetName()
        out['class_default']=panel.GetDefaultPredicate().GetValue()[2].GetName()
        out['class_operators']=[panel._operator.itemText(i) for i in range(panel._operator.count())]
        out['classes']=[]
        for index in range(panel._url_classes.count()):
            for operator in (0,1):
                panel._url_classes.setCurrentIndex(index);panel._operator.setCurrentIndex(operator)
                out['classes'].append({'index':index,'operator_index':operator,'made':made(panel)})
        manager.SetURLClasses([one('not for files',False,202)])
        panel=P.PanelPredicateSystemKnownURLsURLClass(parent,pred('url_class',U.URLClass(name='posts b')))
        none={'offered':[panel._url_classes.itemText(i) for i in range(panel._url_classes.count())],'value_is_none':panel._url_classes.GetValue() is None}
        try:
            panel.CheckValid();none['valid']=True
        except Exception as e:none['valid']=str(e)
        try:
            panel.GetPredicates();none['made']=True
        except Exception as e:none['error']=type(e).__name__
        out['no_classes_for_files']=none
        manager.SetURLClasses(old)
        return out
    return c.CallBlockingToQt(c.gui,work)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2];result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path=os.path.join(tmp,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures','known_urls_panels.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded known urls panels')
if __name__=='__main__':main()
