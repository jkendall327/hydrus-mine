#!/usr/bin/env python3
"""Record actual duplicate and collapse actions on loaded ordered media.

Uses overlapping search pages inside a notebook, verifies one-shot confirmation,
retained closed indices, nested flattening, group deduplication and copied tree
selection. Importers are covered independently by session_importers.json.
"""
import json
import sys
import tempfile
import time
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.pages import ClientGUIPages
    controller,gui=session.controller,session.controller.gui
    manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files'][:4]]
    location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    qt=lambda f:controller.CallBlockingToQt(gui,f)
    old_yesno=ClientGUIDialogsQuick.GetYesNo
    answer,asked=[False],[]
    def yesno(window,message,**kwargs):
        asked.append(message)
        return QW.QDialog.DialogCode.Accepted if answer[0] else QW.QDialog.DialogCode.Rejected
    ClientGUIDialogsQuick.GetYesNo=yesno
    def build(single=False):
        notebook=ClientGUIPages.PagesNotebook(gui,'harvest')
        child=notebook.NewPagesNotebook(name='nested',give_it_a_blank_page=False)
        child.NewPageQuery(location,initial_hashes=[hashes[2],hashes[0]],page_name='child 0')
        child.NewPageQuery(location,initial_hashes=[hashes[1],hashes[2]],page_name='child 1')
        child.setCurrentIndex(1)
        if not single:
            notebook.NewPageQuery(location,initial_hashes=[hashes[3],hashes[0]],page_name='other')
            notebook.NewPageQuery(location,initial_hashes=[hashes[1]],page_name='last')
        notebook.setCurrentIndex(0)
        return notebook
    def facts(notebook):
        def tree(node):
            out=[]
            for p in node.GetPages():
                row={'name':p.GetName()}
                if isinstance(p,ClientGUIPages.PagesNotebook):row['children']=tree(p)
                else:row['hashes']=[h.hex() for h in p.GetHashes()]
                out.append(row)
            return out
        shown=notebook.GetCurrentMediaPage()
        return {'tree':tree(notebook),'shown':shown.GetName() if shown else None,
            'closed_indices':[i for i,key in notebook._closed_pages]}
    def wait_loaded(notebook, expected):
        for _ in range(400):
            if qt(lambda:sum(len(p.GetHashes()) for p in notebook.GetMediaPages()))==expected:return
            time.sleep(.025)
        raise RuntimeError('reference media did not load')
    try:
        collapsed=[]
        for scope,index in [('this',0),('from_here',0),('right',1),('only',0)]:
            for accept in [False,True]:
                notebook=qt(lambda:build(scope=='only'))
                wait_loaded(notebook,4 if scope=='only' else 7)
                before=qt(lambda:facts(notebook))
                answer[0]=accept;asked.clear()
                method='_CollapsePage' if scope in ['this','only'] else '_CollapsePagesToTheRight'
                qt(lambda:getattr(notebook,method)(index))
                time.sleep(.25)
                after=qt(lambda:facts(notebook))
                collapsed.append({'scope':scope,'index':index,'accepted':accept,'asked':list(asked),'before':before,'after':after})
        duplicated=[]
        for index in [0,1]:
            notebook=qt(build);wait_loaded(notebook,7)
            before=qt(lambda:facts(notebook))
            qt(lambda:notebook._DuplicatePage(index))
            time.sleep(.25)
            duplicated.append({'index':index,'before':before,'after':qt(lambda:facts(notebook))})
        result={'hashes':[h.hex() for h in hashes],'collapse':collapsed,'duplicate':duplicated}
        time.sleep(1)
        return result
    finally:ClientGUIDialogsQuick.GetYesNo=old_yesno


def main():
    import hydrus_driver,record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        destination=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        destination.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out=Path(work)/'harvest.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out))
        result=json.loads(out.read_text())
    (HERE/'fixtures/tab_harvest.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()
