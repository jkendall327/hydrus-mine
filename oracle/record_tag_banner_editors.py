#!/usr/bin/env python3
"""Drive actual three TagPresentation banner buttons and child draft controls.

Record detached button Cancel/Apply, appearance RGBA/visibility/separator,
ordered namespace CRUD/movement/delete cancellation, cleaned example tags and
actual GenerateSummary/GetTitleString output. Synthetic tags only; Qt dialogs
are answered by scripted callbacks while their real panel methods run.
"""
import json,os,sys,tempfile
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
def record(session):
 def qt():
  from qtpy import QtWidgets as QW,QtGui as QG
  from hydrus.client.gui.panels.options.TagPresentationPanel import TagPresentationPanel
  from hydrus.client.gui.metadata import ClientGUITagSummaryGenerator as summary
  from hydrus.client.gui import ClientGUITopLevelWindowsPanels as windows,ClientGUIDialogsQuick as quick
  from hydrus.core import HydrusExceptions
  options=session.controller.new_options;parent=TagPresentationPanel(session.controller.gui,options)
  def value(g):
   bg,text,rows,separator,tags,show=g.ToTuple()
   return {'background':list(bg.getRgb()),'text':list(text.getRgb()),'namespace_info':[{'namespace':n,'prefix':p,'separator':s} for n,p,s in rows],'separator':separator,'example_tags':sorted(tags),'show':show}
  initial={k:value(getattr(parent,'_'+k).GetValue()) for k in ('thumbnail_top','thumbnail_bottom_right','media_viewer_top')}
  original_exec=windows.DialogEdit.exec;original_enter=quick.EnterText;original_yes=quick.GetYesNo
  events=[];questions=[];route=[]
  def snapshot(p,name):
   g=p.GetValue();events.append({'action':name,'value':value(g),'preview':p._test_result.text(),'numeric':g.GenerateSummary(['page:2','page:10','page:3']),'summary':g.GenerateSummary(['creator:alpha','title:beta','page:2','page:10','page:3','page:alpha','blue_eyes','page:１２','page:１３','page:１４'])})
  def drive(dialog):
   p=dialog._panel
   p._background_colour.SetValue(QG.QColor(12,34,56,78));p._text_colour.SetValue(QG.QColor(210,180,140,120))
   p._separator.setText(' / ')
   p._example_tags.setPlainText(' CREATOR:Alpha \ncreator:alpha\ntitle:Beta\npage:10\npage:2\npage:3\npage:alpha\n blue_eyes \n')
   snapshot(p,'appearance/examples')
   p._show.setChecked(False);p._UpdateTest();snapshot(p,'hide')
   p._show.setChecked(True);p._UpdateTest();snapshot(p,'show')
   answers=iter(['page','p=','..'])
   def enter(owner,message,**kwargs):questions.append({'text':message,'default':kwargs.get('default'),'allow_blank':kwargs.get('allow_blank')});return next(answers)
   quick.EnterText=enter;p._namespaces_listbox._Add();snapshot(p,'add namespace')
   box=p._namespaces_listbox._listbox;box.clearSelection();box.item(box.count()-1).setSelected(True)
   p._namespaces_listbox._Up();snapshot(p,'move up');p._namespaces_listbox._Down();snapshot(p,'move down')
   answers=iter(['','plain=',' + ']);p._namespaces_listbox._Edit();snapshot(p,'edit namespace')
   quick.EnterText=lambda *a,**kw:(_ for _ in ()).throw(HydrusExceptions.CancelledException())
   p._namespaces_listbox._Edit();snapshot(p,'cancel namespace')
   quick.GetYesNo=lambda owner,text,**kw:(questions.append({'delete':text}) or QW.QDialog.DialogCode.Rejected)
   p._namespaces_listbox._Delete();snapshot(p,'cancel delete')
   quick.GetYesNo=lambda owner,text,**kw:(questions.append({'delete':text}) or QW.QDialog.DialogCode.Accepted)
   p._namespaces_listbox._Delete();snapshot(p,'delete')
   return QW.QDialog.DialogCode.Rejected if len(route)==0 else QW.QDialog.DialogCode.Accepted
  try:
   windows.DialogEdit.exec=drive
   for key in ('thumbnail_top','thumbnail_top','thumbnail_bottom_right','media_viewer_top'):
    button=getattr(parent,'_'+key);before=value(button.GetValue());button._Edit()
    route.append({'key':key,'before':before,'after':value(button.GetValue()),'label':button.text(),'saved_before_parent':value(options.GetTagSummaryGenerator(key))})
   parent.UpdateOptions();saved={k:value(options.GetTagSummaryGenerator(k)) for k in initial}
   from hydrus.client import ClientConstants as CC
   from hydrus.client.media import ClientMediaManagers,ClientMediaSingle
   from hydrus.core import HydrusConstants as HC
   with open(os.path.join(HERE,'fixtures/legacy_db/basic.manifest.json')) as f:manifest=json.load(f)
   file_hash=next(bytes.fromhex(row['hash']) for row in manifest['files'] if row['name']=='jpeg_00.jpg')
   result=session.controller.Read('media_results',[file_hash])[0].Duplicate()
   consumer_tags={'creator:alpha','title:beta','page:2','page:10','page:3','page:alpha','blue_eyes'}
   tags={CC.DEFAULT_LOCAL_TAG_SERVICE_KEY:{HC.CONTENT_STATUS_CURRENT:consumer_tags,HC.CONTENT_STATUS_PENDING:{'title:pending'}}}
   result.SetTagsManager(ClientMediaManagers.TagsManager(tags,tags))
   media=ClientMediaSingle.MediaSingle(result)
   consumers={'tags':sorted(consumer_tags|{'title:pending'}),'viewer_title':media.GetTitleString(),'file':file_hash.hex()}
   consumers['thumbnail_top']=options.GetTagSummaryGenerator('thumbnail_top').GenerateSummary(consumers['tags'])
   consumers['thumbnail_bottom_right']=options.GetTagSummaryGenerator('thumbnail_bottom_right').GenerateSummary(consumers['tags'])
   return {'initial':initial,'events':events,'questions':questions,'routes':route,'saved':saved,'consumers':consumers}
  finally:
   windows.DialogEdit.exec=original_exec;quick.EnterText=original_enter;quick.GetYesNo=original_yes;parent.deleteLater()
 return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
 import hydrus_driver,record_api
 if len(sys.argv)>1:
  output=sys.argv[2];result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
  with open(output,'w') as f:json.dump(result,f)
  return
 with tempfile.TemporaryDirectory() as d:
  path=os.path.join(d,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
  with open(path) as f:result=json.load(f)
 output=os.path.join(HERE,'fixtures/tag_banner_editors.json')
 with open(output,'w') as f:json.dump(result,f,indent=2);f.write('\n')
 print('wrote '+output)
if __name__=='__main__':main()
