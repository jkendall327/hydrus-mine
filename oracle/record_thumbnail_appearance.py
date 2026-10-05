#!/usr/bin/env python3
"""Actual Qt appearance drafts, recovery pixels, two whole-cell fade paths and background cache.

Private copied basic fixture only. Both real thumbnail renderers load actual media;
their decorated cells are painted by the reference. Only the detached fade clock
is held still. Background images are synthetic temporary PNGs; no reference edits.
"""
import hashlib,json,sys,tempfile,time
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
 from qtpy import QtWidgets as QW,QtGui as QG,QtCore as QC
 from hydrus.client import ClientConstants as CC,ClientLocation
 from hydrus.client.gui.panels.options.ThumbnailsPanel import ThumbnailsPanel
 from hydrus.client.gui.pages import ClientGUIMediaResultsPanelThumbnails as T
 from hydrus.core import HydrusTime,HydrusSerialisable
 from hydrus.core.files.images import HydrusBlurhash
 c=session.controller;original=c.new_options;draft=original.Duplicate()
 hashes=[bytes.fromhex(f['hash']) for f in json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())['files']]
 def qt(fn):return c.CallBlockingToQt(c.gui,fn)
 def values():return dict(fade=draft.GetBoolean('fade_thumbnails'),blurhash=draft.GetBoolean('allow_blurhash_fallback'),background=draft.GetNoneableString('media_background_bmp_path'))
 def controls():
  c.new_options=draft
  initial=values();panel=ThumbnailsPanel(c.gui,draft)
  labels=[w.text() for w in panel.findChildren(QW.QLabel) if 'Fade thumbnails' in w.text() or 'blurhash missing' in w.text() or 'background image' in w.text()]
  panel._fade_thumbnails.setChecked(False);panel._allow_blurhash_fallback.setChecked(False);panel._media_background_bmp_path.SetPath('cancelled.png')
  cancelled=values();panel.deleteLater();cases=[]
  for fade,blurhash,path in [(False,False,''),(True,True,' '),(False,True,'synthetic.png'),(True,False,'')]:
   panel=ThumbnailsPanel(c.gui,draft);panel._fade_thumbnails.setChecked(fade);panel._allow_blurhash_fallback.setChecked(blurhash);panel._media_background_bmp_path.SetPath(path);panel.UpdateOptions()
   reopened=ThumbnailsPanel(c.gui,HydrusSerialisable.CreateFromString(draft.DumpToString()))
   cases.append(dict(input=[fade,blurhash,path],saved=values(),reopened=[reopened._fade_thumbnails.isChecked(),reopened._allow_blurhash_fallback.isChecked(),reopened._media_background_bmp_path.GetPath()]))
   panel.deleteLater();reopened.deleteLater()
  panel=ThumbnailsPanel(c.gui,draft);panel._media_background_bmp_path.SetPath('staged/current.png');browse=[];old_pick=QW.QFileDialog.getOpenFileName
  try:
   for response in ['', '/tmp/fixture/../synthetic.png']:
    def pick(*args,**kwargs):
     browse.append(dict(seed=args[2],response=response,options=int(kwargs['options'].value)));return response,''
    QW.QFileDialog.getOpenFileName=staticmethod(pick)
    panel._media_background_bmp_path._button.click();browse[-1]['after']=panel._media_background_bmp_path.GetPath();browse[-1]['saved']=values()
  finally:QW.QFileDialog.getOpenFileName=old_pick;panel.deleteLater()
  return dict(defaults=initial,labels=labels,cancelled=cancelled,cases=cases,browse=browse,new_renderer_default=original.GetBoolean('test_thumbnails_graphics_view'))
 out=qt(controls);print("controls done",flush=True)
 def open_page(new):
  qt(lambda:draft.SetBoolean('test_thumbnails_graphics_view',new))
  page=qt(lambda:c.gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY),initial_hashes=hashes))
  for _ in range(400):
   panel=qt(page.GetMediaResultsPanel)
   if len(panel._sorted_media)==len(hashes) and (not new or len(panel._media_to_thumbnails)==len(hashes)):return panel
   time.sleep(.05)
  raise RuntimeError('thumbnail page did not load')
 def pixels(image):
  image=image.convertToFormat(QG.QImage.Format.Format_RGBA8888)
  return hashlib.sha256(bytes(image.constBits())).hexdigest()
 def fades(panel,new):
  now=[100.0];old_clock=HydrusTime.GetNowFloat;HydrusTime.GetNowFloat=lambda:now[0]
  try:
   media=list(panel._sorted_media)[0]
   if new:
    item=panel._media_to_thumbnails[media]
    def cell():
     image=QG.QImage(item.width,item.height,QG.QImage.Format.Format_ARGB32_Premultiplied);image.fill(QC.Qt.GlobalColor.transparent);p=QG.QPainter(image);item._PaintThumbnailContent(p,item.media,item._view);p.end();return image
    no_old=item._GetFadeInOpacity();old=cell();item.is_selected=True;current=cell();item._cached_pixmap=QG.QPixmap.fromImage(old);item.StartFadeIn();start=item._cached_old_pixmap_for_fade is not None
    samples=[]
    for elapsed in [0,.05,.1,13/60,.5,1]:
     now[0]=100+elapsed;samples.append(dict(elapsed=elapsed,opacity=item._GetFadeInOpacity(),old_retained=item._cached_old_pixmap_for_fade is not None))
    current.save(str(HERE/'fixtures/thumbnail_appearance_qt.png'))
    item.is_animating=False
    return dict(duration=item.FADE_DURATION_S,no_old_opacity=no_old,old=pixels(old),new=pixels(current),whole_cell_changed=pixels(old)!=pixels(current),old_at_start=start,samples=samples)
   thumb=T.ThumbnailMediaSingle(media.GetDisplayMediaResult());old=thumb.GetQtImage(thumb,panel,1.0);thumb.Select();new_image=thumb.GetQtImage(thumb,panel,1.0)
   animation=T.ThumbnailWaitingToBeDrawnAnimated(media.GetHash(),thumb,0,new_image);image=old.copy();samples=[]
   for elapsed in [0,1/60,.1,.25,.5,1]:
    now[0]=100+elapsed;due=animation.DrawDue()
    if due:
     painter=QG.QPainter(image);animation.DrawToPainter(0,0,painter);painter.end()
    samples.append(dict(elapsed=elapsed,due=due,frames=animation.num_frames_drawn,complete=animation.DrawComplete(),pixels=pixels(image)))
   return dict(duration=animation.FADE_DURATION_S,frames=animation.num_frames_to_draw,old=pixels(old),new=pixels(new_image),whole_cell_changed=pixels(old)!=pixels(new_image),samples=samples)
  finally:HydrusTime.GetNowFloat=old_clock
 try:
  new_panel=open_page(True);print("new page",flush=True);out['new_fade']=qt(lambda:fades(new_panel,True))
  print("new fade done",flush=True);old_panel=open_page(False);print("old page",flush=True);out['old_fade']=qt(lambda:fades(old_panel,False))
  out['creation']=qt(lambda:dict(new_type=type(new_panel).__name__,old_type=type(old_panel).__name__,saved=draft.GetBoolean('test_thumbnails_graphics_view')))
  def recovery():
   samples=[]
   for code in ['000000','LEHV6nWB2yk8pyo0adR*.7kCMdnj','~00000'+'00'*89,'bad','!!!!!!']:
    for size in [(3,2),(32,32),(64,40),(16,100)]:
     try:
      a=HydrusBlurhash.GetNumpyFromBlurhash(code,*size);samples.append(dict(code=code,size=size,pixels=a.tobytes().hex()))
     except Exception as error:samples.append(dict(code=code,size=size,error=type(error).__name__))
   result=new_panel._sorted_media[0].GetDisplayMediaResult().Duplicate();result.GetFileInfoManager().blurhash='LEHV6nWB2yk8pyo0adR*.7kCMdnj'
   cache=c.thumbnails_cache;allow=cache._allow_blurhash_fallback;fallback=[]
   try:
    for enabled,code in [(True,result.GetFileInfoManager().blurhash),(False,result.GetFileInfoManager().blurhash),(True,'invalid'),(True,None)]:
     cache._allow_blurhash_fallback=enabled;result.GetFileInfoManager().blurhash=code;bitmap=cache._GetBestRecoveryThumbnailHydrusBitmap(result);fallback.append(dict(enabled=enabled,code=code,size=[bitmap.GetQtImage().width(),bitmap.GetQtImage().height()],default=bitmap is cache._special_thumbs[__import__('hydrus.core.HydrusConstants',fromlist=['APPLICATION_UNKNOWN']).APPLICATION_UNKNOWN]))
   finally:cache._allow_blurhash_fallback=allow
   return dict(pixels=samples,fallback=fallback)
  print('old fade done',flush=True);out['recovery']=qt(recovery);print('recovery done',flush=True)
  with tempfile.TemporaryDirectory() as directory:
   def backgrounds():
    path=Path(directory)/'authored.png';image=QG.QImage(31,17,QG.QImage.Format.Format_RGB32);image.fill(QG.QColor(240,20,90));image.save(str(path));results=[]
    manager=c.bitmap_manager
    for label,value in [('valid',str(path)),('same',str(path)),('blank',None),('missing',str(Path(directory)/'missing.png')),('space',' '),('restored',str(path))]:
     draft.SetNoneableString('media_background_bmp_path',value);bitmap=manager.GetMediaBackgroundPixmap();results.append(dict(case=label,size=[bitmap.width(),bitmap.height()] if bitmap else None,null=bitmap.isNull() if bitmap else None))
    draft.SetNoneableString('media_background_bmp_path',str(path));bitmap=manager.GetMediaBackgroundPixmap()
    # Actual old renderer draw is driven through its viewport paintEvent.
    old_panel.resize(460,300);old_panel.show();QW.QApplication.processEvents()
    old_panel.grab().save(str(HERE/'fixtures/thumbnail_background_qt.png'))
    new_panel.resize(460,300);new_panel.show();QW.QApplication.processEvents();new_draws=[]
    class Painter(QG.QPainter):
     def drawPixmap(self,*args):
      new_draws.append(dict(x=args[0],y=args[1],bitmap=[args[2].width(),args[2].height()]));return super().drawPixmap(*args)
    viewport=new_panel.viewport().rect();new_pixels=[]
    for origin in [(0,0),(30,70)]:
     canvas=QG.QImage(viewport.size(),QG.QImage.Format.Format_ARGB32_Premultiplied);canvas.fill(QC.Qt.GlobalColor.transparent);painter=Painter(canvas);painter.translate(-origin[0],-origin[1]);new_panel.drawBackground(painter,QC.QRectF(origin[0],origin[1],viewport.width(),viewport.height()));painter.end();new_pixels.append(pixels(canvas))
     if origin==(0,0):canvas.save(str(HERE/'fixtures/thumbnail_background_new_qt.png'))
    assert new_pixels[0]==new_pixels[1]
    return dict(cache=results,bitmap_size=[bitmap.width(),bitmap.height()],old_anchor_rule='viewport size minus unscaled pixmap size',new_draws=new_draws,new_viewport=[viewport.width(),viewport.height()],new_scrolled_pixels=new_pixels,new_inherited_owner=type(new_panel).drawBackground.__qualname__)
   out['background']=qt(backgrounds)
  out['legacy_options']=qt(lambda:json.loads(draft.DumpToString()))
  out['limits']=['Default graphics renderer inherits real drawBackground; old renderer bitmap also captured; origin changes counteracted through actual painter translate','Whole-cell content painted by actual Qt painter; fade clock alone held still','Recovery cache method and HydrusBlurhash decoder are real; missing metadata/private duplicated media only','After recording, real GUI animation timer is stopped/disconnected before driver SIGTERM to avoid queued reference shutdown callback against its deleted timer']
  qt(lambda:c.gui.UnregisterAnimationUpdateWindow(new_panel));qt(lambda:c.gui.UnregisterAnimationUpdateWindow(old_panel));qt(lambda:c.gui._animation_update_timer.stop());qt(lambda:c.gui._animation_update_timer.timeout.disconnect())
  return out
 finally:c.new_options=original

def main():
 import hydrus_driver,record_api
 if len(sys.argv)>1:
  import signal
  signal.alarm(55)
  from hydrus.client.gui.canvas import ClientGUIMPV
  ClientGUIMPV.MPV_IS_AVAILABLE=False
  Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
 with tempfile.TemporaryDirectory() as directory:
  out=Path(directory)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));value=json.loads(out.read_text())
 (HERE/'fixtures/thumbnail_appearance.json').write_text(json.dumps(value,indent=2)+'\n');print('wrote thumbnail_appearance.json and Qt images')
if __name__=='__main__':main()
