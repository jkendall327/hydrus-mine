#!/usr/bin/env python3
"""Real FilesAndTrash policy controls and archive/delete finish-panel choices.

Copied basic client and detached real media; only modal/scheduling and worker
publication transports are captured. The actual canvas builds domain choices,
actual Qt buttons enforce activation and actual panel handlers select a context.
"""
import itertools,json,os,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
def record(session):
 def qt():
  from qtpy import QtCore as QC,QtWidgets as QW
  from hydrus.core import HydrusConstants as HC,HydrusSerialisable
  from hydrus.client import ClientConstants as CC,ClientLocation
  from hydrus.client.gui.canvas import ClientGUICanvas as Canvas,ClientGUICanvasFrame
  from hydrus.client.gui.panels import ClientGUIScrolledPanelsCommitFiltering as Finish
  from hydrus.client.gui import ClientGUITopLevelWindowsPanels,ClientGUIDialogsQuick
  from hydrus.client.gui.panels.options.FilesAndTrashPanel import FilesAndTrashPanel
  c=session.controller;original=c.new_options;c.new_options=original.Duplicate()
  names=['only_show_delete_from_all_local_domains_when_filtering','archive_delete_commit_panel_delays_multiple_delete_choices']
  panel=FilesAndTrashPanel(c.gui);controls=[getattr(panel,'_'+name) for name in names]
  def values():return [c.new_options.GetBoolean(name) for name in names]
  initial=values();settings=[]
  for state in itertools.product((False,True),repeat=2):
   for ctrl,value in zip(controls,state):ctrl.setChecked(value)
   before=values();panel.UpdateOptions();serialized=HydrusSerialisable.CreateFromString(c.new_options.DumpToString());reopened=FilesAndTrashPanel(c.gui)
   settings.append(dict(draft=state,before=before,saved=values(),reopened=[getattr(reopened,'_'+name).isChecked() for name in names],roundtrip=[serialized.GetBoolean(name) for name in names]));reopened.deleteLater()
  cancel=FilesAndTrashPanel(c.gui)
  for name in names:getattr(cancel,'_'+name).setChecked(not c.new_options.GetBoolean(name))
  cancel_before=values();cancel.deleteLater();cancel_after=values()
  domains=sorted(c.services_manager.GetServices((HC.LOCAL_FILE_DOMAIN,)),key=lambda s:s.GetName());source,dest=[s.GetServiceKey() for s in domains[:2]]
  forget=[]
  originals=c.Read('media_results_from_ids',[1,2]);frames=[];events=[];jobs=[]
  old_finish=Finish.GetFinishArchiveDeleteFilteringAnswer;old_thread=c.CallToThread;old_later=c.CallLaterQtSafe;old_yesno=ClientGUIDialogsQuick.GetYesNo
  current={}
  def finish(win,kept,options):
   dlg=ClientGUITopLevelWindowsPanels.DialogCustomButtonQuestion(win,'filtering done?')
   scheduled=[]
   def later(owner,delay,label,callback,*args,**kwargs):
    if label=='delayed button enable':scheduled.append((delay,callback));return None
    return old_later(owner,delay,label,callback,*args,**kwargs)
   c.CallLaterQtSafe=later
   try:p=Finish.QuestionArchiveDeleteFinishFilteringPanel(dlg,kept,options)
   finally:c.CallLaterQtSafe=old_later
   dlg.SetPanel(p);dlg.show();QW.QApplication.processEvents()
   commits=[b for b in p.findChildren(QW.QPushButton) if b.text()=='commit']
   initial_enabled=[b.isEnabled() for b in commits]
   before_context=sorted(k.hex() for k in p.GetLocationContext().current_service_keys)
   if scheduled:commits[0].click()
   refused_context=sorted(k.hex() for k in p.GetLocationContext().current_service_keys)
   for _,callback in scheduled:callback()
   after_enabled=[b.isEnabled() for b in commits]
   pick=min(current['pick'],len(commits)-1);commits[pick].click()
   selected=sorted(k.hex() for k in p.GetLocationContext().current_service_keys)
   event=dict(**current,kept=kept,options=[dict(keys=sorted(k.hex() for k in loc.current_service_keys),label=label) for loc,label in options],initial_enabled=initial_enabled,delays=[d for d,_ in scheduled],before_context=before_context,refused_context=refused_context,after_enabled=after_enabled,selected=selected,labels=[x.text() for x in p.findChildren(QW.QLabel) if x.text()],buttons=[b.text() for b in p.findChildren(QW.QPushButton)])
   if current['all']==False and current['delay']==True and current['page']=='source' and current['members']==2:
    dlg.grab().save(str(HERE/'fixtures/archive_delete_finish_qt.png'))
   events.append(event);loc=p.GetLocationContext();dlg.hide();dlg.deleteLater();return QW.QDialog.DialogCode.Accepted,loc,False
  def thread(fn,*args,**kwargs):
   if fn is Canvas.CommitArchiveDelete:
    jobs.append(dict(keys=sorted(k.hex() for k in args[0].current_service_keys),kept=len(args[1]),deleted=len(args[2])));return None
   return old_thread(fn,*args,**kwargs)
  Finish.GetFinishArchiveDeleteFilteringAnswer=finish;c.CallToThread=thread
  try:
   for all_domains,delay,page,members in itertools.product((False,True),(False,True),('source','both','combined','all'),(1,2)):
    c.new_options.SetBoolean(names[0],all_domains);c.new_options.SetBoolean(names[1],delay)
    results=[]
    for original_result in originals:
     result=original_result.Duplicate();manager=result.GetLocationsManager();manager._current={source,CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY,CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY}
     if members==2:manager._current.add(dest)
     times=result.GetTimesManager();times.SetFileModifiedTimestampMS(1234567890000)
     for key in manager.GetCurrent():times.SetImportedTimestampMS(key,1234567890000)
     results.append(result)
    keys={'source':[source],'both':[source,dest],'combined':[CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY],'all':[CC.COMBINED_FILE_SERVICE_KEY]}[page]
    frame=ClientGUICanvasFrame.CanvasFrame(c.gui);frames.append(frame)
    canvas=Canvas.CanvasMediaListFilterArchiveDelete(frame,os.urandom(32),ClientLocation.LocationContext(current_service_keys=set(keys)),results)
    frame.SetCanvas(canvas);frame.showNormal();frame.resize(1000,700);QW.QApplication.processEvents()
    media=canvas._media_list.GetSortedMedia();canvas.SetMedia(media[0]);canvas._kept={media[0]};canvas._deleted={media[1]}
    current=dict(all=all_domains,delay=delay,page=page,members=members,pick=1 if not all_domains and members==2 else 0)
    canvas.UserOKToClose();canvas._kept=set();canvas._deleted=set();canvas.ClearMedia();frame.close()
   for yes in (False,True):
    dlg=ClientGUITopLevelWindowsPanels.DialogCustomButtonQuestion(c.gui,'filtering done?')
    p=Finish.QuestionArchiveDeleteFinishFilteringPanel(dlg,'keep 1',[]);dlg.SetPanel(p);dlg.show();QW.QApplication.processEvents();questions=[]
    def answer(win,message,**kwargs):questions.append(message);return QW.QDialog.DialogCode.Accepted if yes else QW.QDialog.DialogCode.Rejected
    ClientGUIDialogsQuick.GetYesNo=answer;p.DoForget();forget.append(dict(yes=yes,questions=questions,visible=dlg.isVisible(),cancelled=dlg.WasCancelled()));dlg.hide();dlg.deleteLater()
  finally:
   Finish.GetFinishArchiveDeleteFilteringAnswer=old_finish;c.CallToThread=old_thread;c.CallLaterQtSafe=old_later;ClientGUIDialogsQuick.GetYesNo=old_yesno
   for frame in frames:
    try:frame.deleteLater()
    except RuntimeError:pass
   panel.deleteLater();c.new_options=original
   QW.QApplication.sendPostedEvents(None,QC.QEvent.Type.DeferredDelete);QW.QApplication.processEvents()
  return dict(names=names,initial=initial,settings=settings,cancel_before=cancel_before,cancel_after=cancel_after,domains=[dict(name=s.GetName(),key=s.GetServiceKey().hex()) for s in domains[:2]],combined_key=CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY.hex(),events=events,jobs=jobs,forget=forget)
 return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
 import hydrus_driver,record_api
 if len(sys.argv)>1:
  output=Path(sys.argv[2]);result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record);output.write_text(json.dumps(result));return
 with tempfile.TemporaryDirectory() as tmp:
  path=Path(tmp)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(path));value=json.loads(path.read_text())
 (HERE/'fixtures/archive_delete_policies.json').write_text(json.dumps(value,indent=2)+'\n');print('wrote archive_delete_policies.json')
if __name__=='__main__':main()
