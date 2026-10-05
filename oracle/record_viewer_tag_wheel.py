#!/usr/bin/env python3
"""Actual Qt hover-tag wheel policy and QScrollArea edge consumption.

Detached in-memory media carries synthetic tags; no database is changed. The
clock is held still. Real child/hover wheelEvent handlers run unchanged, with
propagated parent events delivered explicitly to observe accepted/ignored gates.
"""
import json, os, sys, tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
def record(session):
    def qt():
        from qtpy import QtCore as QC,QtGui as QG,QtWidgets as QW
        from hydrus.core import HydrusConstants as HC,HydrusTime
        from hydrus.client import ClientConstants as CC,ClientLocation
        from hydrus.client.media import ClientMediaManagers,ClientMediaSingle
        from hydrus.client.gui.panels.options import MediaViewerHoversPanel
        from hydrus.client.gui.canvas import ClientGUICanvas,ClientGUICanvasFrame
        c=session.controller;options=c.new_options;key='media_viewer_tags_scrolling_behaviour';old=options.GetInteger(key)
        panel=MediaViewerHoversPanel.MediaViewerHoversPanel(c.gui)
        choices=[panel._media_viewer_tags_scrolling_behaviour.itemText(i) for i in range(panel._media_viewer_tags_scrolling_behaviour.count())]
        manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text());h=next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name']=='jpeg_00.jpg')
        original=c.Read('media_results',[h])[0];media=original.Duplicate();frame=ClientGUICanvasFrame.CanvasFrame(c.gui)
        canvas=ClientGUICanvas.CanvasMediaListBrowser(frame,os.urandom(32),ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),[original],h)
        frame.SetCanvas(canvas);frame.showNormal();frame.resize(1000,750);QW.QApplication.processEvents()
        hover=canvas._tags_hover;hover.show();hover.resize(200,300);hover.layout().activate();QW.QApplication.processEvents()
        now=[1000.0];old_now=HydrusTime.GetNowFloat;HydrusTime.GetNowFloat=lambda:now[0]
        def event(dy):
            return QG.QWheelEvent(QC.QPointF(20,20),QC.QPointF(20,20),QC.QPoint(),QC.QPoint(0,dy),QC.Qt.MouseButton.NoButton,QC.Qt.KeyboardModifier.NoModifier,QC.Qt.ScrollPhase.NoScrollPhase,False)
        def set_tags(count):
            tags={f'tag:{i:03}' for i in range(count)};mapping={CC.DEFAULT_LOCAL_TAG_SERVICE_KEY:{HC.CONTENT_STATUS_CURRENT:tags}}
            media.SetTagsManager(ClientMediaManagers.TagsManager(mapping,mapping));hover._tags.SetTagsByMediaResults([media]);hover._tags._SetVirtualSize();hover.layout().activate();QW.QApplication.processEvents()
        cases=[];scroll=[]
        try:
            for code in range(4):
                panel._media_viewer_tags_scrolling_behaviour.SetValue(code);panel.UpdateOptions()
                for count in (2,200):
                    set_tags(count);bar=hover._tags.verticalScrollBar();bar.setValue(bar.maximum())
                    for name,t,transition,last_scroll,last_direction,last_parent,parent_direction,dy in [
                        ('recent-scroll',1001.1,1000.0,1001.0,-1,0.0,0,-120),
                        ('equal-delay',1001.57,1000.0,1001.0,-1,0.0,0,-120),
                        ('after-delay',1001.571,1000.0,1001.0,-1,0.0,0,-120),
                        ('direction-change',1003.0,1000.0,1001.0,-1,0.0,0,120),
                        ('settled-new-direction',1003.571,1000.0,1001.0,-1,1003.0,1,120),
                        ('new-media-grace',1000.2,1000.0,1000.1,-1,0.0,0,-120),
                    ]:
                        now[0]=t;hover._last_media_transition_time=transition;hover._last_wheel_direction_event_time=last_parent;hover._last_wheel_direction=parent_direction
                        hover._tags._last_wheel_event_that_scrolled_time=last_scroll;hover._tags._last_wheel_event_that_scrolled_direction=last_direction
                        e=event(dy);e.accept();hover.wheelEvent(e)
                        cases.append(dict(code=code,count=count,scrollbar=bar.isVisible(),name=name,now=t,transition=transition,last_scroll=last_scroll,last_direction=last_direction,last_parent=last_parent,parent_direction=parent_direction,dy=dy,propagates=not e.isAccepted(),stored=options.GetInteger(key)))
            set_tags(200);bar=hover._tags.verticalScrollBar();bar.setValue(0)
            for t,at in [(1000.0,0),(1000.1,bar.maximum())]:
                now[0]=t;bar.setValue(at);before=bar.value();e=event(-120);hover._tags.wheelEvent(e)
                scroll.append(dict(now=t,maximum=bar.maximum(),before=before,after=bar.value(),consumed=e.isAccepted(),last_scroll=hover._tags.GetLastWheelEventThatScrolledTime(),last_direction=hover._tags.GetLastWheelEventThatScrolledDirection()))
            transitions=[]
            set_tags(200);bar.setValue(100);hover._last_wheel_direction=-1;hover._last_wheel_direction_event_time=1000.0
            for t,count in [(1100.0,200),(1301.0,2)]:
                before=bar.value();now[0]=t;set_tags(count);hover.SetMedia(ClientMediaSingle.MediaSingle(media));QW.QApplication.processEvents()
                transitions.append(dict(now=t,count=count,before=before,after=bar.value(),direction=hover._last_wheel_direction,transition=hover._last_media_transition_time))
            return dict(initial=old,choices=choices,cases=cases,scroll=scroll,transitions=transitions)
        finally:
            HydrusTime.GetNowFloat=old_now;options.SetInteger(key,old);panel.deleteLater();frame.hide();frame.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
    import hydrus_driver
    if len(sys.argv)>1:
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as folder:
        out=Path(folder)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    (HERE/'fixtures/viewer_tag_wheel.json').write_text(json.dumps(result,indent=2)+'\n');print('wrote viewer_tag_wheel.json')
if __name__=='__main__':main()
