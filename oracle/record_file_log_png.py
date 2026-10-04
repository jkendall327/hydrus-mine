#!/usr/bin/env python3
"""Drive the real PNG export panel and import picker for file-log source strings."""
import json
import os
import shutil
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    c=session.controller
    def work():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client.gui.importing import ClientGUIFileSeedCache as W
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T, ClientGUIDialogsFiles as F, ClientGUIDialogsMessage as M
        from hydrus.client.importing import ClientImportFileSeeds as S
        from hydrus.client import ClientSerialisable as P
        old_export, old_pick, old_message = T.DialogNullipotent.exec, F.FileDialog.exec, M.ShowCritical
        old_directory=c.new_options.GetNoneableString('last_png_export_dir')
        c.new_options.SetNoneableString('last_png_export_dir',None)
        out={'export':{},'imports':[],'errors':[]}
        cache=S.FileSeedCache()
        W.ImportSources(cache,['https://png-sources.example/a%20b','https://png-sources.example/日'])
        with tempfile.TemporaryDirectory() as tmp:
            path=os.path.join(tmp,'sources')
            def export_exec(dlg):
                def input():
                    p=dlg._panel
                    p._Update()
                    out['export']['initial']={'title':p._title.text(),'summary':p._payload_description.text(),'width':p._width.value(),'min':p._width.minimum(),'max':p._width.maximum(),'enabled':p._export.isEnabled(),'button':p._export.text()}
                    p._filepicker.SetPath(path);p._title.setText('Synthetic source list');p._text.setText('Shared source lines');p._width.setValue(256);p._Update()
                    out['export']['valid']={'enabled':p._export.isEnabled(),'button':p._export.text()}
                    p._export.click()
                    out['export']['done']=p._export.text()
                    out['export']['payload']=P.LoadStringFromPNG(path+'.png')
                    out['export']['remembered']=c.new_options.GetNoneableString('last_png_export_dir')==tmp
                    shutil.copyfile(path+'.png',os.path.join(HERE,'fixtures','file_log_sources.png'))
                    dlg.accept()
                QC.QTimer.singleShot(0,input)
                return old_export(dlg)
            T.DialogNullipotent.exec=export_exec
            W.ExportToPNG(c.gui,cache)
            M.ShowCritical=lambda win,title,text: out['errors'].append([title,text])
            for name,selected in [('cancel',None),('valid',path+'.png'),('malformed',os.path.join(tmp,'broken.png'))]:
                if name=='malformed':
                    with open(selected,'wb') as f:f.write(b'not a PNG')
                def pick_exec(dlg,selected=selected):
                    def input():
                        if selected is None:dlg.reject()
                        else:dlg.selectFile(selected);dlg.accept()
                    QC.QTimer.singleShot(0,input)
                    return old_pick(dlg)
                F.FileDialog.exec=pick_exec
                target=S.FileSeedCache()
                try:W.ImportFromPNG(c.gui,target)
                except Exception:pass
                out['imports'].append({'name':name,'seeds':[s.file_seed_data for s in target.GetFileSeeds()]})
        T.DialogNullipotent.exec,F.FileDialog.exec,M.ShowCritical=old_export,old_pick,old_message
        c.new_options.SetNoneableString('last_png_export_dir',old_directory)
        return out
    return c.CallBlockingToQt(c.gui,work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path=os.path.join(tmp,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures','file_log_png.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded actual PNG export and import dialogs')


if __name__=='__main__':main()
