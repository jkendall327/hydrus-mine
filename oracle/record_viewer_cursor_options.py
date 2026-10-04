#!/usr/bin/env python3
"""Record actual cursor autohide checks, movement, focus and real menu exec.

Drive the NoneableSpinCtrl and a real displayed local JPEG canvas. Freeze only
HydrusTime.GetNowFloat; observe actual QCursor shapes/QTimer intervals from
_HideCursorCheck and HandleMouseMoveWithoutEvent. The menu case runs the real
ClientGUICore.PopupMenu with an actual QMenu and checks inside its nested loop.
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
        from hydrus.core import HydrusTime
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIFunctions, ClientGUICore
        from hydrus.client.gui.panels.options import MediaViewerPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        controller=session.controller
        options=controller.new_options
        key='media_viewer_cursor_autohide_time_ms'
        before=options.GetNoneableInteger(key)
        old_hide=options.GetBoolean('hide_canvas_drags')
        old_can=CC.CAN_HIDE_MOUSE
        panel=MediaViewerPanel.MediaViewerPanel(controller.gui)
        control=panel._media_viewer_cursor_autohide_time_ms
        initial=control.GetValue()
        bounds=[control._number_value.minimum(),control._number_value.maximum()]
        none_phrase=control._checkbox.text()
        with open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')) as stream:manifest=json.load(stream)
        file_hash=next(bytes.fromhex(file['hash']) for file in manifest['files'] if file['name']=='jpeg_00.jpg')
        frame=ClientGUICanvasFrame.CanvasFrame(controller.gui)
        canvas=ClientGUICanvas.CanvasMediaListBrowser(frame,os.urandom(32),ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),controller.Read('media_results',[file_hash]),file_hash)
        frame.SetCanvas(canvas);frame.showNormal()
        QW.QApplication.processEvents()
        canvas.SetMedia(canvas._media_list.GetMediaByHashes({file_hash})[0])
        assert canvas.GetMedia() is not None
        other=QW.QWidget(None);other.show()
        clock=[1000.0]
        position=[QC.QPoint(500,500)]
        old_now=HydrusTime.GetNowFloat
        old_mouse=ClientGUIFunctions.GetMousePos
        HydrusTime.GetNowFloat=lambda:clock[0]
        ClientGUIFunctions.GetMousePos=lambda:canvas.mapToGlobal(position[0])
        def focus(target):
            target.activateWindow();QW.QApplication.processEvents()
            QW.QApplication.setActiveWindow(target)
            assert QW.QApplication.activeWindow() is target
        def state(phase):
            return {'phase':phase,'now_ms':round((clock[0]-1000)*1000),
                    'touch_ms':round((canvas._last_cursor_autohide_touch_time-1000)*1000),
                    'blank':canvas.cursor().shape()==QC.Qt.CursorShape.BlankCursor,
                    'interval_ms':canvas._cursor_autohide_timer.interval(),
                    'timer_active':canvas._cursor_autohide_timer.isActive()}
        def check(phase,ms):
            clock[0]=1000+ms/1000
            canvas._HideCursorCheck()
            result=state(phase)
            canvas._cursor_autohide_timer.stop()
            return result
        events=[]
        menu_events=[]
        movement=[]
        try:
            for value in (None,100,700,1250,100000):
                control.SetValue(value);panel.UpdateOptions()
                focus(frame);CC.CAN_HIDE_MOUSE=True
                canvas.setCursor(QG.QCursor(QC.Qt.CursorShape.ArrowCursor))
                clock[0]=1000;canvas._RestartCursorHideWait();canvas._cursor_autohide_timer.stop()
                deadline=value or 700
                traces=[check('initial',0),check('at-deadline',deadline),check('past-deadline',deadline+1)]
                focus(other)
                traces.append(check('inactive',deadline+2))
                focus(frame)
                traces.append(check('after-refocus',deadline+3))
                traces.append(check('past-refocus-deadline',deadline*2+3))
                CC.CAN_HIDE_MOUSE=False
                traces.append(check('ineligible-widget',deadline*2+4))
                CC.CAN_HIDE_MOUSE=True
                traces.append(check('after-eligible',deadline*2+5))
                events.append({'value':value,'trace':traces})
            control.SetValue(700);panel.UpdateOptions();focus(frame);CC.CAN_HIDE_MOUSE=True
            clock[0]=1000;canvas._RestartCursorHideWait();canvas._cursor_autohide_timer.stop()
            menu_events.append(check('idle-hidden',701))
            menu=QW.QMenu(frame);menu.addAction('synthetic cursor menu')
            def inside_menu():
                assert ClientGUICore.core().MenuIsOpen()
                menu_events.append(check('inside-real-menu',800))
                menu.close()
            QC.QTimer.singleShot(0,inside_menu)
            ClientGUICore.core().PopupMenu(canvas,menu)
            assert not ClientGUICore.core().MenuIsOpen()
            focus(frame)
            menu_events.append(check('menu-just-closed',801))
            menu_events.append(check('after-menu-deadline',1501))
            # Actual canvas movement restores arrow and restarts at100ms;
            # a hidden drag stops the timer and retains blank on release.
            clock[0]=1002
            position[0]=QC.QPoint(510,510)
            canvas.HandleMouseMoveWithoutEvent(False)
            movement.append(state('ordinary-move'))
            canvas._cursor_autohide_timer.stop()
            options.SetBoolean('hide_canvas_drags',True)
            canvas._last_drag_pos=QC.QPoint(position[0])
            position[0]=QC.QPoint(520,520)
            canvas.HandleMouseMoveWithoutEvent(True)
            movement.append(state('hidden-drag'))
            position[0]=QC.QPoint(530,530)
            canvas.HandleMouseMoveWithoutEvent(False)
            movement.append(state('after-drag-move'))
            canvas._cursor_autohide_timer.stop()
        finally:
            HydrusTime.GetNowFloat=old_now
            ClientGUIFunctions.GetMousePos=old_mouse
            options.SetNoneableInteger(key,before)
            options.SetBoolean('hide_canvas_drags',old_hide)
            CC.CAN_HIDE_MOUSE=old_can
            panel.deleteLater();frame.hide();frame.deleteLater();other.hide();other.deleteLater()
        return {'hash':file_hash.hex(),'initial':initial,'bounds':bounds,'none_phrase':none_phrase,
                'events':events,'menu':menu_events,'movement':movement}
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
    path=os.path.join(HERE,'fixtures','viewer_cursor_options.json')
    with open(path,'w') as stream:json.dump(result,stream,indent=2);stream.write('\n')
    print('wrote '+path)


if __name__=='__main__':main()
