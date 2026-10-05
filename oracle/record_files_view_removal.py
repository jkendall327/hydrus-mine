#!/usr/bin/env python3
"""Real FilesAndTrash controls, filter-close signal and MediaList content updates.

Temporary basic client and detached duplicated media only. CommitArchiveDelete's
worker is captured at its publication boundary; real filter UserOKToClose emits
its actual removal signal. Real media managers consume authored delete/migration
updates before real MediaList pruning. No physical migration or network calls.
"""
import itertools,json,os,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
def record(session):
 def qt():
  from qtpy import QtCore as QC, QtWidgets as QW
  from hydrus.core import HydrusConstants as HC
  from hydrus.client import ClientConstants as CC,ClientLocation
  from hydrus.client.gui.panels.options.FilesAndTrashPanel import FilesAndTrashPanel
  from hydrus.client.gui.canvas import ClientGUICanvas as Canvas,ClientGUICanvasFrame
  from hydrus.client.gui.panels import ClientGUIScrolledPanelsCommitFiltering as Finish
  from hydrus.client.media import ClientMediaList
  from hydrus.client.metadata import ClientContentUpdates as U
  c=session.controller
  old_options=c.new_options;c.new_options=old_options.Duplicate()
  legacy_before={k:HC.options[k] for k in ('remove_filtered_files','remove_trashed_files')}
  panel=FilesAndTrashPanel(c.gui)
  names=('remove_filtered_files','remove_filtered_files_even_when_skipped','remove_trashed_files','remove_local_domain_moved_files')
  controls=[getattr(panel,'_'+n) for n in names]
  initial=[x.isChecked() for x in controls]
  def prefs():return [HC.options['remove_filtered_files'],c.new_options.GetBoolean(names[1]),HC.options['remove_trashed_files'],c.new_options.GetBoolean(names[3])]
  settings=[]
  for values in itertools.product((False,True),repeat=4):
   for control,value in zip(controls,values):control.setChecked(value)
   panel._UpdateRemoveFiltered();panel.UpdateOptions()
   settings.append(dict(values=values,persisted=prefs(),skipped_enabled=controls[1].isEnabled()))
  panel.resize(1000,900);panel.show();QW.QApplication.processEvents()
  panel.grab().save(str(HERE/'fixtures/files_view_removal.png'))
  domains=sorted(c.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN,)),key=lambda s:s.GetName())
  source=domains[0].GetServiceKey();dest=domains[1].GetServiceKey()
  results=c.Read('media_results_from_ids',[1,2,3]);hashes=[r.GetHash() for r in results]
  # Detached fixture media has missing imported timestamps; give its real managers
  # synthetic display metadata so the hover text does not throw during SetMedia.
  def detached(result):
   result=result.Duplicate()
   times=result.GetTimesManager();times.SetFileModifiedTimestampMS(1234567890000)
   for key in set(result.GetLocationsManager().GetCurrent())|{s.GetServiceKey() for s in c.services_manager.GetLocalMediaFileServices()}|{CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY}:times.SetImportedTimestampMS(key,1234567890000)
   return result
  old_finish=Finish.GetFinishArchiveDeleteFilteringAnswer;old_thread=c.CallToThread
  answer=[QW.QDialog.DialogCode.Accepted,False];commits=[]
  Finish.GetFinishArchiveDeleteFilteringAnswer=lambda *args:(answer[0],ClientLocation.LocationContext.STATICCreateSimple(source),answer[1])
  def thread(fn,*args,**kwargs):
   if fn is Canvas.CommitArchiveDelete:
    commits.append(dict(kept=[m.GetHash().hex() for m in args[1]],deleted=[m.GetHash().hex() for m in args[2]]));return None
   return old_thread(fn,*args,**kwargs)
  c.CallToThread=thread
  filters=[];frames=[]
  try:
   for remove,skipped in itertools.product((False,True),repeat=2):
    for outcome in ('accept','forget','cancel'):
     controls[0].setChecked(remove);controls[1].setChecked(skipped);panel.UpdateOptions()
     frame=ClientGUICanvasFrame.CanvasFrame(c.gui);frames.append(frame)
     canvas=Canvas.CanvasMediaListFilterArchiveDelete(frame,os.urandom(32),ClientLocation.LocationContext.STATICCreateSimple(source),[detached(r) for r in results])
     frame.SetCanvas(canvas);frame.showNormal();frame.resize(1000,700);QW.QApplication.processEvents()
     media=canvas._media_list.GetSortedMedia();canvas.SetMedia(media[0])
     canvas._kept={media[0]};canvas._deleted={media[1]};canvas._skipped={media[2]}
     removed=[];canvas.userRemovedMedia.connect(lambda h:removed.extend(sorted(x.hex() for x in h)))
     answer[:]=[QW.QDialog.DialogCode.Accepted if outcome=='accept' else QW.QDialog.DialogCode.Rejected,outcome=='cancel']
     before=len(commits);closed=canvas.UserOKToClose()
     filters.append(dict(remove=remove,skipped=skipped,outcome=outcome,can_close=closed,removed=removed,committed=len(commits)>before,current=canvas.GetMedia().GetHash().hex() if canvas.GetMedia() else None))
     canvas._kept=set();canvas._deleted=set();canvas._skipped=set();canvas.ClearMedia();frame.close()
   events=[]
   locations={'source':[source],'both':[source,dest],'combined':[CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY],'storage':[CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY],'trash':[CC.TRASH_SERVICE_KEY]}
   for trash,move in itertools.product((False,True),repeat=2):
    controls[2].setChecked(trash);controls[3].setChecked(move);panel.UpdateOptions()
    for local_count in (1,2):
     for action_label,action,key in [('trash',HC.CONTENT_UPDATE_DELETE,source),('move',HC.CONTENT_UPDATE_DELETE_FROM_SOURCE_AFTER_MIGRATE,source),('physical',HC.CONTENT_UPDATE_DELETE,CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY)]:
      for location_label,keys in locations.items():
       result=results[0].Duplicate();manager=result.GetLocationsManager()
       # Populate only synthetic in-memory current membership, keeping actual managers.
       manager._current={source,CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY,CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY}
       if local_count==2:manager._current.add(dest)
       if action_label=='move':manager._current.add(dest)
       media_list=ClientMediaList.MediaList(ClientLocation.LocationContext(current_service_keys=set(keys)),[result])
       update=U.ContentUpdate(HC.CONTENT_TYPE_FILES,action,[hashes[0]])
       result.ProcessContentUpdate(key,update)
       package=U.ContentUpdatePackage.STATICCreateFromContentUpdate(key,update)
       media_list.ProcessContentUpdatePackage(package)
       events.append(dict(trash=trash,move=move,local_count=local_count,action=action_label,location=location_label,remaining=sorted(h.hex() for h in media_list.GetHashes()),actual_trash=CC.TRASH_SERVICE_KEY in manager.GetCurrent()))
  finally:
   Finish.GetFinishArchiveDeleteFilteringAnswer=old_finish;c.CallToThread=old_thread
   for frame in frames:
    try:frame.deleteLater()
    except RuntimeError:pass
   panel.close();panel.deleteLater();HC.options.update(legacy_before);c.new_options=old_options
   QW.QApplication.sendPostedEvents(None,QC.QEvent.Type.DeferredDelete);QW.QApplication.processEvents()
  return dict(initial=initial,settings=settings,hashes=[h.hex() for h in hashes],filters=filters,commits=commits,content_events=events)
 return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
 import hydrus_driver,record_api
 if len(sys.argv)>1:
  output=Path(sys.argv[2]);result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
  output.write_text(json.dumps(result));return
 with tempfile.TemporaryDirectory() as tmp:
  path=Path(tmp)/'record.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(path));value=json.loads(path.read_text())
 (HERE/'fixtures/files_view_removal.json').write_text(json.dumps(value,indent=2)+'\n')
 print('wrote files_view_removal.json and PNG')
if __name__=='__main__':main()
