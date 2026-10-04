#!/usr/bin/env python3
"""Record selected import-object export and actual cancel/accept renormalisation."""
import json
import os
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    c=session.controller
    def work():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client.gui.importing import ClientGUIFileSeedCache as W
        from hydrus.client.gui import ClientGUIMenus as M, ClientGUITopLevelWindowsPanels as T
        from hydrus.client.importing import ClientImportFileSeeds as S
        from hydrus.client.networking import ClientNetworkingURLClass as U
        from hydrus.client import ClientStrings as CS
        domain=c.network_engine.domain_manager
        old_classes=domain.GetURLClasses(); old_pub=c.pub;old_exec=T.DialogCustomButtonQuestion.exec
        domain.SetURLClasses([])
        out={'clipboard':[],'questions':[],'states':[]}
        c.pub=lambda *a,**kw:out['clipboard'].append(a[2]) if a and a[0]=='clipboard' else None
        cache=S.FileSeedCache()
        urls=['https://renormalise.example/post?id=1&token=a','https://renormalise.example/post?id=1&token=b','https://renormalise.example/post?id=2&token=c']
        seeds=[S.FileSeed(S.FILE_SEED_TYPE_URL,url) for url in urls]
        for i,s in enumerate(seeds):
            s.created=1700000000+i;s.modified=1700000100+i;s.source_time=1699990000+i
            s.status=4 if i==0 else 1;s.note=f'entry {i} 日本';s.SetReferralURL('https://renormalise.example/gallery')
            s._request_headers={'X-Synthetic':'header'};s._external_filterable_tags={'filter:tag'};s._external_additional_service_keys_to_tags[bytes([17])*32]={'extra:tag'}
            s._primary_urls={'https://renormalise.example/primary'};s._source_urls={'https://source.example/a'};s._tags={'tag:one'};s._names_and_notes_dict={'note':'metadata note'};s.SetHash(bytes([i+1])*32)
        cache.AddFileSeeds(seeds)
        def menu():
            m=M.GenerateMenu(c.gui);W.PopulateFileSeedCacheMenu(c.gui,m,cache,seeds[:2]);return next(a.menu() for a in m.actions() if a.text()=='advanced')
        next(a for a in menu().actions() if a.text()=='export selected import objects to clipboard').trigger()
        def state():return [[s.file_seed_data,s.file_seed_data_for_comparison,s.status,s.created,s.modified,s.note] for s in cache.GetFileSeeds()]
        out['states'].append(state())
        cls=U.URLClass('synthetic changed class',url_domain_mask=U.URLDomainMask(raw_domains=['renormalise.example']),path_components=[(CS.StringMatch(match_type=CS.STRING_MATCH_FIXED,match_value='post'),None)],parameters=[U.URLClassParameterFixedName('id',CS.StringMatch())],example_url=urls[0])
        cls.SetKeepExtraParametersForServer(False);domain.SetURLClasses([cls])
        for yes in [False,True]:
            def execute(dlg,yes=yes):
                def answer():
                    p=dlg._panel;out['questions'].append({'title':dlg.windowTitle(),'text':'\n'.join(w.text() for w in p.findChildren(QW.QLabel)),'yes':p._yes.text(),'no':p._no.text()})
                    (p._yes if yes else p._no).click()
                QC.QTimer.singleShot(0,answer);return old_exec(dlg)
            T.DialogCustomButtonQuestion.exec=execute
            next(a for a in menu().actions() if a.text()=='re-normalise all URLs').trigger()
            out['states'].append(state())
        domain.SetURLClasses(old_classes);c.pub=old_pub;T.DialogCustomButtonQuestion.exec=old_exec
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
    with open(os.path.join(HERE,'fixtures','file_log_advanced.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded file-log advanced export and confirmation')


if __name__=='__main__':main()
