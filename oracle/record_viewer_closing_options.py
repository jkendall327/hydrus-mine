#!/usr/bin/env python3
"""Record four closing-focus controls against real pages and viewer close events.

Use initialized locked query pages containing synthetic fixture JPEGs. Connect a
real CanvasMediaListBrowser through the originating media panel's normal signal
wiring, switch the main notebook to another page, close the real CanvasFrame,
and capture original selection/focus, current page and activation requests.
Activation wrappers call the original Qt methods. Include retained closed owner,
missing exit media, already-selected multi-selection and unowned canvas cases.
"""
import itertools
import json
import os
import sys
import tempfile
import time
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    controller=session.controller
    gui=controller.gui
    def qt(fn):return controller.CallBlockingToQt(gui,fn)
    def build():
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.panels.options import MediaViewerPanel
        options=controller.new_options
        keys=('focus_media_tab_on_viewer_close_if_possible','focus_media_thumb_on_viewer_close',
              'activate_main_gui_on_focusing_viewer_close','activate_main_gui_on_viewer_close')
        before={key:options.GetBoolean(key) for key in keys}
        panel=MediaViewerPanel.MediaViewerPanel(gui)
        with open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')) as stream:manifest=json.load(stream)
        named={file['name']:bytes.fromhex(file['hash']) for file in manifest['files']}
        hashes=[named[name] for name in ('jpeg_00.jpg','jpeg_01.jpg','jpeg_02.jpg')]
        location=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        original=gui._notebook.NewPageQuery(location,page_name='synthetic originating page',initial_hashes=hashes[:2])
        other=gui._notebook.NewPageQuery(location,page_name='synthetic other page',initial_hashes=hashes[:2])
        return options,keys,before,panel,hashes,location,original,other
    options,keys,before,panel,hashes,location,original,other=qt(build)
    for _ in range(400):
        if qt(lambda:original._initialised and other._initialised):break
        time.sleep(.025)
    else:raise RuntimeError('real locked pages did not initialize')
    def drive():
        from qtpy import QtWidgets as QW, QtCore as QC
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        owner=original.GetMediaResultsPanel()
        controls=(panel._focus_media_tab_on_viewer_close_if_possible,panel._focus_media_thumb_on_viewer_close,
                  panel._activate_main_gui_on_focusing_viewer_close,panel._activate_main_gui_on_viewer_close)
        initial=[control.isChecked() for control in controls]
        media={media.GetHash():media for media in owner.GetSortedMedia()}
        results=controller.Read('media_results',hashes)
        other_tlw=QW.QWidget(None)
        other_tlw.setWindowTitle('synthetic background active window')
        other_tlw.show()
        requests=[]
        old_gui_activate=gui.activateWindow
        old_owner_activate=owner.activateWindow
        def activate_gui():
            requests.append('debug-main')
            old_gui_activate()
        def activate_owner():
            requests.append('focusing-panel')
            old_owner_activate()
        gui.activateWindow=activate_gui
        owner.activateWindow=activate_owner
        frames=[]
        def selected():return [media.GetHash().hex() for media in owner._GetSelectedMediaOrdered()]
        def snapshot():
            focused=owner._focused_media
            current=gui.GetCurrentPage()
            return {'page':'original' if current is original else 'other' if current is other else 'another',
                    'selected':selected(),'focused':focused.GetHash().hex() if focused else None,
                    'original_open':gui.GetPageFromPageKey(original.GetPageKey()) is not None,
                    'activation':list(requests)}
        def close(values,scenario='normal'):
            for control,value in zip(controls,values):control.setChecked(value)
            panel.UpdateOptions()
            if scenario!='closed-owner':
                owner._HitMedia(None,False,False)
                owner.SetFocusedMedia(media[hashes[0]])
                if scenario=='selected-multiple':owner._HitMedia(media[hashes[1]],True,False)
            frame=ClientGUICanvasFrame.CanvasFrame(gui)
            page_key=os.urandom(32) if scenario=='unowned' else original.GetPageKey()
            canvas=ClientGUICanvas.CanvasMediaListBrowser(frame,page_key,location,results,hashes[0])
            frame.SetCanvas(canvas)
            if scenario=='unowned':canvas.canvasWithHoversExiting.connect(gui.NotifyMediaViewerExiting)
            else:owner._ConnectCanvasWindowSignals(canvas)
            frame.showNormal()
            QW.QApplication.processEvents()
            target=hashes[2] if scenario=='missing-exit' else hashes[1]
            canvas.SetMedia(canvas._media_list.GetMediaByHashes({target})[0])
            assert canvas.GetMedia() is not None and canvas.GetMedia().GetHash()==target
            gui.ShowPage(other.GetPageKey())
            other_tlw.activateWindow()
            QW.QApplication.processEvents()
            QW.QApplication.setActiveWindow(other_tlw)
            requests.clear()
            before_state=snapshot()
            frame.close()
            after_state=snapshot()
            # Finish the close-owned deferred destruction before making the next
            # canvas; pooled renderers must not straddle a batch of closes.
            QW.QApplication.sendPostedEvents(None, QC.QEvent.Type.DeferredDelete)
            QW.QApplication.processEvents()
            return {'values':list(values),'scenario':scenario,'exit':target.hex(),'before':before_state,'after':after_state}
        events=[]
        try:
            for values in itertools.product((False,True),repeat=4):events.append(close(values))
            events.append(close((False,True,True,False),'missing-exit'))
            events.append(close((False,True,False,False),'selected-multiple'))
            events.append(close((True,True,True,True),'unowned'))
            # Actual notebook close retains an undoable hidden page/media panel.
            owner._HitMedia(None,False,False)
            owner.SetFocusedMedia(media[hashes[0]])
            index=gui._notebook.indexOf(original)
            gui._notebook._ClosePage(index)
            assert gui.GetPageFromPageKey(original.GetPageKey()) is None
            events.append(close((True,True,True,False),'closed-owner'))
        finally:
            del gui.activateWindow
            del owner.activateWindow
            for key,value in before.items():options.SetBoolean(key,value)
            panel.deleteLater()
            other_tlw.hide()
            other_tlw.deleteLater()
            # CanvasFrame.close owns its C++ destruction. Do not queue another
            # delete of already-closed frames or pooled rendering children.
        return {'initial':initial,'hashes':[value.hex() for value in hashes],'events':events}
    return qt(drive)


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
    path=os.path.join(HERE,'fixtures','viewer_closing_options.json')
    with open(path,'w') as stream:json.dump(result,stream,indent=2);stream.write('\n')
    print('wrote '+path)


if __name__=='__main__':main()
