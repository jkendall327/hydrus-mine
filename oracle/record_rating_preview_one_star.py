#!/usr/bin/env python3
"""Record the real numerical preview's one-star boundary and saved normalization.

The imported service is duplicated with allow-zero disabled, without a commit.
Four actual sample controls receive centre clicks, then the numerical panel's
star count and allow-zero checkbox change. No preview/update/conversion method
is replaced. A fresh temporary basic DB is used.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'rating_preview_one_star.json')


def record(session):
    def work():
        from qtpy import QtCore as QC, QtWidgets as QW, QtTest as QT
        from hydrus.client.gui import ClientGUIRatings as R
        from hydrus.client.gui.services import ClientGUIClientsideServices as S
        from hydrus.client import ClientServices
        from hydrus.core import HydrusConstants as HC
        controller = session.controller
        service = controller.services_manager.GetServices((HC.LOCAL_RATING_NUMERICAL,))[0]
        registered = service
        registered_original = dict(registered.GetSerialisableDictionary())
        original = dict(registered_original)
        original['allow_zero'] = False
        service = ClientServices.GenerateService(service.GetServiceKey(), service.GetServiceType(), service.GetName(), original)
        panel = S.EditClientServicePanel(controller.gui, service)
        numerical = next(p for p in panel._panels if isinstance(p, S.EditServiceRatingsNumericalSubPanel))
        example = next(p for p in panel._panels if isinstance(p, S.EditServiceRatingSettingExamplePanel))
        widgets = [getattr(example, '_rating_example_display_' + n)
                   for n in ('thumbnails', 'media_viewer', 'preview_window', 'dialog')]
        panel.resize(720, 900)
        panel.show()

        def settle():
            for _ in range(4):
                QW.QApplication.processEvents()

        def state(action):
            settle()
            configured = panel.GetValue().GetSerialisableDictionary()
            return dict(action=action, num_stars=numerical._num_stars.value(),
                        checkbox_allow_zero=numerical._allow_zero.isChecked(),
                        saved_allow_zero=configured['allow_zero'],
                        live_allow_zero=example._rating_example_service._modifiable_dict['allow_zero'],
                        samples=[dict(rating=w.GetRating(), state=w.GetRatingState(),
                                      fraction=R.GetNumericalFractionText(w.GetRatingState(), R.GetStars(w.GetServiceKey(), w.GetRatingState(), w.GetRating())[1]))
                                 for w in widgets])

        try:
            settle()
            for widget in widgets:
                QT.QTest.mouseClick(widget, QC.Qt.MouseButton.LeftButton,
                                   pos=QC.QPoint(widget.width() // 2, widget.height() // 2))
            events = [state('centre_clicks')]
            numerical._num_stars.setValue(1)
            events.append(state('one_star'))
            numerical._allow_zero.setChecked(True)
            events.append(state('allow_zero_checked'))
            numerical._num_stars.setValue(7)
            events.append(state('seven_stars'))
            numerical._allow_zero.setChecked(False)
            events.append(state('allow_zero_unchecked'))
            return dict(name=service.GetName(), opening_num_stars=original['num_stars'],
                        opening_allow_zero=original['allow_zero'], events=events,
                        original_unchanged=dict(registered.GetSerialisableDictionary()) == registered_original)
        finally:
            panel.hide()
            panel.deleteLater()

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
