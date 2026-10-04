#!/usr/bin/env python3
"""Drive gallery-log exchange actions and real duplicate/continuation question dialogs."""
import json
import os
import shutil
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    c=session.controller
    def work():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client.gui.importing import ClientGUIGallerySeedLog as W
        from hydrus.client.gui import ClientGUIMenus as M, ClientGUITopLevelWindowsPanels as T, ClientGUIDialogsFiles as F, ClientGUIDialogsMessage as E
        from hydrus.client.importing import ClientImportGallerySeeds as S
        from hydrus.client import ClientSerialisable as P
        domain=c.network_engine.domain_manager
        old_classes=domain.GetURLClasses();old_pub=c.pub;old_clip=c.GetClipboardText;old_exec=T.DialogCustomButtonQuestion.exec;old_export=T.DialogNullipotent.exec;old_pick=F.FileDialog.exec;old_error=E.ShowCritical
        domain.SetURLClasses([])
        out={'cases':[],'questions':[],'objects':None,'png':{},'errors':[]}
        def snapshot(log):return [[s.url,s._can_generate_more_pages,s.status,s.note] for s in log.GetGallerySeeds()]
        def menu(log):
            m=M.GenerateMenu(c.gui);W.PopulateGallerySeedLogButton(c.gui,m,log,log.GetGallerySeeds(),False,True,'search');return m
        def trigger(log,submenu,item):
            m=next(a.menu() for a in menu(log).actions() if a.text()==submenu)
            next(a for a in m.actions() if a.text()==item).trigger()
        answers=[]
        def question_exec(dlg):
            def input():
                p=dlg._panel;answer=answers.pop(0)
                out['questions'].append({'title':dlg.windowTitle(),'text':'\n'.join(w.text() for w in p.findChildren(QW.QLabel)),'choices':[p._yes.text(),p._no.text()],'answer':answer})
                if answer=='cancel':dlg.SetCancelled(True);dlg.close()
                else:(p._yes if answer=='yes' else p._no).click()
            QC.QTimer.singleShot(0,input);return old_exec(dlg)
        T.DialogCustomButtonQuestion.exec=question_exec
        for name,text,replies in [('only-new','\ufeff https://gallery-exchange.example/a\r\n https://gallery-exchange.example/b\nhttps://gallery-exchange.example/b',['yes','yes']),('duplicates-no','https://gallery-exchange.example/a\nhttps://gallery-exchange.example/c',['no']),('duplicate-cancel','https://gallery-exchange.example/a\nhttps://gallery-exchange.example/c',['cancel']),('continue','https://gallery-exchange.example/c',['no']),('continuation-cancel','https://gallery-exchange.example/d',['cancel']),('equivalent-new','https://gallery-exchange.example/b#one\nhttps://gallery-exchange.example/b#two',['yes']),('empty','\n ',['yes']),('invalid-preserved','not a valid URL',['yes'])]:
            log=S.GallerySeedLog();first=S.GallerySeed('https://gallery-exchange.example/a');first.status=4;first.note='old failure';log.AddGallerySeeds([first]);answers[:]=replies;c.GetClipboardText=lambda text=text:text
            trigger(log,'ADVANCED: import new urls','from clipboard')
            out['cases'].append({'name':name,'text':text,'answers':replies,'seeds':snapshot(log)})
            assert not answers
        log=S.GallerySeedLog();s=S.GallerySeed('https://gallery-exchange.example/日',False);s.created=1700000000;s.modified=1700000100;s.status=4;s.note='note 日本';s._referral_url='https://gallery-exchange.example/ref';s._request_headers={'X-Synthetic':'header'};s._external_filterable_tags={'filter:tag'};s._external_additional_service_keys_to_tags[bytes([17])*32]={'extra:tag'};log.AddGallerySeeds([s]);c.pub=lambda *a,**kw:out.update(objects=a[2]) if a and a[0]=='clipboard' else None
        trigger(log,'advanced','export selected page objects to clipboard')
        with tempfile.TemporaryDirectory() as tmp:
            path=os.path.join(tmp,'gallery')
            def export_exec(dlg):
                def input():
                    p=dlg._panel;p._filepicker.SetPath(path);p._title.setText('Synthetic gallery URLs');p._width.setValue(256);p._Update();p._export.click();out['png']={'payload':P.LoadStringFromPNG(path+'.png'),'done':p._export.text(),'width':p._width.value()};shutil.copyfile(path+'.png',os.path.join(HERE,'fixtures','search_log_urls.png'));dlg.accept()
                QC.QTimer.singleShot(0,input);return old_export(dlg)
            T.DialogNullipotent.exec=export_exec;trigger(log,'export all urls','to png')
            E.ShowCritical=lambda win,title,text:out['errors'].append([title,text])
            for name,selected in [('png-cancel',None),('png-import',path+'.png'),('png-invalid',os.path.join(tmp,'bad.png'))]:
                if name=='png-invalid':
                    with open(selected,'wb') as f:f.write(b'not a png')
                def pick_exec(dlg,selected=selected):
                    def input():
                        if selected is None:dlg.reject()
                        else:dlg.selectFile(selected);dlg.accept()
                    QC.QTimer.singleShot(0,input);return old_pick(dlg)
                F.FileDialog.exec=pick_exec;target=S.GallerySeedLog();answers[:]=['no'] if name=='png-import' else []
                try:W.ImportFromPNG(c.gui,target,True)
                except Exception:pass
                out['cases'].append({'name':name,'seeds':snapshot(target)})
        domain.SetURLClasses(old_classes);c.pub=old_pub;c.GetClipboardText=old_clip;T.DialogCustomButtonQuestion.exec=old_exec;T.DialogNullipotent.exec=old_export;F.FileDialog.exec=old_pick;E.ShowCritical=old_error
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
    with open(os.path.join(HERE,'fixtures','search_log_exchange.json'),'w') as f:json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded search-log exchange actions and real questions')


if __name__=='__main__':main()
