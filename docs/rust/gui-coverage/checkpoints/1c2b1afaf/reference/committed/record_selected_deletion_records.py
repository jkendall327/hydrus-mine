#!/usr/bin/env python3
"""Record genuine old/default thumbnail deletion-record menu and accepted writes.

Copied basic DB only. Real menus, selected flat media, question helper and scoped
content-update consumer are used. Explicit answers and synchronous writer transport
are controlled; synthetic MediaSingle objects additionally prove 64-record batches
and a later-batch exception without pretending those hashes exist in the fixture.
"""
import hashlib,json,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
OUT=HERE/'fixtures/selected_deletion_records.json'

def record(session):
 from qtpy import QtWidgets as QW
 from hydrus.client import ClientConstants as CC,ClientLocation
 from hydrus.client.gui import ClientGUIDialogsQuick as Q
 from hydrus.client.gui.media import ClientGUIMediaModalActions as A
 from hydrus.client.gui.pages import ClientGUIPageManager as PM,ClientGUIMediaResultsPanelThumbnails as T
 from hydrus.client.media import ClientMediaFileFilter as F,ClientMediaSingle as M,ClientMediaList as ML,ClientMediaCollect as MC
 from hydrus.client.search import ClientSearchFileSearchContext as S
 from hydrus.client.metadata import ClientContentUpdates as U
 from hydrus.core import HydrusConstants as HC
 c=session.controller
 hashes=[bytes.fromhex(r['hash']) for r in json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())['files'][:4]]
 c.new_options.SetBoolean('delete_lock_for_archived_files',False)
 originals=c.Read('media_results',hashes)
 def update(key,action,row):c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdate(key,U.ContentUpdate(HC.CONTENT_TYPE_FILES,action,row)))
 def reset():
  for result in originals:update(CC.LOCAL_FILE_SERVICE_KEY,HC.CONTENT_UPDATE_ADD,(result.GetFileInfoManager(),1700000000000))
  update(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY,HC.CONTENT_UPDATE_DELETE,hashes[:3])
  update(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY,HC.CONTENT_UPDATE_DELETE,hashes[:2])
 def state():
  results=c.Read('media_results',hashes)
  return dict(storage_deleted=[CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY in m.GetLocationsManager().GetDeleted() for m in results],trash=[CC.TRASH_SERVICE_KEY in m.GetLocationsManager().GetCurrent() for m in results],status=[c.Read('hash_status','sha256',h).status for h in hashes])
 def work():
  old_question,old_write=Q.GetYesNo,c.Write
  panels=[];out=dict(corpus=[h.hex() for h in hashes],menus=[],actions=[],batches=[])
  calls=[];questions=[];yes=[False];during=[None];transport=[True];fail_at=[None]
  def question(parent,message,**kw):
   questions.append(dict(message=message,kwargs=kw))
   if during[0]:during[0]()
   return QW.QDialog.DialogCode.Accepted if yes[0] else QW.QDialog.DialogCode.Rejected
  def write(action,package,*args,**kw):
   batches=[]
   for key,updates in package.IterateContentUpdates():
    for u in updates:batches.append(dict(service=key.hex(),action=u.GetAction(),hashes=[h.hex() for h in u.GetRow()]))
   if fail_at[0] is not None and len(calls)==fail_at[0]:raise RuntimeError('scripted later batch failure')
   calls.extend(batches)
   if transport[0]:return c.WriteSynchronous(action,package,*args,**kw)
  Q.GetYesNo=question;c.Write=write
  try:
   for new,cls in [(False,T.MediaResultsPanelThumbnails),(True,T.MediaResultsPanelThumbnailsGraphicsViewTest)]:
    reset();context=S.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_FILE_SERVICE_KEY))
    manager=PM.CreatePageManagerQuery('deletion record fixture',context)
    panel=cls(c.gui,hashlib.sha256(str(new).encode()).digest(),manager,c.Read('media_results',hashes));panels.append(panel)
    for case,indices in [('none',[]),('one_deleted',[0]),('two_deleted',[0,1]),('mixed',[0,2,3]),('trash',[2]),('current',[3])]:
     panel._Select(F.FileFilter(F.FILE_FILTER_NONE))
     media=list(panel._sorted_media)
     selected=[m for m in media if m.GetHash() in [hashes[i] for i in indices]]
     for i,m in enumerate(selected):panel._HitMedia(m,i>0,False)
     menu=panel.GetMenu();labels=[a.text() for a in menu.actions() if 'clear deletion record' in a.text()]
     out['menus'].append(dict(new_renderer=new,case=case,selected_count=panel._GetNumSelected(),labels=labels))
     if new and case=='two_deleted':
      menu.grab().save(str(OUT.with_name('selected_deletion_records_qt.png')))
     menu.deleteLater()
    # A real retained QAction reads the panel selection when triggered, not
    # the singular selection used when constructing that QAction.
    panel._Select(F.FileFilter(F.FILE_FILTER_NONE))
    media=list(panel._sorted_media)
    panel._HitMedia(next(m for m in media if m.GetHash()==hashes[0]),False,False)
    menu=panel.GetMenu();action=next(a for a in menu.actions() if 'clear deletion record' in a.text())
    panel._HitMedia(next(m for m in media if m.GetHash()==hashes[1]),True,False)
    calls.clear();questions.clear();yes[0]=False;action.trigger()
    out.setdefault('dispatch',[]).append(dict(new_renderer=new,label=action.text(),selected_count=panel._GetNumSelected(),questions=list(questions),writes=list(calls)))
    menu.deleteLater()
    panel.Collect(MC.MediaCollect(namespaces=['parity-unmatched-collection'],collect_unmatched=True))
    collection=next(m for m in panel._sorted_media if m.IsCollection())
    panel._Select(F.FileFilter(F.FILE_FILTER_NONE));panel._HitMedia(collection,False,False)
    menu=panel.GetMenu();out['menus'].append(dict(new_renderer=new,case='collection',selected_count=panel._GetNumSelected(),labels=[a.text() for a in menu.actions() if 'clear deletion record' in a.text()]))
    calls.clear();questions.clear();yes[0]=False;panel._ClearDeleteRecord()
    out.setdefault('collections',[]).append(dict(new_renderer=new,flat=[m.GetHash().hex() for m in ML.FlattenMedia([collection])],questions=list(questions),writes=list(calls)))
    menu.deleteLater();panel.close()
   # Direct genuine media action uses the same flattening as both panels.
   for case,accepted,indices in [('none',True,[]),('cancel',False,[0,1,2,3]),('one',True,[0,2,3]),('mixed',True,[0,1,2,3]),('readd_during_question',True,[0,1]),('new_delete_during_question',True,[0,3])]:
    reset();calls.clear();questions.clear();yes[0]=accepted;before=state()
    media=[M.MediaSingle(r) for r in c.Read('media_results',[hashes[i] for i in indices])]
    during[0]=(lambda:update(CC.LOCAL_FILE_SERVICE_KEY,HC.CONTENT_UPDATE_ADD,(originals[0].GetFileInfoManager(),1700000000100))) if case=='readd_during_question' else (lambda:(update(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY,HC.CONTENT_UPDATE_DELETE,[hashes[3]]),update(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY,HC.CONTENT_UPDATE_DELETE,[hashes[3]]))) if case=='new_delete_during_question' else None
    A.ClearDeleteRecord(c.gui,media)
    out['actions'].append(dict(case=case,accepted=accepted,selected=indices,before=before,questions=list(questions),writes=list(calls),after=state()))
   during[0]=None;transport[0]=False;yes[0]=True
   reset();seed=c.Read('media_results',[hashes[0]])[0]
   for count,failure in [(64,None),(65,None),(130,None),(130,1)]:
    calls.clear();questions.clear();fail_at[0]=failure;medias=[]
    for i in range(count):
     result=seed.Duplicate();result.GetFileInfoManager().hash=hashlib.sha256(f'deletion-{i}'.encode()).digest();medias.append(M.MediaSingle(result))
    error=None
    try:A.ClearDeleteRecord(c.gui,medias)
    except RuntimeError as e:error=str(e)
    out['batches'].append(dict(count=count,failure_after=failure,questions=list(questions),sizes=[len(w['hashes']) for w in calls],hashes=[w['hashes'] for w in calls],error=error))
   return out
  finally:
   Q.GetYesNo,c.Write=old_question,old_write
   for p in panels:p.close();p.deleteLater()
 return c.CallBlockingToQt(c.gui,work)

def main():
 import hydrus_driver,record_api
 if len(sys.argv)>1 and sys.argv[1]=='--child':
  Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
 with tempfile.TemporaryDirectory() as directory:
  result=Path(directory)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(result));data=json.loads(result.read_text())
 OUT.write_text(json.dumps(data,indent=2)+'\n');print('wrote',OUT)
if __name__=='__main__':main()
