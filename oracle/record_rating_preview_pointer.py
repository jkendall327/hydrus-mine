#!/usr/bin/env python3
"""Real numerical preview press/held-drag/fraction routes on a copied fixture.

Each opening fraction side (none/left/right) gets a detached service with five
stars, allow-zero enabled. Real QWidget mouse events are dispatched through Qt;
no conversion, event handler or paint method is replaced. Press outside clears,
drag outside preserves, ordinary motion after release does not change samples.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'rating_preview_pointer.json')

def record(session):
    def work():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW, QtTest as QT
        from hydrus.client import ClientServices
        from hydrus.client.gui import ClientGUIRatings as R
        from hydrus.client.gui.services import ClientGUIClientsideServices as S
        from hydrus.core import HydrusConstants as HC
        c = session.controller
        registered = c.services_manager.GetServices((HC.LOCAL_RATING_NUMERICAL,))[0]
        before = json.dumps(dict(registered.GetSerialisableDictionary()), sort_keys=True)
        cases = []
        for side in [0, 1, 2]:
            original = dict(registered.GetSerialisableDictionary())
            original.update(num_stars=5, allow_zero=True, custom_pad=3, show_fraction_beside_stars=side)
            service = ClientServices.GenerateService(registered.GetServiceKey(), registered.GetServiceType(), registered.GetName(), original)
            panel = S.EditClientServicePanel(c.gui, service)
            example = next(p for p in panel._panels if isinstance(p, S.EditServiceRatingSettingExamplePanel))
            widget = example._rating_example_display_thumbnails
            other = example._rating_example_display_media_viewer
            panel.resize(720, 900)
            panel.show()
            for _ in range(4): QW.QApplication.processEvents()
            events = []
            def event(action, x, held=False, right=False):
                pos = QC.QPointF(x, widget.height() / 2)
                if action == 'move':
                    button = QC.Qt.MouseButton.NoButton
                    buttons = QC.Qt.MouseButton.LeftButton if held else QC.Qt.MouseButton.NoButton
                    ev = QG.QMouseEvent(QC.QEvent.Type.MouseMove, pos, pos, button, buttons, QC.Qt.KeyboardModifier.NoModifier)
                    QW.QApplication.sendEvent(widget, ev)
                elif action == 'release':
                    QT.QTest.mouseRelease(widget, QC.Qt.MouseButton.LeftButton, pos=pos.toPoint())
                else:
                    QT.QTest.mousePress(widget, QC.Qt.MouseButton.RightButton if right else QC.Qt.MouseButton.LeftButton, pos=pos.toPoint())
                for _ in range(3): QW.QApplication.processEvents()
                events.append(dict(action=action, x=x, width=widget.width(), icon=widget._icon_size.width(), held=held, right=right, rating=widget.GetRating(), state=widget.GetRatingState(), fraction=R.GetNumericalFractionText(widget.GetRatingState(), R.GetStars(widget.GetServiceKey(), widget.GetRatingState(), widget.GetRating())[1]), other_state=other.GetRatingState()))
            event('press', 2)
            event('move', widget.width() * 0.85, held=True)
            if side == 2: example.grab().save(OUT.replace('.json', '.png'))
            event('move', -10, held=True)
            event('release', widget.width() * 0.85)
            event('move', widget.width() * 0.2)
            # Actual painted fraction is beside the fixed-width star graphics,
            # inside the full stretched QWidget hit area.
            painted_star_width = 5 * 12 + 4 * 3
            fraction_x = 3 if side == 1 else painted_star_width + 7
            event('press', fraction_x)
            event('release', fraction_x)
            event('press', widget.width() - 2)
            event('release', widget.width() - 2)
            event('press', 0)
            event('release', 0)
            event('press', widget.width() * 0.5, right=True)
            cases.append(dict(side=side, opening=original, events=events, example_value=example.GetValue()))
            panel.hide()
            panel.deleteLater()
        return dict(cases=cases, original_unchanged=json.dumps(dict(registered.GetSerialisableDictionary()), sort_keys=True) == before)
    return session.controller.CallBlockingToQt(session.controller.gui, work)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream: json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as stream: result = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()
