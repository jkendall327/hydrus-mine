#!/usr/bin/env python3
"""Actual local rating configuration example controls; temporary basic fixture only."""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'service_rating_preview.json')

def record(session):
    from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW, QtTest as QT
    from hydrus.client.gui import ClientGUIRatings as R
    from hydrus.client.gui.services import ClientGUIClientsideServices as S
    from hydrus.core import HydrusConstants as HC
    from hydrus.client.metadata import ClientRatings as CR
    c = session.controller
    def work():
        contexts = ['thumbnails', 'media_viewer', 'preview_window', 'dialog']
        keys = ['draw_thumbnail_rating_icon_size_px', 'media_viewer_rating_icon_size_px', 'preview_window_rating_icon_size_px', 'dialog_rating_icon_size_px']
        inc_keys = ['thumbnail_rating_incdec_height_px', 'media_viewer_rating_incdec_height_px', 'preview_window_rating_incdec_height_px', 'dialog_rating_incdec_height_px']
        defaults = dict(icon=[c.new_options.GetFloat(k) for k in keys], incdec=[c.new_options.GetFloat(k) for k in inc_keys])
        results = []
        for kind in [HC.LOCAL_RATING_LIKE, HC.LOCAL_RATING_NUMERICAL, HC.LOCAL_RATING_INCDEC]:
            service = c.services_manager.GetServices((kind,))[0]
            original = json.dumps(dict(service.GetSerialisableDictionary()), sort_keys=True)
            panel = S.EditClientServicePanel(c.gui, service)
            example = next(p for p in panel._panels if isinstance(p, S.EditServiceRatingSettingExamplePanel))
            generic = next(p for p in panel._panels if isinstance(p, S.EditServiceRatingsSubPanel))
            widgets = [getattr(example, '_rating_example_display_' + n) for n in contexts]
            panel.resize(720, 900)
            panel.show()
            def settle():
                for _ in range(3):
                    QW.QApplication.processEvents()
            def state():
                settle()
                return [dict(state=w.GetRatingState(), rating=w.GetRating() if hasattr(w, 'GetRating') else None, icon=[w._icon_size.width(), w._icon_size.height()], size=[w.width(), w.height()], click_num_stars=getattr(w, "_num_stars", None), click_allow_zero=getattr(w, "_allow_zero", None)) for w in widgets]
            events = [dict(action='initial', samples=state())]
            def click(index, button):
                w = widgets[index]
                QT.QTest.mouseClick(w, button, pos=QC.QPoint(max(2,w.width() // 2), max(2,w.height() // 2)))
                events.append(dict(action='right' if button == QC.Qt.MouseButton.RightButton else 'left', index=index, samples=state()))
            click(0, QC.Qt.MouseButton.LeftButton)
            click(0, QC.Qt.MouseButton.LeftButton)
            click(1, QC.Qt.MouseButton.RightButton)
            click(2, QC.Qt.MouseButton.LeftButton)
            click(3, QC.Qt.MouseButton.LeftButton)
            border, fill = generic._colour_ctrls[CR.LIKE]
            border.SetColour(QG.QColor(17,34,51))
            fill.SetColour(QG.QColor(68,85,102))
            fill.colourChanged.emit()
            settle()
            if kind != HC.LOCAL_RATING_INCDEC:
                star = next(p for p in panel._panels if isinstance(p, S.EditServiceStarRatingsSubPanel))
                star._shape.SetValue(40)
                star._UpdatePreviewExample()
                settle()
            if kind == HC.LOCAL_RATING_NUMERICAL:
                num = next(p for p in panel._panels if isinstance(p, S.EditServiceRatingsNumericalSubPanel))
                num._num_stars.setValue(7)
                num._custom_pad.setValue(2)
                num._draw_fraction.SetValue(CR.DRAW_ON_RIGHT)
                settle()
                # The next signal refreshes widget geometry using the queued service data.
                num._custom_pad.setValue(3)
                settle()
                click(3, QC.Qt.MouseButton.LeftButton)
                click(3, QC.Qt.MouseButton.RightButton)
            if kind == HC.LOCAL_RATING_INCDEC:
                # Middle click uses the real owned edit-value dialog, replacing only exec/decision.
                from hydrus.client.gui import ClientGUITopLevelWindowsPanels as TL
                old = TL.DialogEdit.exec
                def execute(dlg):
                    ctrl = dlg.findChild(QW.QSpinBox)
                    ctrl.setValue(12345)
                    return QW.QDialog.DialogCode.Accepted
                TL.DialogEdit.exec = execute
                try:
                    QT.QTest.mouseClick(widgets[3], QC.Qt.MouseButton.MiddleButton)
                finally:
                    TL.DialogEdit.exec = old
                events.append(dict(action='middle_accept', index=3, value=12345, samples=state()))
                old = TL.DialogEdit.exec
                TL.DialogEdit.exec = lambda dlg: QW.QDialog.DialogCode.Rejected
                try:
                    QT.QTest.mouseClick(widgets[3], QC.Qt.MouseButton.MiddleButton)
                finally:
                    TL.DialogEdit.exec = old
                events.append(dict(action='middle_cancel', index=3, samples=state()))
            events.append(dict(action='configured', samples=state(), rendered=[dict(fraction=R.GetNumericalFractionText(w.GetRatingState(), R.GetStars(w.GetServiceKey(), w.GetRatingState(), w.GetRating())[1])) if kind == HC.LOCAL_RATING_NUMERICAL else {} for w in widgets]))
            after = panel.GetValue().GetSerialisableDictionary()
            settle()
            example.grab().save(OUT.replace('.json', '_' + str(kind) + '.png'))
            results.append(dict(kind=kind, name=service.GetName(), labels=['Thumbnails','Media Viewer','Preview Window','Dialog (Default)'], events=events, live_dictionary=dict(example._rating_example_service._modifiable_dict), configured=dict(after), example_value=example.GetValue(), original_unchanged=json.dumps(dict(service.GetSerialisableDictionary()), sort_keys=True) == original))
            panel.hide()
            panel.deleteLater()
        return dict(default_sizes=defaults, results=results)
    return c.CallBlockingToQt(c.gui, work)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__':
    main()
