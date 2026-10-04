#!/usr/bin/env python3
"""Record real Ratings Options sizes, save/cancel, controls and dialog consumers.

A temporary basic client drives all four actual DoubleSpinBoxes and their real
editingFinished example consumers. Real page UpdateOptions/reopen/serialization
and real Manage Ratings controls are captured. Actual held-button mouse events
check right-drag exclusion. No controller writes or rating handlers are replaced;
dialogs are cancelled, so existing file ratings are unchanged.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'rating_context_sizes.json')
KEYS = ['preview_window_rating_icon_size_px', 'preview_window_rating_incdec_height_px',
        'dialog_rating_icon_size_px', 'dialog_rating_incdec_height_px']


def record(session):
    def work():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW, QtTest as QT
        from hydrus.client import ClientConstants as CC
        from hydrus.client.media import ClientMediaSingle
        from hydrus.client.gui import ClientGUIDialogsManage as M, ClientGUIRatings as R
        from hydrus.client.gui.panels.options.RatingsPanel import RatingsPanel
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable
        controller = session.controller
        original = controller.new_options.Duplicate()
        panel = RatingsPanel(controller.gui, controller.new_options)
        panel.resize(900, 1100)
        panel.show()
        controls = [getattr(panel, '_' + key) for key in KEYS]
        labels = [label.text() for label in panel.findChildren(QW.QLabel)
                  if label.text().startswith(('Preview window like/', 'Preview window inc/', 'Dialogs like/', 'Dialogs inc/'))]
        initial = [controller.new_options.GetFloat(key) for key in KEYS]
        limits = [dict(minimum=w.minimum(), maximum=w.maximum(), decimals=w.decimals()) for w in controls]
        def settle():
            for _ in range(3):
                QW.QApplication.processEvents()
        def edit(values):
            for control, value in zip(controls, values):
                control.setValue(value)
                control.editingFinished.emit()
            settle()
        def consumers():
            result = []
            for canvas in (CC.CANVAS_PREVIEW, CC.CANVAS_DIALOG):
                items = []
                for kind, cls in ((HC.LOCAL_RATING_LIKE, R.RatingLikeDialog),
                                  (HC.LOCAL_RATING_NUMERICAL, R.RatingNumericalDialog),
                                  (HC.LOCAL_RATING_INCDEC, R.RatingIncDecDialog)):
                    service = controller.services_manager.GetServices((kind,))[0]
                    widget = cls(panel, service.GetServiceKey(), canvas)
                    if kind == HC.LOCAL_RATING_INCDEC:
                        widget.SetRating(12345)
                    items.append(dict(kind=kind, icon=[widget._icon_size.width(), widget._icon_size.height()]))
                    widget.deleteLater()
                result.append(items)
            return result
        examples = [panel._preview_window_star_example, panel._preview_window_incdec_example,
                    panel._dialog_star_example, panel._dialog_incdec_example]
        events = []
        try:
            for values in (initial, [31.75, 47.5, 26.9, 41.25], [1, 255, 6, 128],
                           [0, -10, 0, 0], [300, 300, 200, 200]):
                edit(values)
                before = [controller.new_options.GetFloat(key) for key in KEYS]
                staged = [w.value() for w in controls]
                example_sizes = [[w._icon_size.width(), w._icon_size.height()] for w in examples]
                panel.UpdateOptions()
                serialized = HydrusSerialisable.CreateFromSerialisableTuple(controller.new_options.GetSerialisableTuple())
                reopened = RatingsPanel(controller.gui, controller.new_options)
                events.append(dict(input=values, before=before, staged=staged,
                                   examples=example_sizes, saved=[controller.new_options.GetFloat(key) for key in KEYS],
                                   round_trip=[serialized.GetFloat(key) for key in KEYS],
                                   reopened=[getattr(reopened, '_' + key).value() for key in KEYS],
                                   consumers=consumers()))
                reopened.deleteLater()
            edit([31.75, 47.5, 26.9, 41.25])
            panel.UpdateOptions()
            panel.grab().save(OUT.replace('.json', '.png'))
            saved = [controller.new_options.GetFloat(key) for key in KEYS]
            cancelled = RatingsPanel(controller.gui, controller.new_options)
            for key in KEYS:
                getattr(cancelled, '_' + key).setValue(99)
            cancelled.deleteLater()
            with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
                hashes = [bytes.fromhex(json.load(stream)['files'][0]['hash'])]
            media = [ClientMediaSingle.MediaSingle(result) for result in controller.Read('media_results', hashes)]
            dialog = M.DialogManageRatings(controller.gui, media)
            dialog.show()
            settle()
            actual_dialog = []
            numerical = None
            for subpanel in dialog._panels:
                for key, widget in subpanel._service_keys_to_controls.items():
                    service = controller.services_manager.GetService(key)
                    actual_dialog.append(dict(kind=service.GetServiceType(), icon=[widget._icon_size.width(), widget._icon_size.height()]))
                    if isinstance(widget, R.RatingNumerical):
                        numerical = widget
            dragging = []
            for button in (QC.Qt.MouseButton.RightButton, QC.Qt.MouseButton.LeftButton):
                numerical.SetRating(0.4)
                QT.QTest.mousePress(numerical, button, pos=QC.QPoint(5, 5))
                before_drag = dict(state=numerical.GetRatingState(), rating=numerical.GetRating())
                event = QG.QMouseEvent(QC.QEvent.Type.MouseMove, QC.QPointF(numerical.width() * .8, 5),
                                      QC.Qt.MouseButton.NoButton, button, QC.Qt.KeyboardModifier.NoModifier)
                QW.QApplication.sendEvent(numerical, event)
                dragging.append(dict(button='right' if button == QC.Qt.MouseButton.RightButton else 'left',
                                     before=before_drag, after=dict(state=numerical.GetRatingState(), rating=numerical.GetRating())))
                QT.QTest.mouseRelease(numerical, button, pos=QC.QPoint(5, 5))
            dialog._cancel.click()
            dialog.deleteLater()
            return dict(keys=KEYS, labels=labels, initial=initial, limits=limits, events=events,
                        cancel_before=saved, cancel_after=[controller.new_options.GetFloat(key) for key in KEYS],
                        dialog=actual_dialog, dragging=dragging)
        finally:
            panel.hide()
            panel.deleteLater()
            controller.new_options = original
    return session.controller.CallBlockingToQt(session.controller.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as stream:
            result = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
