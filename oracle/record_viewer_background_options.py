#!/usr/bin/env python3
"""Record four passive background preferences through actual Qt painting.

Use a real displayed synthetic JPEG canvas, real Options UpdateOptions and
actual _DrawBackgroundDetails/_DrawTags/_DrawTopMiddle/_DrawTopRight/_DrawNotes.
A QPainter subclass observes text calls while still painting the QImage. Record
all checkbox combinations with hover windows enabled and disabled, relative
notes origins and pixel occupancy before/after an opaque media-sized cover.
"""
import itertools
import json
import os
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.panels.options import MediaViewerHoversPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        controller=session.controller
        options=controller.new_options
        keys=('draw_tags_hover_in_media_viewer_background','draw_top_hover_in_media_viewer_background',
              'draw_top_right_hover_in_media_viewer_background','draw_notes_hover_in_media_viewer_background')
        disable=('disable_tags_hover_in_media_viewer','disable_top_right_hover_in_media_viewer','disable_notes_hover_in_media_viewer')
        index='draw_bottom_right_index_in_media_viewer_background'
        before={key:options.GetBoolean(key) for key in keys+disable+(index,)}
        panel=MediaViewerHoversPanel.MediaViewerHoversPanel(controller.gui)
        controls=(panel._draw_tags_hover_in_media_viewer_background,panel._draw_top_hover_in_media_viewer_background,
                  panel._draw_top_right_hover_in_media_viewer_background,panel._draw_notes_hover_in_media_viewer_background)
        hover_controls=(panel._enable_tags_hover_in_media_viewer,panel._enable_top_right_hover_in_media_viewer,panel._enable_notes_hover_in_media_viewer)
        initial=[control.isChecked() for control in controls]
        with open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')) as stream:manifest=json.load(stream)
        file_hash=next(bytes.fromhex(file['hash']) for file in manifest['files'] if file['name']=='jpeg_00.jpg')
        results=controller.Read('media_results',[file_hash])
        notes={'alpha':'synthetic first background note','zeta':'synthetic second background note'}
        results[0].GetNotesManager().SetNamesToNotes(notes)
        frame=ClientGUICanvasFrame.CanvasFrame(controller.gui)
        canvas=ClientGUICanvas.CanvasMediaListBrowser(frame,os.urandom(32),ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),results,file_hash)
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000,750)
        QW.QApplication.processEvents()
        canvas.SetMedia(canvas._media_list.GetMediaByHashes({file_hash})[0])
        assert canvas.GetMedia() is not None and canvas.GetMedia().GetHash()==file_hash
        kind=['']
        draws=[]
        calls=[]
        originals={name:getattr(canvas,'_Draw'+name) for name in ('Tags','TopMiddle','TopRight','Notes')}
        for name,original in originals.items():
            def observe(painter,*args,name=name,original=original):
                kind[0]=name
                entry={'kind':name}
                if args:entry['input_y']=args[0]
                calls.append(entry)
                result=original(painter,*args)
                if result is not None:entry['output_y']=result
                return result
            setattr(canvas,'_Draw'+name,observe)
        class Painter(QG.QPainter):
            def drawText(self,*args):
                rect=args[0]
                text=args[-1]
                draws.append({'kind':kind[0],'rect':[rect.x(),rect.y(),rect.width(),rect.height()],'text':text})
                return super().drawText(*args)
        def occupancy(image):
            data=bytes(image.constBits())
            return sum(data[at:at+3]!=b'\x20\x20\x20' for at in range(0,len(data),4))
        events=[]
        try:
            for enabled in (False,True):
                for values in itertools.product((False,True),repeat=4):
                    for control,value in zip(controls,values):control.setChecked(value)
                    for control in hover_controls:control.setChecked(enabled)
                    panel._draw_bottom_right_index_in_media_viewer_background.setChecked(False)
                    panel.UpdateOptions()
                    image=QG.QImage(canvas.width(),canvas.height(),QG.QImage.Format.Format_RGB32)
                    image.fill(QG.QColor(32,32,32))
                    painter=Painter(image)
                    draws.clear();calls.clear()
                    canvas._DrawBackgroundDetails(painter)
                    painter.end()
                    visible=occupancy(image)
                    cover=QG.QPainter(image)
                    cover.fillRect(image.rect(),QG.QColor(32,32,32))
                    cover.end()
                    events.append({'values':list(values),'hovers_enabled':enabled,'calls':list(calls),'draws':list(draws),
                                   'visible_pixels':visible,'opaque_cover_pixels':occupancy(image)})
        finally:
            for name in originals:delattr(canvas,'_Draw'+name)
            for key,value in before.items():options.SetBoolean(key,value)
            panel.deleteLater()
            frame.hide();frame.deleteLater()
        return {'initial':initial,'hash':file_hash.hex(),'canvas':[canvas.width(),canvas.height()],
                'notes':notes,'events':events}
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
    path=os.path.join(HERE,'fixtures','viewer_background_options.json')
    with open(path,'w') as stream:json.dump(result,stream,indent=2);stream.write('\n')
    print('wrote '+path)


if __name__=='__main__':main()
