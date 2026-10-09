#!/usr/bin/env python3
"""Record the file log's "search for URLs" on the real reference panel.

Builds the reference's EditFileSeedCachePanel over lists of URLs and of paths,
selects rows, opens its right-click menu and triggers "search for URLs",
recording the new page the reference asks for: location, predicates, name and
whether it is brought to the front.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    c=session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.importing import ClientGUIFileSeedCache as W
        from hydrus.client.importing import ClientImportFileSeeds as S
        old_pub=c.pub
        out={'cases':[]}
        U,H=S.FILE_SEED_TYPE_URL,S.FILE_SEED_TYPE_HDD
        cases=[
            ('urls',[(U,x) for x in ['https://example.com/post/1','https://example.com/post/%E6%97%A5%E6%9C%AC 2','https://example.com/post/3']],[0,1,2]),
            ('some selected',[(U,x) for x in ['https://example.com/post/1','https://example.com/post/2','https://example.com/post/3']],[2,0]),
            ('one selected',[(U,x) for x in ['https://example.com/post/1','https://example.com/post/2']],[1]),
            ('paths',[(H,x) for x in ['/synthetic/a.jpg','/synthetic/b.jpg']],[0,1]),
            ('non-http url among http',[(U,'ftp://example.com/a'),(U,'https://example.com/post/9')],[0,1]),
            ('only a non-http url',[(U,'ftp://example.com/a'),(U,'https://example.com/post/9')],[0]),
            ('url first, then a path',[(U,'https://example.com/post/1'),(H,'/synthetic/a.jpg')],[0,1]),
            ('path first, then a url',[(H,'/synthetic/a.jpg'),(U,'https://example.com/post/1')],[0,1]),
        ]
        for name,items,selected in cases:
            sources=[x for _,x in items];kinds=['url' if k==U else 'path' for k,_ in items]
            cache=S.FileSeedCache();cache.AddFileSeeds([S.FileSeed(k,x) for k,x in items])
            panel=W.EditFileSeedCachePanel(c.gui,cache)
            seeds=cache.GetFileSeeds()
            lc=panel._list_ctrl
            lc.clearSelection()
            for i in selected:
                lc.SelectDatas([seeds[i]])
            QW.QApplication.processEvents()
            pubs=[]
            def pub(*a,**k):
                if not a or a[0]!='new_page_query':return
                from hydrus.client.search import ClientSearchPredicate
                OR=ClientSearchPredicate.PREDICATE_TYPE_OR_CONTAINER
                preds=[]
                for p in k.get('initial_predicates',[]):
                    inner=[[q.ToString(),str(q.GetValue())] for q in p.GetValue()] if p.GetType()==OR else None
                    preds.append({'text':p.ToString(),'type':p.GetType(),'inner':inner})
                pubs.append({'location':sorted(x.hex() for x in a[1].current_service_keys),'predicates':preds,'page_name':k.get('page_name'),'activate_window':k.get('activate_window')})
            c.pub=pub
            menu=panel._GetListCtrlMenu()
            entry=next((a for a in menu.actions() if a.text()=='search for URLs'),None)
            record_={'name':name,'sources':sources,'kinds':kinds,'selected':selected,'offered':entry is not None,'pubs':pubs}
            if entry is not None:entry.trigger()
            QW.QApplication.processEvents()
            out['cases'].append(record_)
            c.pub=old_pub;panel.deleteLater()
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
    with open(os.path.join(HERE,'fixtures','file_log_search_urls.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded file log search urls')
if __name__=='__main__':main()
