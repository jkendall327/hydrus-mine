#!/usr/bin/env python3
"""Record real focus controls and actual Qt activation-dependent pop-in gates.

Drive MediaViewerPanel and MediaViewerHoversPanel UpdateOptions, activate real
synthetic Qt top-level windows, and run the real canvas seek-bar/hover show-hide
consumers at controlled pointer positions. No activeWindow mocking; assert the
actual Qt active top-level before recording each state. Synthetic basic fixture.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.panels.options import MediaViewerPanel, MediaViewerHoversPanel
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        controller = session.controller
        options = controller.new_options
        keys = ('animated_scanbar_pop_in_requires_focus', 'hover_windows_need_window_focus_to_pop_in',
                'draw_tags_hover_in_media_viewer_background', 'draw_top_hover_in_media_viewer_background',
                'draw_top_right_hover_in_media_viewer_background', 'draw_notes_hover_in_media_viewer_background',
                'force_animation_scanbar_show')
        before = {key: options.GetBoolean(key) for key in keys}
        panel = MediaViewerPanel.MediaViewerPanel(controller.gui)
        hovers = MediaViewerHoversPanel.MediaViewerHoversPanel(controller.gui)
        initial = [panel._animated_scanbar_pop_in_requires_focus.isChecked(), hovers._hover_windows_need_window_focus_to_pop_in.isChecked()]
        with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream: manifest = json.load(stream)
        file_hash = next(bytes.fromhex(file['hash']) for file in manifest['files'] if file['name'] == 'gif_animated.gif')
        results = controller.Read('media_results', [file_hash])
        results[0].GetNotesManager().SetNamesToNotes({'details': 'synthetic focus note'})
        frame = ClientGUICanvasFrame.CanvasFrame(controller.gui)
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, results, file_hash)
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 750)
        other = QW.QWidget(None)
        other.setWindowTitle('synthetic other focus window')
        other.show()
        QW.QApplication.processEvents()
        canvas.SetMedia(canvas._media_list.GetMediaByHashes({file_hash})[0])
        assert canvas.GetMedia() is not None and canvas.GetMedia().GetHash() == file_hash
        container = canvas._media_container
        windows = (canvas._top_hover, canvas._tags_hover, canvas._top_right_hover, canvas._right_notes_hover)
        old_mouse = ClientGUIFunctions.GetMousePos
        position = [QC.QPoint(500, 700)]
        ClientGUIFunctions.GetMousePos = lambda: canvas.mapToGlobal(position[0])
        def activate(target):
            target.activateWindow()
            QW.QApplication.processEvents()
            # Apply real Qt activation after settling queued media-widget focus
            # requests; no activeWindow function or consumer is mocked.
            QW.QApplication.setActiveWindow(target)
            assert QW.QApplication.activeWindow() is target
        events = []
        transient = []
        try:
            for seek_focus in (False, True):
                for hover_focus in (False, True):
                    panel._animated_scanbar_pop_in_requires_focus.setChecked(seek_focus)
                    hovers._hover_windows_need_window_focus_to_pop_in.setChecked(hover_focus)
                    panel.UpdateOptions()
                    hovers.UpdateOptions()
                    for key in keys[2:]: options.SetBoolean(key, False)
                    for active in (False, True):
                        for window in windows: window._LowerHover()
                        target = frame if active else other
                        activate(target)
                        full_rect = container.GetIdealControlsBarRect(True)
                        position[0] = container.mapTo(canvas, full_rect.center())
                        container._ShowHideControlBar()
                        seek_full = container._controls_bar_show_full
                        seek_height = container._controls_bar.height()
                        hover_states = []
                        for window in windows:
                            window._SizeAndPosition()
                            _, size, origin = window._GetIdealSizeAndPosition()
                            position[0] = origin + QC.QPoint(max(size.width(), 50)//2, max(size.height(), 50)//2)
                            window.DoRegularHideShow()
                            hover_states.append(window._is_currently_up)
                            window._LowerHover()
                            # LowerHover restores widget focus; assert the top-level is unchanged.
                            assert QW.QApplication.activeWindow() is target
                        events.append({'seek_requires_focus': seek_focus, 'hover_requires_focus': hover_focus,
                                       'active': active, 'seek_full': seek_full, 'seek_height': seek_height,
                                       'hover_up': hover_states})
            # An already-raised hover remains raised with no active Qt top-level,
            # but a new hover must not rise; another own-app top-level hides it.
            hovers._hover_windows_need_window_focus_to_pop_in.setChecked(True)
            hovers.UpdateOptions()
            for key in keys[2:]: options.SetBoolean(key, False)
            for label, window in zip(('top', 'tags', 'ratings', 'notes'), windows):
                for current in windows: current._LowerHover()
                activate(frame)
                window._SizeAndPosition()
                _, size, origin = window._GetIdealSizeAndPosition()
                over = origin + QC.QPoint(max(size.width(), 50)//2, max(size.height(), 50)//2)
                states = []
                def capture(phase):
                    window.DoRegularHideShow()
                    active = QW.QApplication.activeWindow()
                    states.append({'phase':phase, 'active':'viewer' if active is frame else 'other' if active is other else None,
                                   'up':window._is_currently_up})
                position[0] = over
                capture('viewer-over')
                QW.QApplication.setActiveWindow(None)
                assert QW.QApplication.activeWindow() is None
                capture('none-hold')
                position[0] = QC.QPoint(-100, -100)
                capture('none-away')
                position[0] = over
                capture('none-new-raise')
                activate(frame)
                capture('viewer-again')
                activate(other)
                capture('other-hide')
                transient.append({'hover':label, 'states':states})
        finally:
            ClientGUIFunctions.GetMousePos = old_mouse
            for key, value in before.items(): options.SetBoolean(key, value)
            panel.deleteLater()
            hovers.deleteLater()
            frame.hide()
            frame.deleteLater()
            other.hide()
            other.deleteLater()
        return {'initial': initial, 'events': events, 'hover_order': ['top', 'tags', 'ratings', 'notes'], 'transient':transient}
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
    path = os.path.join(HERE, 'fixtures', 'viewer_focus_options.json')
    with open(path, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + path)


if __name__ == '__main__': main()
