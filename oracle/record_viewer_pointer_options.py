#!/usr/bin/env python3
"""Record real drag settings, cursor transitions and animation-start boundaries.

Apply MediaViewerPanel drag controls and drive actual CanvasMediaListBrowser
BeginDrag/HandleMouseMoveWithoutEvent/EndDrag with synthetic pointer coordinates,
recording rectangle deltas and Qt cursor shapes. Also apply MediaPlaybackPanel
start percentage and inspect real cold/warm Animation.SetMedia indices, exposing
its previous-frame-count ordering. Uses only the synthetic basic fixture.
"""
import json
import os
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.panels.options import MediaViewerPanel, MediaPlaybackPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame, ClientGUICanvasMedia
        controller=session.controller
        options=controller.new_options
        keys=('disallow_media_drags_on_duration_media','hide_canvas_drags','anchor_canvas_drags')
        before={key:options.GetBoolean(key) for key in keys}
        old_start=HC.options['animation_start_position']
        panel=MediaViewerPanel.MediaViewerPanel(controller.gui)
        playback=MediaPlaybackPanel.MediaPlaybackPanel(controller.gui)
        initial={'disallow_duration_drag':panel._disallow_media_drags_on_duration_media.isChecked(),
                 'hide_drag':panel._hide_canvas_drags.isChecked(),'start_percent':playback._animation_start_position.value(),
                 'start_bounds':[playback._animation_start_position.minimum(),playback._animation_start_position.maximum()]}
        with open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')) as stream:manifest=json.load(stream)
        hashes={file['name']:bytes.fromhex(file['hash']) for file in manifest['files']}
        names=('jpeg_00.jpg','webp_animated.webp','gif_animated.gif')
        results=controller.Read('media_results',[hashes[name] for name in names])
        frame=ClientGUICanvasFrame.CanvasFrame(controller.gui)
        location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        canvas=ClientGUICanvas.CanvasMediaListBrowser(frame,os.urandom(32),location,results,hashes['jpeg_00.jpg'])
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000,750)
        QW.QApplication.processEvents()
        media={name:canvas._media_list.GetMediaByHashes({hashes[name]})[0] for name in names}
        old_mouse=ClientGUIFunctions.GetMousePos
        position=[QC.QPoint(120,140)]
        ClientGUIFunctions.GetMousePos=lambda:canvas.mapToGlobal(position[0])
        generator=ClientGUICanvas.CanvasBackgroundColourGenerator(canvas)
        def rect():
            g=canvas._media_container.geometry()
            return [g.x(),g.y(),g.width(),g.height()]
        def cursor():return int(canvas.cursor().shape().value)
        drags=[]
        starts=[]
        try:
            for name in ('jpeg_00.jpg','gif_animated.gif'):
                canvas.SetMedia(media[name])
                assert canvas.GetMedia() is not None, (name, media[name].GetLocationsManager().GetCurrent(), canvas.isVisible())
                assert canvas.GetMedia().GetHash() == hashes[name]
                for disallow in (False,True):
                    for hide in (False,True):
                        panel._disallow_media_drags_on_duration_media.setChecked(disallow)
                        panel._hide_canvas_drags.setChecked(hide)
                        panel.UpdateOptions()
                        options.SetBoolean('anchor_canvas_drags',False)
                        canvas.EndDrag()
                        canvas._last_motion_pos=QC.QPoint(0,0)
                        canvas.setCursor(QG.QCursor(QC.Qt.CursorShape.ArrowCursor))
                        position[0]=QC.QPoint(120,140)
                        before_rect=rect()
                        canvas.BeginDrag()
                        accepted=canvas._last_drag_pos is not None
                        pressed=cursor()
                        position[0]=QC.QPoint(141,147)
                        canvas.HandleMouseMoveWithoutEvent(True)
                        moved=cursor()
                        after_rect=rect()
                        canvas.EndDrag()
                        released=cursor()
                        position[0]=QC.QPoint(150,159)
                        canvas.HandleMouseMoveWithoutEvent(False)
                        drags.append({'name':name,'has_duration':media[name].HasDuration(),'disallow':disallow,'hide':hide,
                                      'accepted':accepted,'delta':[after_rect[0]-before_rect[0],after_rect[1]-before_rect[1]],
                                      'cursor':{'pressed':pressed,'moved':moved,'released':released,'ordinary_move':cursor()}})
            for percent in (0,25,50,75,100):
                playback._animation_start_position.setValue(percent)
                playback.UpdateOptions()
                animation=ClientGUICanvasMedia.Animation(canvas,CC.CANVAS_MEDIA_VIEWER,generator)
                animation.SetMedia(media['webp_animated.webp'],start_paused=True)
                cold={'frame':animation.CurrentFrame(),'frames':animation.GetNumFrames()}
                animation.SetMedia(media['gif_animated.gif'],start_paused=True)
                warm={'frame':animation.CurrentFrame(),'frames':animation.GetNumFrames()}
                starts.append({'percent':percent,'fraction':HC.options['animation_start_position'],'cold':cold,'warm':warm})
                animation.ClearMedia()
                animation.deleteLater()
        finally:
            ClientGUIFunctions.GetMousePos=old_mouse
            for key,value in before.items():options.SetBoolean(key,value)
            HC.options['animation_start_position']=old_start
            panel.deleteLater()
            playback.deleteLater()
            frame.hide()
            frame.deleteLater()
        return {'initial':initial,'drags':drags,'starts':starts,'cursor_values':{'arrow':int(QC.Qt.CursorShape.ArrowCursor.value),'blank':int(QC.Qt.CursorShape.BlankCursor.value)}}
    return session.controller.CallBlockingToQt(session.controller.gui,qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1:
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as stream:json.dump(result,stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as stream:result=json.load(stream)
    output=os.path.join(HERE,'fixtures','viewer_pointer_options.json')
    with open(output,'w') as stream:
        json.dump(result,stream,indent=2)
        stream.write('\n')
    print('wrote '+output)


if __name__=='__main__':main()
