#!/usr/bin/env python3
"""Record actual Qt thumbnail navigation preferences, selection and scroll consumers.

Private Options drafts test Apply/Cancel/serialization. Actual loaded media panel
methods and QWheelEvent drive consumers; source Hydrus stays unchanged.
"""
import json,sys,tempfile,time
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
KEYS=['on_shift_click_move_ghost_focus_to_last_hit','thumbnail_visibility_scroll_percent','thumbnail_scroll_rate']

def record(session):
    from qtpy import QtCore as QC,QtGui as QG,QtWidgets as QW
    from hydrus.client import ClientConstants as CC,ClientLocation,ClientApplicationCommand as CAC
    from hydrus.client.gui.panels.options.ThumbnailsPanel import ThumbnailsPanel
    from hydrus.core import HydrusSerialisable
    c=session.controller;gui=c.gui;original=c.new_options
    manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes=[bytes.fromhex(f['hash']) for f in manifest['files']]
    def qt(fn):return c.CallBlockingToQt(gui,fn)
    qt(lambda:gui.resize(1100,650))
    page=qt(lambda:gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),initial_hashes=hashes))
    for _ in range(400):
        panel=qt(page.GetMediaResultsPanel)
        if hasattr(panel,'_thumbnail_layout') and len(panel._sorted_media)==len(hashes):break
        time.sleep(.05)
    else:raise RuntimeError('thumbnail page did not load')
    time.sleep(.5)
    def drive():
        draft=original.Duplicate();c.new_options=draft
        def values():return dict(shift=draft.GetBoolean(KEYS[0]),percent=draft.GetInteger(KEYS[1]),rate=draft.GetString(KEYS[2]))
        defaults=values();options=ThumbnailsPanel(gui,draft)
        labels=[label.text() for label in options.findChildren(QW.QLabel) if any(s in label.text() for s in ['navigate from here','Do not scroll down','Scroll thumbnails at this rate'])]
        options._on_shift_click_move_ghost_focus_to_last_hit.setChecked(True);options._thumbnail_visibility_scroll_percent.setValue(40);options._thumbnail_scroll_rate.setText('0.5');cancelled=values();options.deleteLater()
        events=[]
        for shift,percent,rate in [(True,40,'0.5'),(False,0,'1.5'),(True,100,' 2e-1 '),(False,75,'invalid'),(False,75,'０.５'),(False,75,'1_0.0'),(False,75,'1__0'),(False,75,'0'),(False,75,'-0.5'),(False,75,'nan'),(False,75,'inf')]:
            options=ThumbnailsPanel(gui,draft);options._on_shift_click_move_ghost_focus_to_last_hit.setChecked(shift);options._thumbnail_visibility_scroll_percent.setValue(percent);options._thumbnail_scroll_rate.setText(rate);options.UpdateOptions();saved=values()
            reopened=ThumbnailsPanel(gui,HydrusSerialisable.CreateFromString(draft.DumpToString()))
            events.append(dict(input=[shift,percent,rate],saved=saved,reopened=dict(shift=reopened._on_shift_click_move_ghost_focus_to_last_hit.isChecked(),percent=reopened._thumbnail_visibility_scroll_percent.value(),rate=reopened._thumbnail_scroll_rate.text())))
            options.deleteLater();reopened.deleteLater()
        draft.SetString(KEYS[2],'1.0')
        media=list(panel._sorted_media);index={m:i for i,m in enumerate(media)}
        def snapshot():return dict(selected=sorted(index[m] for m in panel._selected_media),focused=index.get(panel._focused_media),last_hit=index.get(panel._last_hit_media))
        selections=[]
        for enabled in [False,True]:
            draft.SetBoolean(KEYS[0],enabled);panel._HitMedia(None,False,False)
            steps=[]
            for action,i in [('click',2),('shift',6),('move',0),('shift',4),('move',0)]:
                if action=='move':panel.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_MOVE_THUMBNAIL_FOCUS,(CAC.MOVE_RIGHT,CAC.SELECTION_STATUS_NORMAL)))
                else:panel._HitMedia(media[i],False,action=='shift')
                steps.append(dict(action=action,index=i,after=snapshot()))
            selections.append(dict(enabled=enabled,steps=steps))
        thumb=panel._media_to_thumbnails[media[-1]];span=panel._thumbnail_layout.ThumbnailSpanDimensions(thumb);height=panel.viewport().height();scrolls=[]
        for percent in [1,40,75,99]:
            draft.SetInteger(KEYS[1],percent)
            for extra in [-1,0,1]:
                top=thumb.scenePos().y();offset=int(top-height+span[1]*percent/100)+extra;panel.verticalScrollBar().setValue(offset)
                before=panel.verticalScrollBar().value();visible=panel.mapToScene(panel.viewport().rect()).boundingRect()
                panel._ScrollToMedia(media[-1]);scrolls.append(dict(percent=percent,extra=extra,top=top,span=span[1],view_y=visible.y(),view_height=visible.height(),before=before,after=panel.verticalScrollBar().value()))
        rates=[]
        for rate in ['1.0','0.5','1.5','0.125','0','-0.5','nan','inf']:
            draft.SetString(KEYS[2],rate);before=panel.verticalScrollBar().singleStep();error=None
            try:panel._ResetThumbnailScrollSingleStep()
            except Exception as e:error=type(e).__name__
            step=panel.verticalScrollBar().singleStep();panel.verticalScrollBar().setValue(0);point=QC.QPointF(30,30)
            event=QG.QWheelEvent(point,QC.QPointF(panel.viewport().mapToGlobal(point.toPoint())),QC.QPoint(),QC.QPoint(0,-120),QC.Qt.MouseButton.NoButton,QC.Qt.KeyboardModifier.NoModifier,QC.Qt.ScrollPhase.NoScrollPhase,False)
            QW.QApplication.sendEvent(panel.viewport(),event)
            rates.append(dict(rate=rate,span=span[1],before=before,step=step,error=error,wheel_after=panel.verticalScrollBar().value(),wheel_lines=QW.QApplication.wheelScrollLines()))
        draft.SetString(KEYS[2],'1.0')
        return dict(keys=KEYS,labels=labels,defaults=defaults,cancelled=cancelled,events=events,selections=selections,scrolls=scrolls,rates=rates,geometry=dict(columns=panel._thumbnail_layout._num_columns if hasattr(panel._thumbnail_layout,'_num_columns') else None,span=list(span),viewport_height=height,content_height=panel.sceneRect().height()),limits=['MPV availability disabled only in this offscreen recorder process; preview playback is not tested','actual thumbnail panel decision handlers unchanged; full native OS wheel routing outside the recorder'])
    try:return qt(drive)
    finally:c.new_options=original

def child(out):
    import hydrus_driver,record_api,signal
    signal.alarm(25)
    # Offscreen MPV emits fatal GPU logs and blocks Qt initialization. Preview
    # playback is outside this recorder; retain actual thumbnail/scroll handlers.
    from hydrus.client.gui.canvas import ClientGUIMPV
    ClientGUIMPV.MPV_IS_AVAILABLE=False
    Path(out).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as temp:
        out=Path(temp)/'out.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));data=json.loads(out.read_text())
    dest=HERE/'fixtures/thumbnail_navigation.json';dest.write_text(json.dumps(data,indent=2)+'\n');print(f'wrote {dest}')
if __name__=='__main__':main()
