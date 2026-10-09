#!/usr/bin/env python3
"""Record the reference's duplicates filter right-hand hover and its pin option.

A private basic-fixture client puts the real `CanvasHoverFrameRightDuplicates`
(the hover the duplicates filter shows on the right) over a plain widget
standing in for the filter's canvas, at each of several canvas sizes, with a
pair of files given to it. With `hover_window_duplicates_always_on_top` on and
off, the hover's regular hide/show (`DoRegularHideShow`, which the filter's
timer calls) is run with the mouse at each of a set of points (the middle,
just inside and just outside each edge of where the hover would be, a corner of
the canvas), recording whether the hover is up afterwards and where it sits
(its rectangle against the canvas, in the hover's own `_GetIdealSizeAndPosition`).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_duplicates_hover_pin.py
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'duplicates_hover_pin.json')

CANVAS_SIZES = [(1200, 800), (800, 600), (3000, 1200), (500, 400), (1920, 1080)]


def record(session):
    controller = session.controller
    gui = controller.gui
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIFunctions
    from hydrus.client.gui.canvas import ClientGUICanvasHoverFrames as Hover

    options = controller.new_options
    original_mouse = ClientGUIFunctions.GetMousePos
    options.SetBoolean('hover_windows_need_window_focus_to_pop_in', False)

    class Canvas(QW.QWidget):

        def MouseIsNearAnimationBar(self):
            return False

        def GetCanvasType(self):
            return CC.CANVAS_MEDIA_VIEWER_DUPLICATES

    results = []
    for (cw, ch) in CANVAS_SIZES:
        for pinned in (True, False):
            options.SetBoolean('hover_window_duplicates_always_on_top', pinned)

            def run():
                window = QW.QWidget()
                window.resize(cw, ch)
                canvas = Canvas(window)
                canvas.resize(cw, ch)
                window.show()
                hover = Hover.CanvasHoverFrameRightDuplicates(canvas, canvas, os.urandom(32))
                hover._current_media = object()
                for _ in range(5):
                    QW.QApplication.processEvents()
                hover.adjustSize()
                (should_resize, size, pos) = hover._GetIdealSizeAndPosition()
                rect = QC.QRect(pos.x(), pos.y(), size.width(), size.height())
                hover.resize(size)
                points = [
                    ('middle', QC.QPoint(cw // 2, ch // 2)),
                    ('inside', rect.center()),
                    ('top-left', rect.topLeft()),
                    ('just left', rect.topLeft() + QC.QPoint(-1, 5)),
                    ('just above', rect.topLeft() + QC.QPoint(5, -1)),
                    ('bottom-right', rect.bottomRight()),
                    ('just below', rect.bottomRight() + QC.QPoint(-5, 1)),
                    ('canvas corner', QC.QPoint(cw - 1, 0)),
                    ('canvas bottom', QC.QPoint(cw - 5, ch - 1)),
                ]
                out = []
                for (name, point) in points:
                    hover.hide()
                    hover._is_currently_up = False
                    hover._position_initialised_since_last_media = False
                    ClientGUIFunctions.GetMousePos = lambda point=point: canvas.mapToGlobal(point)
                    try:
                        hover.DoRegularHideShow()
                        for _ in range(3):
                            QW.QApplication.processEvents()
                    finally:
                        ClientGUIFunctions.GetMousePos = original_mouse
                    out.append({'point': name, 'x': point.x(), 'y': point.y(), 'up': bool(hover._is_currently_up)})
                    # (once up, it stays while the mouse is over it, and goes with the mouse)
                result = {
                    'canvas': [cw, ch],
                    'pinned': pinned,
                    'ideal': [rect.x(), rect.y(), rect.width(), rect.height()],
                    'ideal_from_right': cw - rect.right() - 1,
                    'ideal_top_over_height': rect.y() / ch,
                    'points': out,
                }
                hover.deleteLater()
                window.close()
                return result

            results.append(qt(run))
    options.SetBoolean('hover_window_duplicates_always_on_top', True)
    return {'cases': results}


def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
