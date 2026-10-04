#!/usr/bin/env python3
"""Record all eight real browser eye-menu collapse combinations on one open viewer.

The actual MediaViewerHoversPanel writes the three controls. Capture the real
CanvasHoverFrameTop._ShowViewOptionsMenu menu, reopen the options page, and
discard another detached page to verify that editing alone changes nothing.
The reference is unmodified. Media comes from the basic imported fixture.
"""
import itertools
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUICore, ClientGUIMenus
        from hydrus.client.gui.panels.options import MediaViewerHoversPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        from record_viewer_menu import tree
        controller = session.controller
        options = controller.new_options
        keys = tuple('collapse_eye_menu_' + group for group in ('window', 'hovers', 'rendering'))
        before = [options.GetBoolean(key) for key in keys]
        def panel():
            page = MediaViewerHoversPanel.MediaViewerHoversPanel(controller.gui)
            controls = [getattr(page, '_' + key) for key in keys]
            return page, controls
        initial_page, controls = panel()
        initial = [control.isChecked() for control in controls]
        labels = [label.text() for label in initial_page.findChildren(QW.QLabel)
                  if label.text().startswith('Collapse ')]
        with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
            manifest = json.load(stream)
        hashes = [bytes.fromhex(file['hash']) for file in manifest['files'] if file['name'] == 'jpeg_00.jpg']
        media = controller.Read('media_results', hashes)
        frame = ClientGUICanvasFrame.CanvasFrame(controller.gui)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32),
            ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), media, hashes[0])
        frame.SetCanvas(canvas)
        frame.showNormal()
        QW.QApplication.processEvents()
        core = ClientGUICore.core()
        old_popup = core.PopupMenu
        captured = []
        def capture(widget, menu):
            ClientGUIMenus.RemoveFinalSeparator(menu)
            captured.append(tree(menu))
        core.PopupMenu = capture
        events = []
        try:
            for values in itertools.product((False, True), repeat=3):
                for control, value in zip(controls, values): control.setChecked(value)
                initial_page.UpdateOptions()
                reopened, reopened_controls = panel()
                captured.clear()
                canvas._top_hover._ShowViewOptionsMenu()
                events.append({'values': list(values), 'stored': [options.GetBoolean(key) for key in keys],
                    'reopened': [control.isChecked() for control in reopened_controls], 'menu': captured[-1]})
                reopened.deleteLater()
            cancelled, cancelled_controls = panel()
            for control in cancelled_controls: control.setChecked(False)
            cancelled.deleteLater()
            cancelled_values = [options.GetBoolean(key) for key in keys]
            initial_page.resize(980, 1100)
            initial_page.grab().save(os.path.join(HERE, 'fixtures', 'viewer_eye_menu.png'))
        finally:
            core.PopupMenu = old_popup
            for key, value in zip(keys, before): options.SetBoolean(key, value)
            initial_page.deleteLater()
            frame.hide()
            frame.deleteLater()
        return {'initial': initial, 'labels': labels, 'events': events, 'cancelled_values': cancelled_values}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream: json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream: result = json.load(stream)
    output = os.path.join(HERE, 'fixtures', 'viewer_eye_menu.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2, ensure_ascii=False)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__': main()
