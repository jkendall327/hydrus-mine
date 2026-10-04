#!/usr/bin/env python3
"""Record four real MediaViewerHoversPanel options and canvas consumers.

Drive the three hover enable controls plus passive index drawing on a synthetic
CanvasMediaListBrowser. Read the real hover ideal-layout decisions with actual
media/notes, and capture the actual _DrawIndexAndZoom text and anchor through
ClientGUIFunctions.DrawText. No backend player or OS cursor manipulation.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtGui as QG, QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.panels.options import MediaViewerHoversPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        controller = session.controller
        options = controller.new_options
        keys = ('disable_tags_hover_in_media_viewer', 'disable_top_right_hover_in_media_viewer',
                'disable_notes_hover_in_media_viewer', 'draw_bottom_right_index_in_media_viewer_background')
        background_keys = ('draw_tags_hover_in_media_viewer_background', 'draw_top_hover_in_media_viewer_background',
                           'draw_top_right_hover_in_media_viewer_background', 'draw_notes_hover_in_media_viewer_background')
        before = {key:options.GetBoolean(key) for key in keys + background_keys}
        panel = MediaViewerHoversPanel.MediaViewerHoversPanel(controller.gui)
        controls = (panel._enable_tags_hover_in_media_viewer, panel._enable_top_right_hover_in_media_viewer,
                    panel._enable_notes_hover_in_media_viewer, panel._draw_bottom_right_index_in_media_viewer_background)
        initial = [control.isChecked() for control in controls]
        with open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')) as stream:
            manifest = json.load(stream)
        hashes = [bytes.fromhex(file['hash']) for file in manifest['files'] if file['name'] in ('jpeg_00.jpg','png_alpha_00.png')]
        media = controller.Read('media_results', hashes)
        media[0].GetNotesManager().SetNamesToNotes({'details':'synthetic viewer hover note'})
        frame = ClientGUICanvasFrame.CanvasFrame(controller.gui)
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, media, media[0].GetHash())
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000,750)
        QW.QApplication.processEvents()
        hover_widgets = (canvas._tags_hover, canvas._top_right_hover, canvas._right_notes_hover)
        old_draw = ClientGUIFunctions.DrawText
        draws = []
        def draw(painter,x,y,text):
            draws.append({'x':x,'y':y,'text':text})
            return old_draw(painter,x,y,text)
        ClientGUIFunctions.DrawText = draw
        events = []
        try:
            # Keep other passive text disabled to isolate the selected drawing.
            for key in background_keys: options.SetBoolean(key,False)
            for values in ((True,True,True,True),(False,False,False,False),
                           (True,False,False,False),(False,True,False,False),
                           (False,False,True,False),(False,False,False,True),
                           (True,True,True,False),(False,False,False,True)):
                for control,value in zip(controls,values): control.setChecked(value)
                panel.UpdateOptions()
                # Panel UpdateOptions also restores other background fields.
                for key in background_keys: options.SetBoolean(key,False)
                ideals = []
                for widget in hover_widgets:
                    resize,size,position = widget._GetIdealSizeAndPosition()
                    ideals.append({'resize':resize,'size':[size.width(),size.height()],
                                   'position':[position.x(),position.y()]})
                image = QG.QImage(canvas.width(),canvas.height(),QG.QImage.Format.Format_RGB32)
                image.fill(QG.QColor(0,0,0))
                painter = QG.QPainter(image)
                draws.clear()
                canvas._DrawBackgroundDetails(painter)
                painter.end()
                events.append({'values':list(values),'stored':[options.GetBoolean(key) for key in keys],
                               'ideals':ideals,'draws':list(draws),'canvas':[canvas.width(),canvas.height()],
                               'index':canvas._GetIndexString(),'zoom':canvas._media_container.GetCurrentZoom()})
        finally:
            ClientGUIFunctions.DrawText = old_draw
            for key,value in before.items(): options.SetBoolean(key,value)
            panel.deleteLater()
            frame.hide()
            frame.deleteLater()
        return {'initial':initial,'events':events,'notes':{'details':'synthetic viewer hover note'}}
    return session.controller.CallBlockingToQt(session.controller.gui,qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output,'w') as stream: json.dump(result,stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as stream: result=json.load(stream)
    output = os.path.join(HERE,'fixtures','viewer_hover_options.json')
    with open(output,'w') as stream:
        json.dump(result,stream,indent=2)
        stream.write('\n')
    print('wrote '+output)


if __name__=='__main__': main()
