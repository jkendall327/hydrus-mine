#!/usr/bin/env python3
"""Record six real viewer options and their native Qt canvas consumers.

Drive MediaPlaybackPanel/MediaViewerPanel, paint StaticImage transparency pixels,
resize a real CanvasMediaListBrowser after zoom/pan, and inspect its control bar
rectangles plus AnimationBar nub positions. Uses only the synthetic basic fixture.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.panels.options import MediaPlaybackPanel, MediaViewerPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame, ClientGUICanvasMedia
        controller = session.controller
        options = controller.new_options
        bools = ('media_viewer_recenter_media_on_window_resize','draw_transparency_checkerboard_media_canvas','draw_transparency_checkerboard_as_greenscreen')
        ints = ('animated_scanbar_height','animated_scanbar_nub_width')
        before = {key: options.GetBoolean(key) for key in bools}
        before.update({key: options.GetInteger(key) for key in ints})
        before['animated_scanbar_hide_height'] = options.GetNoneableInteger('animated_scanbar_hide_height')
        playback = MediaPlaybackPanel.MediaPlaybackPanel(controller.gui)
        viewer_panel = MediaViewerPanel.MediaViewerPanel(controller.gui)
        initial = dict(before)
        initial['height_bounds'] = [viewer_panel._animated_scanbar_height.minimum(), viewer_panel._animated_scanbar_height.maximum()]
        initial['nub_bounds'] = [viewer_panel._animated_scanbar_nub_width.minimum(), viewer_panel._animated_scanbar_nub_width.maximum()]
        manifest = json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
        named = {file['name']: bytes.fromhex(file['hash']) for file in manifest['files']}
        media_results = controller.Read('media_results', [named['jpeg_00.jpg'], named['png_alpha_00.png'], named['webp_animated.webp']])
        frame = ClientGUICanvasFrame.CanvasFrame(controller.gui)
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, media_results, named['jpeg_00.jpg'])
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000,750)
        QW.QApplication.processEvents()
        container = canvas._media_container
        generator = ClientGUICanvas.CanvasBackgroundColourGenerator(canvas)
        static = ClientGUICanvasMedia.StaticImage(canvas, CC.CANVAS_MEDIA_VIEWER, generator)
        alpha = next(media for media in media_results if media.GetHash() == named['png_alpha_00.png'])
        static._media = alpha
        bar = ClientGUICanvasMedia.AnimationBar(canvas)
        bar.resize(128,20)
        bar._duration_ms = 1000
        bar._num_frames = 5
        bar._media_window = container._animation_window
        old_mouse = ClientGUIFunctions.GetMousePos
        old_goto = bar._media_window.GotoFrame
        old_near = container.MouseIsNearAnimationBar
        container.MouseIsNearAnimationBar = lambda: False
        targets = []
        bar._media_window.GotoFrame = targets.append
        def rect(widget):
            geom = widget.geometry()
            return [geom.x(),geom.y(),geom.width(),geom.height()]
        backgrounds = []
        resizes = []
        seek = []
        try:
            for checker, green in ((False,False),(False,True),(True,False),(True,True)):
                playback._draw_transparency_checkerboard_media_canvas.setChecked(checker)
                playback._draw_transparency_checkerboard_as_greenscreen.setChecked(green)
                playback.UpdateOptions()
                image = QG.QImage(64,64,QG.QImage.Format.Format_RGB32)
                painter = QG.QPainter(image)
                static._DrawBackground(painter)
                painter.end()
                backgrounds.append({'checker':checker,'green':green,'has_transparency':alpha.GetFileInfoManager().has_transparency,
                    'pixels': {str(x)+','+str(y):list(image.pixelColor(x,y).getRgb())[:3] for x,y in ((0,0),(16,0),(0,16),(16,16),(32,0))}})
            for enabled in (False,True):
                playback._media_viewer_recenter_media_on_window_resize.setChecked(enabled)
                playback.UpdateOptions()
                frame.resize(1000,750)
                QW.QApplication.processEvents()
                container._TryToChangeZoom(2.0)
                container.MoveDelta(QC.QPoint(37,-19))
                previous = {'rect':rect(container),'zoom':container.GetCurrentZoom(),'canvas':[canvas.width(),canvas.height()]}
                frame.resize(800,600)
                QW.QApplication.processEvents()
                resizes.append({'recenter':enabled,'before':previous,'after':{'rect':rect(container),'zoom':container.GetCurrentZoom(),'canvas':[canvas.width(),canvas.height()]}})
            for height, hidden, nub in ((20,5,10),(1,None,1),(255,255,63),(37,3,19)):
                viewer_panel._animated_scanbar_height.setValue(height)
                viewer_panel._animated_scanbar_hide_height.SetValue(hidden)
                viewer_panel._animated_scanbar_nub_width.setValue(nub)
                viewer_panel.UpdateOptions()
                full = container.GetIdealControlsBarRect(True)
                small = container.GetIdealControlsBarRect(False)
                targets.clear()
                clicks = [0, int(nub / 2), 32, 64, 96, 128]
                for x in clicks:
                    ClientGUIFunctions.GetMousePos = lambda x=x: bar.mapToGlobal(QC.QPoint(x,0))
                    bar._ScanToCurrentMousePos()
                old_media, old_action = container._media, container._show_action
                container._media = next(media for media in media_results if media.GetHash() == named['webp_animated.webp'])
                container._show_action = CC.MEDIA_VIEWER_ACTION_SHOW_WITH_NATIVE
                container._ShowHideControlBar()
                hidden_visible = not container._controls_bar.isHidden()
                container._media, container._show_action = old_media, old_action
                seek.append({'height':height,'hidden_height':hidden,'nub':nub,
                    'full_rect':[full.x(),full.y(),full.width(),full.height()],
                    'small_rect':[small.x(),small.y(),small.width(),small.height()],
                    'hidden_visible':hidden_visible,'click_x':clicks,'frame_targets':list(targets),
                    'nub_x':[bar._GetXFromTimestamp(timestamp,width_offset=nub) for timestamp in (0,250,500,1000)]})
        finally:
            ClientGUIFunctions.GetMousePos = old_mouse
            bar._media_window.GotoFrame = old_goto
            container.MouseIsNearAnimationBar = old_near
            for key in bools: options.SetBoolean(key,before[key])
            for key in ints: options.SetInteger(key,before[key])
            options.SetNoneableInteger('animated_scanbar_hide_height',before['animated_scanbar_hide_height'])
            static.deleteLater()
            bar.deleteLater()
            playback.deleteLater()
            viewer_panel.deleteLater()
            frame.hide()
            frame.deleteLater()
        return {'media_resolution':list(next(media for media in media_results if media.GetHash() == named['jpeg_00.jpg']).GetResolution()),'initial':initial,'backgrounds':backgrounds,'resizes':resizes,'seek':seek}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    output = os.path.join(HERE, 'fixtures', 'viewer_canvas_options.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()
