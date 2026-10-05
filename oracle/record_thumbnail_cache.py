#!/usr/bin/env python3
"""Actual Qt thumbnail byte/time controls, real DataCache boundaries and debug clear.

Only a detached DataCache monotonic clock and synthetic HydrusBitmap bytes are
scripted. The real help/debug QAction executes the real thumbnail-cache Clear
synchronously at its publication boundary, recording reset notification. No
network, production launch, or image/tile/video cache clear is invoked.
"""
import json,sys,tempfile,types
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
def record(session):
 def qt():
  from qtpy import QtWidgets as QW
  from hydrus.client.gui.panels.options.SpeedAndMemoryPanel import SpeedAndMemoryPanel
  from hydrus.client.gui.metadata import ClientGUITime
  from qtpy import QtCore as QC
  from hydrus.client.caches import ClientCachesBase as Base
  from hydrus.client import ClientRendering
  c=session.controller;opts=c.new_options.Duplicate()
  panel=SpeedAndMemoryPanel(c.gui,opts)
  initial=dict(bytes=panel._thumbnail_cache_size.GetValue(),timeout=panel._thumbnail_cache_timeout.GetValue())
  controls=[]
  for size,timeout in [(1,299),(1024,300),(32768,301),(32*1024*1024,86400),(1048576*1024**4,60*60*24*2)]:
   panel._thumbnail_cache_size.SetValue(size);panel._thumbnail_cache_timeout.SetValue(timeout)
   panel.UpdateOptions()
   controls.append(dict(input=[size,timeout],separated=list(panel._thumbnail_cache_size.GetSeparatedValue()),saved=[opts.GetInteger('thumbnail_cache_size'),opts.GetInteger('thumbnail_cache_timeout')],estimate=panel._estimated_number_thumbnails.text()))
  panel._thumbnail_cache_size.SetValue(initial['bytes']);panel._thumbnail_cache_timeout.SetValue(initial['timeout'])
  parent=panel._thumbnail_cache_size.parentWidget()
  while not hasattr(parent,'_expand_button'):parent=parent.parentWidget()
  parent._expand_button.click()
  panel.resize(1100,600);panel.show();QW.QApplication.processEvents()
  panel.grab().save(str(HERE/'fixtures/thumbnail_cache.png'));panel.close()
  raw_options=opts.Duplicate();raw_options.SetInteger('thumbnail_cache_timeout',299);raw_options.SetInteger('thumbnail_cache_size',1048577)
  raw_panel=SpeedAndMemoryPanel(c.gui,raw_options);raw_panel.UpdateOptions()
  edited=ClientGUITime.TimeDeltaWidget(raw_panel,min=300,days=True,hours=True,minutes=True)
  edited.SetValue(0);edited.eventFilter(edited._minutes,QC.QEvent(QC.QEvent.Type.FocusOut))
  timeout_boundaries=dict(loaded=raw_panel._thumbnail_cache_timeout.GetValue(),unchanged_apply=raw_options.GetInteger('thumbnail_cache_timeout'),edited_minimum=edited.GetValue())
  bytes_boundaries=dict(raw=1048577,loaded=raw_panel._thumbnail_cache_size.GetValue(),unchanged_apply=raw_options.GetInteger('thumbnail_cache_size'))
  raw_panel.deleteLater()
  class Owner:
   def sub(self,*args):pass
  clock=[0.0];old_time=Base.time;Base.time=types.SimpleNamespace(monotonic=lambda:clock[0])
  trace=[];cache=Base.DataCache(Owner(),'authored thumbnail cache',100,timeout=300)
  def state(action):trace.append(dict(action=action,now=clock[0],keys=cache.GetAllKeys(),bytes=cache._total_estimated_memory_footprint,limit=cache.GetSizeLimit()))
  def add(key,size):cache.AddData(key,ClientRendering.HydrusBitmap(bytes([23])*size,(size,1),1,compressed=False));state('add '+key)
  try:
   add('a',60);clock[0]=1;add('b',40);clock[0]=2;add('c',20)
   cache.GetData('a');state('get a');clock[0]=3;add('d',10)
   cache.MaintainCache();state('maintain size')
   clock[0]=302;cache.MaintainCache();state('maintain exact timeout')
   clock[0]=302.001;cache.MaintainCache();state('maintain past timeout')
   cache.TouchKey('d');state('touch d');clock[0]=603;cache.MaintainCache();state('maintain idle')
   add('huge',120);cache.SetCacheSizeAndTimeout(50,300);state('shrink')
   add('e',20);add('f',20);cache.SetCacheSizeAndTimeout(50,600);state('extend timeout')
   cache.Clear();state('clear');add('e',20);state('reload')
  finally:Base.time=old_time
  before_policy=[c.new_options.GetInteger('thumbnail_cache_size'),c.new_options.GetInteger('thumbnail_cache_timeout')]
  c.new_options.SetInteger('thumbnail_cache_size',100);c.new_options.SetInteger('thumbnail_cache_timeout',300)
  c.thumbnails_cache.NotifyNewOptions()
  live_policy=dict(bytes=c.thumbnails_cache._data_cache.GetSizeLimit(),timeout=c.thumbnails_cache._data_cache._timeout)
  c.new_options.SetInteger('thumbnail_cache_size',before_policy[0]);c.new_options.SetInteger('thumbnail_cache_timeout',before_policy[1]);c.thumbnails_cache.NotifyNewOptions()
  old_pub=c.pub;published=[]
  def pub(topic,*args,**kwargs):
   if topic=='clear_thumbnail_cache':
    published.append(topic);c.thumbnails_cache.Clear();return
   if topic=='notify_complete_thumbnail_reset':published.append(topic);return
   return old_pub(topic,*args,**kwargs)
  c.pub=pub
  try:
   menu,label=c.gui._InitialiseMenuInfoHelp()
   def find(menu,path):
    for action in menu.actions():
     if action.text()==path[0]:return action if len(path)==1 else find(action.menu(),path[1:])
    raise AssertionError(path)
   action=find(menu,['debug','memory actions','clear thumbnail cache'])
   action.trigger();QW.QApplication.processEvents()
   debug=dict(label=action.text(),tooltip=action.toolTip(),published=published,keys=c.thumbnails_cache._data_cache.GetAllKeys())
   menu.deleteLater()
  finally:c.pub=old_pub
  return dict(initial=initial,controls=controls,timeout_boundaries=timeout_boundaries,bytes_boundaries=bytes_boundaries,cache=trace,live_policy=live_policy,debug=debug)
 return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
 import hydrus_driver,record_api
 if len(sys.argv)>1:
  out=Path(sys.argv[2]);value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record);out.write_text(json.dumps(value));return
 with tempfile.TemporaryDirectory() as tmp:
  out=Path(tmp)/'record.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));value=json.loads(out.read_text())
 (HERE/'fixtures/thumbnail_cache.json').write_text(json.dumps(value,indent=2)+'\n');print('wrote thumbnail_cache.json and PNG')
if __name__=='__main__':main()
