#!/usr/bin/env python3
"""Record actual Qt zoom-switch choices and file-metadata animation loop limits.

Drive Options panels, the real top hover button, real zoom container geometry,
and native Animation frames decoded by RasterContainerVideo from synthetic WebP
files with infinite/one/two play counts. Keep the reference clock controlled;
frames are painted normally before TIMERAnimationUpdate advances them.
"""
import json, os, sys, tempfile, time
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    from PIL import Image
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'synthetic-large.jpg')
        Image.new('RGB',(2000,1600),(31,47,63)).save(path)
        big_hash=bytes.fromhex(session.api.post('/add_files/add_file',{'path':path})['hash'])
    def qt():
        from PIL import Image
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusTime
        from hydrus.core.files import HydrusAnimationHandling
        from hydrus.client import ClientConstants as CC, ClientLocation, ClientApplicationCommand as CAC
        from hydrus.client.gui.panels.options import MediaViewerHoversPanel, MediaPlaybackPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame, ClientGUICanvasMedia, ClientGUICanvasHoverFrames
        controller=session.controller;options=controller.new_options
        before_zoom=options.GetInteger('zoom_switch_command');before_loop=options.GetBoolean('always_loop_gifs')
        hp=MediaViewerHoversPanel.MediaViewerHoversPanel(controller.gui)
        pp=MediaPlaybackPanel.MediaPlaybackPanel(controller.gui)
        initial={'zoom':hp._zoom_switch_sets.GetValue(),'loop':pp._always_loop_animations.isChecked(),
                 'choices':[hp._zoom_switch_sets.itemText(i) for i in range(hp._zoom_switch_sets.count())]}
        manifest=json.load(open(os.path.join(HERE,'fixtures/legacy_db/basic.manifest.json')))
        hashes={f['name']:bytes.fromhex(f['hash']) for f in manifest['files']}
        hashes['synthetic-large.jpg']=big_hash
        results=controller.Read('media_results',[hashes[name] for name in ('jpeg_00.jpg','synthetic-large.jpg','webp_animated.webp')])
        frame=ClientGUICanvasFrame.CanvasFrame(controller.gui)
        canvas=ClientGUICanvas.CanvasMediaListBrowser(frame,os.urandom(32),ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),results,hashes['jpeg_00.jpg'])
        frame.SetCanvas(canvas);frame.showNormal();frame.resize(1000,600);QW.QApplication.processEvents()
        media={name:canvas._media_list.GetMediaByHashes({hashes[name]})[0] for name in ('jpeg_00.jpg','synthetic-large.jpg','webp_animated.webp')}
        container=canvas._media_container;zoom=[];loops=[];metadata=[]
        mouse_orig=ClientGUICanvasMedia.ClientGUIFunctions.GetMousePos
        path_orig=controller.client_files_manager.GetFilePath
        clock_orig=HydrusTime.GetNowPrecise;clock=[100.0]
        try:
            canvas.SetMedia(media['jpeg_00.jpg']);QW.QApplication.processEvents()
            for index in range(hp._zoom_switch_sets.count()):
                hp._zoom_switch_sets.setCurrentIndex(index);hp.UpdateOptions()
                command=options.GetInteger('zoom_switch_command')
                hover=ClientGUICanvasHoverFrames.CanvasHoverFrameTop(frame,canvas,canvas._canvas_key)
                hover.sendApplicationCommand.connect(canvas.ProcessApplicationCommand)
                button=next(button for button in hover.findChildren(QW.QPushButton)
                            if getattr(button,'_args',()) and isinstance(button._args[0],CAC.ApplicationCommand)
                            and button._args[0].GetSimpleAction()==command)
                for name in ('jpeg_00.jpg','synthetic-large.jpg'):
                    canvas.SetMedia(media[name])
                    for centre in (ClientGUICanvasMedia.ZOOM_CENTERPOINT_MEDIA_CENTER,ClientGUICanvasMedia.ZOOM_CENTERPOINT_MOUSE):
                        options.SetInteger('media_viewer_zoom_center',centre)
                        container.ZoomReinit();container.ResetCenterPosition()
                        container.ZoomToZoomPercent(1.0)
                        container.move(container.pos()+QC.QPoint(37,19))
                        pointer=QC.QPoint(123,145)
                        ClientGUICanvasMedia.ClientGUIFunctions.GetMousePos=lambda:canvas.mapToGlobal(pointer)
                        states=[]
                        for _ in range(4):
                            button.click()
                            g=container.geometry()
                            states.append({'zoom':container.GetCurrentZoom(),'rect':[g.x(),g.y(),g.width(),g.height()]})
                        zoom.append({'choice':index,'command':command,'centre':centre,'name':name,
                                     'pointer':[pointer.x(),pointer.y()], 'canvas':[canvas.width(),canvas.height()],
                                     'resolution':list(media[name].GetResolution()),'states':states})
                hover.hide();hover.deleteLater()
            source=path_orig(hashes['webp_animated.webp'],HC.ANIMATION_WEBP)
            raw=open(source,'rb').read();offset=raw.index(b'ANIM')+12
            with tempfile.TemporaryDirectory() as directory:
                for format in ('GIF','PNG'):
                    for count in (None,0,1,2) if format=='GIF' else (0,1,2):
                        path=os.path.join(directory,'metadata.'+format.lower())
                        args={'save_all':True,'append_images':[Image.new('RGBA',(20,16),(0,255,0,255))],'duration':40}
                        if count is not None:args['loop']=count
                        Image.new('RGBA',(20,16),(255,0,0,255)).save(path,format=format,**args)
                        actual=(HydrusAnimationHandling.GetTimesToPlayPILAnimation(path) if format=='GIF'
                                else HydrusAnimationHandling.GetTimesToPlayAPNG(path))
                        metadata.append({'format':format,'stored_count':count,'count':actual,'bytes':open(path,'rb').read().hex()})
            HydrusTime.GetNowPrecise=lambda:clock[0]
            generator=ClientGUICanvas.CanvasBackgroundColourGenerator(canvas)
            with tempfile.TemporaryDirectory() as directory:
                for count in (0,1,2):
                    path=os.path.join(directory,f'loops-{count}.webp')
                    modified=bytearray(raw);modified[offset:offset+2]=count.to_bytes(2,'little');open(path,'wb').write(modified)
                    controller.client_files_manager.GetFilePath=lambda *args,**kwargs:path if args[0]==hashes['webp_animated.webp'] else path_orig(*args,**kwargs)
                    for always in (False,True):
                        pp._always_loop_animations.setChecked(always);pp.UpdateOptions()
                        animation=ClientGUICanvasMedia.Animation(canvas,CC.CANVAS_MEDIA_VIEWER,generator)
                        animation.resize(80,60);animation.show();clock[0]=100.0
                        animation.SetMedia(media['webp_animated.webp'],start_paused=False)
                        states=[]
                        for step in range(animation.GetNumFrames()*3+1):
                            deadline=time.monotonic()+5
                            while not animation._current_frame_drawn and time.monotonic()<deadline:
                                animation._TryToDrawCanvasBitmap();QW.QApplication.processEvents();time.sleep(.005)
                            assert animation._current_frame_drawn,'decoder did not supply actual frame'
                            clock[0]+=1
                            animation.TIMERAnimationUpdate()
                            states.append({'frame':animation.CurrentFrame(),'paused':animation._paused,'playthroughs':animation._playthrough_count})
                        loops.append({'count':count,'always':always,'decoded_count':animation._video_container.GetTimesToPlayAnimation(),'frames':animation.GetNumFrames(),'states':states})
                        animation.ClearMedia();animation.hide();animation.deleteLater()
        finally:
            controller.client_files_manager.GetFilePath=path_orig
            HydrusTime.GetNowPrecise=clock_orig
            ClientGUICanvasMedia.ClientGUIFunctions.GetMousePos=mouse_orig
            options.SetInteger('zoom_switch_command',before_zoom);options.SetBoolean('always_loop_gifs',before_loop)
            hp.deleteLater();pp.deleteLater();frame.hide();frame.deleteLater()
        return {'initial':initial,'zoom':zoom,'loops':loops,'metadata':metadata,'animation_bytes':raw.hex()}
    return session.controller.CallBlockingToQt(session.controller.gui,qt)

def main():
    import hydrus_driver, record_api
    if len(sys.argv)>1:
        output=sys.argv[2];result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record,port=45902)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        path=os.path.join(directory,'result.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:result=json.load(f)
    output=os.path.join(HERE,'fixtures/viewer_zoom_loop_options.json')
    with open(output,'w') as f:json.dump(result,f,indent=2);f.write('\n')
    print('wrote '+output)
if __name__=='__main__':main()
