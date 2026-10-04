#!/usr/bin/env python3
"""Record real downloader-display list edits, tri-state questions and viewer URL filtering."""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui import ClientGUIDownloaders as G, ClientGUIDialogsQuick as Q
        from hydrus.client.networking import ClientNetworkingGUG as N, ClientNetworkingURLClass as U
        parent = QW.QWidget()
        gugs = [N.GalleryURLGenerator(name, gug_key=bytes([key])*32, url_template='https://example.com/search?q=%tags%', replacement_phrase='%tags%') for name,key in [('alpha',17),('beta',34)]]
        classes = [U.URLClass(name, url_class_key=bytes([key])*32, url_type=HC.URL_TYPE_POST, path_components=[], parameters=[], url_domain_mask=U.URLDomainMask(raw_domains=domains)) for name,key,domains in [('example post',51,['example.com']),('multi post',68,['a.example','b.example'])]]
        engine=session.controller.network_engine
        manager=engine.domain_manager
        manager.SetGUGs(gugs)
        manager.SetURLClasses(classes)
        p=G.EditDownloaderDisplayPanel(parent,engine,gugs,{gugs[0].GetGUGKey()},classes,{c.GetClassKey() for c in classes},True)
        def rows():
            return {'gugs':[p._ConvertGUGDisplayDataToDisplayTuple(d) for d in p._gug_display_list_ctrl.GetData()], 'classes':[p._ConvertURLDisplayDataToDisplayTuple(d) for d in p._url_display_list_ctrl.GetData()]}
        out={'initial':rows(),'questions':[],'steps':[]}
        original=Q.GetYesNo
        answer=[QW.QDialog.DialogCode.Rejected,True]
        def ask(parent,message,*args,**kwargs):
            out['questions'].append({'message':message,'title':kwargs.get('title'),'cancel':kwargs.get('check_for_cancelled')})
            return tuple(answer)
        Q.GetYesNo=ask
        try:
            for kind,yes,cancel in [('gugs',False,True),('gugs',True,False),('gugs',False,False),('classes',False,True),('classes',False,False),('classes',True,False)]:
                ctrl=p._gug_display_list_ctrl if kind=='gugs' else p._url_display_list_ctrl
                ctrl.SelectDatas(ctrl.GetData())
                answer[:]=[QW.QDialog.DialogCode.Accepted if yes else QW.QDialog.DialogCode.Rejected,cancel]
                (p._EditGUGDisplay if kind=='gugs' else p._EditURLDisplay)()
                out['steps'].append(rows())
        finally: Q.GetYesNo=original
        urls=['https://other.example/item','https://example.com/','https://a.example/','invalid','https://b.example/']
        out['urls']=urls
        out['viewer']=[]
        for visible,unmatched in [([classes[0]],True),([classes[1]],True),([],False)]:
            manager.SetURLClassKeysToDisplay({c.GetClassKey() for c in visible})
            session.controller.new_options.SetBoolean('show_unmatched_urls_in_media_viewer',unmatched)
            out['viewer'].append(manager.ConvertURLsToMediaViewerTuples(urls))
        manager.SetURLClassKeysToDisplay({classes[0].GetClassKey()})
        session.controller.new_options.SetBoolean('show_unmatched_urls_in_media_viewer',True)
        cap_urls=['https://other.example/first']+['https://example.com/?id='+str(i) for i in range(12)]+['https://other.example/last']
        out['cap_urls']=cap_urls
        out['cap']=manager.ConvertURLsToMediaViewerTuples(cap_urls)
        p._show_unmatched_urls_in_media_viewer.setChecked(False)
        g,c,u=p.GetValue()
        out['value']={'gugs':sorted(k.hex() for k in g),'classes':sorted(k.hex() for k in c),'unmatched':u}
        parent.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui,qt)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f: json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path=os.path.join(tmp,'display.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f: result=json.load(f)
    with open(os.path.join(HERE,'fixtures','downloader_display.json'),'w') as f:
        json.dump(result,f,indent=2); f.write('\n')
    print('recorded downloader display questions and media URL consumers')
if __name__=='__main__': main()
