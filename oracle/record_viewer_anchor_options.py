#!/usr/bin/env python3
"""Actual Qt anchor/touch override controls and real media drag transitions.

Synthetic pointer coordinates replace only GetMousePos; QCursor.setPos captures
requested warps instead of moving the workstation cursor. Actual BeginDrag,
HandleMouseMoveWithoutEvent, MoveDelta and Options save remain unchanged.
"""
import json, os, sys, tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
def record(session):
    def qt():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.panels.options import MediaViewerPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        c=session.controller; options=c.new_options
        keys=('anchor_canvas_drags','touchscreen_canvas_drags_unanchor','hide_canvas_drags')
        old={k:options.GetBoolean(k) for k in keys}
        panel=MediaViewerPanel.MediaViewerPanel(c.gui)
        initial=dict(anchor=panel._anchor_canvas_drags.isChecked(),touch_override=panel._touchscreen_canvas_drags_unanchor.isChecked())
        manifest=json.loads((HERE/'fixtures/legacy_db/basic.manifest.json').read_text())
        h=next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name']=='jpeg_00.jpg')
        media=c.Read('media_results',[h])
        frame=ClientGUICanvasFrame.CanvasFrame(c.gui)
        canvas=ClientGUICanvas.CanvasMediaListBrowser(frame,os.urandom(32),ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),media,h)
        frame.SetCanvas(canvas); frame.showNormal(); frame.resize(1000,750); QW.QApplication.processEvents()
        point=[QC.QPoint(120,140)]; warps=[]
        old_mouse=ClientGUIFunctions.GetMousePos; old_warp=QG.QCursor.setPos
        ClientGUIFunctions.GetMousePos=lambda:canvas.mapToGlobal(point[0])
        QG.QCursor.setPos=lambda p:warps.append([canvas.mapFromGlobal(p).x(),canvas.mapFromGlobal(p).y()])
        cases=[]
        try:
            for anchor in (False,True):
                for touch in (False,True):
                    panel._anchor_canvas_drags.setChecked(anchor); panel._touchscreen_canvas_drags_unanchor.setChecked(touch); panel.UpdateOptions()
                    options.SetBoolean('hide_canvas_drags',False)
                    point[0]=QC.QPoint(120,140); canvas.EndDrag(); canvas.BeginDrag(); warps.clear()
                    moves=[]
                    for x,y in [(140,150),(170,140),(171,140),(180,150),(120,140),(121,140)]:
                        point[0]=QC.QPoint(x,y); before=canvas._media_container.pos(); count=len(warps)
                        canvas.HandleMouseMoveWithoutEvent(True)
                        after=canvas._media_container.pos(); last=canvas._last_drag_pos
                        moves.append(dict(point=[x,y],delta=[after.x()-before.x(),after.y()-before.y()],last=[last.x(),last.y()],touch=canvas._current_drag_is_touch,warp=warps[count:]))
                    canvas.EndDrag(); before=canvas._media_container.pos(); point[0]=QC.QPoint(150,150); canvas.HandleMouseMoveWithoutEvent(False)
                    after=canvas._media_container.pos()
                    point[0]=QC.QPoint(120,140); canvas.BeginDrag()
                    cases.append(dict(anchor=anchor,touch_override=touch,moves=moves,ordinary_delta=[after.x()-before.x(),after.y()-before.y()],new_drag_touch=canvas._current_drag_is_touch,saved={k:options.GetBoolean(k) for k in keys[:2]}))
            return dict(initial=initial,cases=cases)
        finally:
            ClientGUIFunctions.GetMousePos=old_mouse;QG.QCursor.setPos=old_warp
            for k,v in old.items():options.SetBoolean(k,v)
            panel.deleteLater();frame.hide();frame.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui,qt)
def main():
    import hydrus_driver
    if len(sys.argv)>1:
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)));return
    with tempfile.TemporaryDirectory() as folder:
        out=Path(folder)/'result.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    (HERE/'fixtures/viewer_anchor_options.json').write_text(json.dumps(result,indent=2)+'\n')
    print('wrote viewer_anchor_options.json')
if __name__=='__main__':main()
