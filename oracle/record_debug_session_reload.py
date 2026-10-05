#!/usr/bin/env python3
"""Record real Help reload QAction, asynchronous save/load, selection and cleanup.

No scheduler, worker, save, clock or load method is replaced. Scripted questions
only record unexpected prompts; the actual existing file corpus seeds two nested
notebooks. Merely hidden Main still receives admitted worker completion.
"""
import json, sys, tempfile, time
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
 from qtpy import QtTest as T, QtWidgets as W
 from hydrus.client import ClientConstants as CC,ClientLocation
 from hydrus.client.gui import ClientGUIDialogsQuick
 from hydrus.client.gui.pages import ClientGUIPages
 from hydrus.client.media import ClientMediaFileFilter
 from hydrus.core import HydrusSerialisable
 c=session.controller;g=c.gui
 media=c.Read('media_results_from_ids',list(range(1,41)))
 hashes=[m.GetHash() for m in media[:3]]
 def drive():
  root=g._notebook;location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
  while root.count():root.CloseCurrentPage(polite=False)
  first=root.NewPageQuery(location,initial_hashes=hashes[:2],page_name='reload first')
  nest=root.NewPagesNotebook(name='reload notebook',give_it_a_blank_page=False)
  nest.NewPageQuery(location,initial_hashes=hashes[1:],page_name='reload inner first')
  last=nest.NewPageQuery(location,initial_hashes=[hashes[2],hashes[0]],page_name='reload inner selected')
  deadline=time.monotonic()+15
  while not all(p._initialised for p in root.GetMediaPages()):
   if time.monotonic()>deadline:raise RuntimeError('initial pages never loaded')
   T.QTest.qWait(50)
  last.GetMediaResultsPanel()._Select(ClientMediaFileFilter.FileFilter(ClientMediaFileFilter.FILE_FILTER_ALL))
  def snapshot(node):
   out=[]
   for p in node.GetPages():
    row=dict(name=p.GetName(),key=p.GetPageKey().hex(),selected=node.currentWidget() is p)
    if isinstance(p,ClientGUIPages.PagesNotebook):row['children']=snapshot(p)
    else:
     row['hashes']=[h.hex() for h in p.GetHashes()]
     row['selected_hashes']=[m.GetHash().hex() for m in p.GetMediaResultsPanel()._GetSelectedFlatMedia()]
    out.append(row)
   return out
  closed_before=len(g._closed_pages)
  before=snapshot(root)
  assert len(before[1]['children'][1]['selected_hashes'])==2
  old_keys={p.GetPageKey() for p in root.GetMediaPages()};questions=[]
  old=ClientGUIDialogsQuick.GetYesNo
  def yesno(win,message,**kw):questions.append(message);return W.QDialog.DialogCode.Rejected
  ClientGUIDialogsQuick.GetYesNo=yesno
  menu,label=g._InitialiseMenuInfoHelp()
  def find(menu,labels):
   for a in menu.actions():
    if a.text()==labels[0]:return a if len(labels)==1 else find(a.menu(),labels[1:])
   raise ValueError(labels)
  try:
   action=find(menu,['debug','gui actions','close and reload current gui session'])
   start=time.monotonic();action.trigger();g.hide()
   deadline=start+20
   while time.monotonic()<deadline:
    T.QTest.qWait(50)
    pages=root.GetMediaPages()
    if len(pages)==3 and all(p.GetPageKey() not in old_keys for p in pages):break
   else:raise RuntimeError('real reload did not deliver')
   T.QTest.qWait(700)
   after=snapshot(root);shown=g.GetCurrentPage().GetName()
   names=c.Read('serialisable_names',HydrusSerialisable.SERIALISABLE_TYPE_GUI_SESSION_CONTAINER)
   result=dict(menu_path=['help','debug','gui actions',action.text()],before=before,after=after,shown=shown,questions=questions,
    elapsed_ms=round((time.monotonic()-start)*1000,3),visible_at_delivery=g.isVisible(),temporary_slot_present='temp_session_slot_for_reload_if_you_see_this_you_can_delete_it' in names,
    closed_added=[p.GetName() for _,p in g._closed_pages[closed_before:]],
    clock='actual wall clock and controller worker/Qt delivery')
   g.showNormal();T.QTest.qWait(100);g.grab().save(str(HERE/'fixtures/debug_session_reload.png'))
   return result
  finally:ClientGUIDialogsQuick.GetYesNo=old;menu.deleteLater();g.showNormal()
 return c.CallBlockingToQt(g,drive)

def main():
 import hydrus_driver,record_api
 if len(sys.argv)>1 and sys.argv[1]=='--child':
  destination=Path(sys.argv[2])
  result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
  destination.write_text(json.dumps(result));return
 with tempfile.TemporaryDirectory() as d:
  out=Path(d)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
 (HERE/'fixtures/debug_session_reload.json').write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()
