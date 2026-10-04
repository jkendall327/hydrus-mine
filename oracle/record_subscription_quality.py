#!/usr/bin/env python3
"""Run actual saved-log quality reads and advanced menu asynchronous show/CSV actions."""
import json
import os
import sys
import tempfile
import time
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    c=session.controller
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.importing import ClientImportFileSeeds as F, ClientImportSubscriptionQuery as Q, ClientImportSubscriptions as S
    from hydrus.client.gui import ClientGUISubscriptions as W, ClientGUIDialogsMessage as M, ClientGUICore as G
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:4]]
    for action,selected,key in [(HC.CONTENT_UPDATE_INBOX,hashes,CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY),(HC.CONTENT_UPDATE_ARCHIVE,[hashes[1]],CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY),(HC.CONTENT_UPDATE_DELETE,[hashes[2]],CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY)]:
        c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdate(key,U.ContentUpdate(HC.CONTENT_TYPE_FILES,action,set(selected))))
    headers=[];containers={}
    for name,display,selected in [('synthetic query','display, name',[hashes[0],hashes[1],hashes[2],hashes[0],bytes([170])*32]),('empty',None,[])]:
        h=Q.SubscriptionQueryHeader();h.SetQueryText(name);h.SetDisplayName(display)
        log=Q.SubscriptionQueryLogContainer(h.GetQueryLogContainerName());cache=log.GetFileSeedCache()
        seeds=[]
        for i,hash in enumerate(selected):
            seed=F.FileSeed(F.FILE_SEED_TYPE_URL,f'https://quality.example/{name}/{i}');seed.SetHash(hash);seed.status=4 if i==1 else 1;seeds.append(seed)
        cache.AddFileSeeds(seeds);c.WriteSynchronous('serialisable',log);headers.append(h);containers[log.GetName()]=log
    missing=Q.SubscriptionQueryHeader();missing.SetQueryText('missing saved log');headers.append(missing)
    draft=Q.SubscriptionQueryHeader();draft.SetQueryText('unsaved query');headers.append(draft)
    out={'data':W.GetQueryHeadersQualityInfo(headers[:2]),'menu':[],'messages':[],'clipboard':[],'steps':[]}
    try:W.GetQueryHeadersQualityInfo([missing])
    except Exception as error:out['missing_log_error_type']=type(error).__name__
    old_advanced=c.new_options.GetBoolean('advanced_mode');c.new_options.SetBoolean('advanced_mode',True)
    old_info=M.ShowInformation;old_pub=c.pub;core=G.core();old_popup=core.PopupMenu
    M.ShowInformation=lambda win,message,**kw:out['messages'].append(message)
    c.pub=lambda *a,**kw:out['clipboard'].append(a[2]) if a and a[0]=='clipboard' else old_pub(*a,**kw)
    requested=['show']
    def popup(widget,menu):
        out['menu']=[a.text() for a in menu.actions()]
        next(a for a in menu.actions() if a.text()==requested[0]).trigger()
    core.PopupMenu=popup
    def setup():
        from qtpy import QtWidgets as QW
        sub=S.Subscription('quality',gug_key_and_name=(os.urandom(32),'synthetic'))
        sub.SetQueryHeaders(headers)
        panel=W.EditSubscriptionPanel(c.gui,sub,containers,set(containers)|{missing.GetQueryLogContainerName()})
        panel._query_headers.SelectDatas([h for h in panel._query_headers.GetData() if h.GetQueryText()!='missing saved log'])
        button=next(w for w in panel.findChildren(QW.QPushButton) if w.text()=='quality info')
        return panel,button
    panel,button=c.CallBlockingToQt(c.gui,setup)
    for action,key in [('show','messages'),('copy csv data to clipboard','clipboard')]:
        requested[0]=action
        def start():button.click();return not panel.isEnabled()
        out['steps'].append({'action':action,'disabled':c.CallBlockingToQt(c.gui,start)})
        deadline=time.monotonic()+5
        while not out[key]:
            if time.monotonic()>deadline:raise RuntimeError('quality worker did not finish')
            time.sleep(.01)
        out['steps'][-1]['restored']=c.CallBlockingToQt(c.gui,panel.isEnabled)
    def cleanup():
        panel.deleteLater();M.ShowInformation=old_info;c.pub=old_pub;core.PopupMenu=old_popup;c.new_options.SetBoolean('advanced_mode',old_advanced)
    c.CallBlockingToQt(c.gui,cleanup)
    return out


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
    with open(os.path.join(HERE,'fixtures','subscription_quality.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded actual subscription quality reads and async menu outputs')


if __name__=='__main__':main()
