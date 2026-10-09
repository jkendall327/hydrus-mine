#!/usr/bin/env python3
"""Record the file log's whole-log menu actions on the real reference cache.

Builds one import list holding every status, then for each menu entry (from
the reference's own PopulateFileSeedCacheMenu) triggers it with the question
answered no, then yes (a retry-ignored choice by its button), recording the
questions asked, the list afterwards and the files shown in a new page.
"""
import json, os, sys, tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    c=session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui.importing import ClientGUIFileSeedCache as W
        from hydrus.client.gui import ClientGUIMenus as M, ClientGUIDialogsQuick as Q
        from hydrus.client.importing import ClientImportFileSeeds as S
        from hydrus.client import ClientLocation
        from hydrus.client.media import ClientMediaSort
        from hydrus.client.search import ClientSearchPredicate, ClientSearchFileSearchContext
        old_yesno=Q.GetYesNo;old_select=Q.SelectFromListButtons;old_pub=c.pub
        plan=[(CC.STATUS_SUCCESSFUL_AND_NEW,''),(CC.STATUS_SUCCESSFUL_BUT_REDUNDANT,''),(CC.STATUS_DELETED,'Previously deleted'),
              (CC.STATUS_ERROR,'Error: 404'),(CC.STATUS_ERROR,'Error: 500'),(CC.STATUS_VETOED,'403 forbidden'),(CC.STATUS_VETOED,'404 not found'),
              (CC.STATUS_VETOED,'veto: blacklisted!'),(CC.STATUS_SKIPPED,''),(CC.STATUS_UNKNOWN,''),(CC.STATUS_UNKNOWN,'')]
        location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        everything=ClientSearchPredicate.Predicate(ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_EVERYTHING)
        context=ClientSearchFileSearchContext.FileSearchContext(location_context=location,predicates=[everything])
        sort=ClientMediaSort.MediaSort(sort_type=('system',CC.SORT_FILES_BY_HASH),sort_order=CC.SORT_ASC)
        ids=c.Read('file_query_ids',context,limit_sort_by=sort)
        real=sorted(c.Read('hash_ids_to_hashes',ids).values())[:2]
        def build():
            seeds=[]
            for i,(status,note) in enumerate(plan):
                s=S.FileSeed(S.FILE_SEED_TYPE_URL,f'https://example.com/post/{i+1}')
                if status!=CC.STATUS_UNKNOWN:
                    s.SetStatus(status,note=note)
                if status in (CC.STATUS_SUCCESSFUL_AND_NEW,CC.STATUS_SUCCESSFUL_BUT_REDUNDANT):
                    s.SetHash(real[i])
                seeds.append(s)
            cache=S.FileSeedCache();cache.AddFileSeeds(seeds);return cache
        def state(cache):return [[s.file_seed_data,s.status,s.note] for s in cache.GetFileSeeds()]
        def menu(cache):
            m=M.GenerateMenu(c.gui);W.PopulateFileSeedCacheMenu(c.gui,m,cache,[]);return m
        def labels(m):
            out=[]
            for a in m.actions():
                if a.isSeparator():continue
                out.append(a.text())
            return out
        base=build();out={'hashes':[h.hex() for h in real],'start':state(base),'menu':labels(menu(base)),'actions':[]}
        skip=('to clipboard','to png','from clipboard','from png','export all','ADVANCED','advanced')
        wanted=[t for t in out['menu'] if not t.startswith(skip) and not t.startswith('ADVANCED')]
        for text in wanted:
            for answer in ('no','yes') if not text.startswith('retry') or 'ignored' not in text else ('cancel','retry all','retry 403s','retry 404s','retry blacklisted'):
                cache=build();m=menu(cache);asked=[];pubs=[]
                def yesno(win,message,*a,**k):
                    asked.append({'kind':'yesno','text':message})
                    return QW.QDialog.DialogCode.Accepted if answer=='yes' else QW.QDialog.DialogCode.Rejected
                def select(win,message,choices,*a,**k):
                    asked.append({'kind':'choices','text':message,'buttons':[x[0] for x in choices]})
                    if answer=='cancel':
                        from hydrus.core import HydrusExceptions;raise HydrusExceptions.CancelledException()
                    return next(x[1] for x in choices if x[0]==answer)
                Q.GetYesNo=yesno;Q.SelectFromListButtons=select
                c.pub=lambda *a,**k:pubs.append([a[0],[h.hex() for h in sorted(k.get('initial_hashes',[]))]]) if a and a[0]=='new_page_query' else None
                next(a for a in m.actions() if a.text()==text).trigger()
                QW.QApplication.processEvents()
                out['actions'].append({'label':text,'answer':answer,'asked':asked,'after':state(cache),'pages':pubs})
        Q.GetYesNo=old_yesno;Q.SelectFromListButtons=old_select;c.pub=old_pub
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
    with open(os.path.join(HERE,'fixtures','file_log_effects.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded file log effects')
if __name__=='__main__':main()
