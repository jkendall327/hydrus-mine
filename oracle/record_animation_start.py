#!/usr/bin/env python3
"""Record real staged percent controls and fresh/reused Animation.SetMedia.

The native Qt Animation widget remains hidden for metadata edge cases so its
actual SetMedia ordering can be observed without scheduling invalid decodes.
A valid reused WebP also paints through the real RasterContainerVideo reader.
GIF/video MPV startup does not consume this option; no MPV seek is invented.
"""
import json
import os
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def qt():
        from PIL import Image
        from qtpy import QtWidgets as QW
        import yaml
        from hydrus.core import HydrusConstants as HC, HydrusTime
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasMedia
        from hydrus.client.gui.panels.options import MediaPlaybackPanel
        from hydrus.client.media import ClientMediaSingle

        c = session.controller
        key = 'animation_start_position'
        old = HC.options[key]
        initial_panel = MediaPlaybackPanel.MediaPlaybackPanel(c.gui)
        defaults = {'raw': old, 'display': initial_panel._animation_start_position.value(),
                    'minimum': initial_panel._animation_start_position.minimum(),
                    'maximum': initial_panel._animation_start_position.maximum(),
                    'enabled': initial_panel._animation_start_position.isEnabled()}
        initial_panel.deleteLater()
        controls = []
        manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
        h = next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name'] == 'webp_animated.webp')
        original = c.Read('media_results', [h])[0]
        canvas = ClientGUICanvas.Canvas(c.gui, ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))
        generator = ClientGUICanvas.CanvasBackgroundColourGenerator(canvas)
        animations = []
        saved_path = c.client_files_manager.GetFilePath
        saved_clock = HydrusTime.GetNowPrecise
        serial = [1]

        def media(count, mime=None):
            value = original.Duplicate()
            info = value.GetFileInfoManager().Duplicate()
            info.hash = serial[0].to_bytes(32, 'big')
            serial[0] += 1
            info.num_frames = count
            if mime is not None:
                info.mime = mime
            value.SetFileInfoManager(info)
            return ClientMediaSingle.MediaSingle(value)

        def animation():
            result = ClientGUICanvasMedia.Animation(canvas, CC.CANVAS_MEDIA_VIEWER, generator)
            result.resize(80, 60)
            animations.append(result)
            return result

        try:
            for raw, typed, apply in [(0.119, 72, False), (.119, 72, True), (-.1, None, True),
                                       (0, None, True), (.119, None, True),
                                       (.3333, None, True), (.999, None, True),
                                       (1, None, True), (1.2, None, True)]:
                HC.options[key] = raw
                panel = MediaPlaybackPanel.MediaPlaybackPanel(c.gui)
                displayed = panel._animation_start_position.value()
                if typed is not None:
                    panel._animation_start_position.setValue(typed)
                if apply:
                    panel.UpdateOptions()
                reopened = MediaPlaybackPanel.MediaPlaybackPanel(c.gui)
                controls.append({'raw': raw, 'typed': typed, 'apply': apply,
                                 'displayed': displayed, 'saved': HC.options[key],
                                 'reopened': reopened._animation_start_position.value()})
                panel.deleteLater()
                reopened.deleteLater()
            states = []
            for fraction in [0, .119, .3333, .5, .999, 1]:
                HC.options[key] = fraction
                for previous, count in [(None, 4), (4, 4), (3, 4), (0, 4),
                                        (None, 0), (None, None), (5, 2)]:
                    widget = animation()
                    if previous is not None:
                        widget.SetMedia(media(previous), start_paused=True)
                    previous_count = widget.GetNumFrames()
                    widget.SetMedia(media(count), start_paused=True)
                    states.append({'fraction': fraction, 'previous': previous,
                                   'previous_count': previous_count, 'new_count': count,
                                   'frames': widget.GetNumFrames(), 'index': widget.CurrentFrame(),
                                   'paused': widget._paused, 'timestamp': widget._current_timestamp_ms})
                    widget.ClearMedia()
            HC.options[key] = .5
            reset = animation()
            reset.SetMedia(media(4), start_paused=True)
            reset.ClearMedia()
            reset.SetMedia(media(4), start_paused=True)
            after_clear = {'index': reset.CurrentFrame(), 'frames': reset.GetNumFrames(), 'paused': reset._paused}
            reset.ClearMedia()
            with tempfile.TemporaryDirectory() as directory:
                path = str(Path(directory) / 'three-frames.webp')
                frames = [Image.new('RGB', (24, 20), rgb) for rgb in [(240, 10, 20), (10, 240, 20), (10, 20, 240)]]
                frames[0].save(path, format='WEBP', save_all=True, append_images=frames[1:], duration=[120, 240, 360], loop=0, lossless=True)
                c.client_files_manager.GetFilePath = lambda *args, **kwargs: path
                widget = animation()
                widget.SetMedia(media(3), start_paused=True)
                widget.SetMedia(media(3), start_paused=True)
                widget.show()
                HydrusTime.GetNowPrecise = lambda: 100.0
                deadline = time.monotonic() + 5
                while not widget._current_frame_drawn and time.monotonic() < deadline:
                    widget._TryToDrawCanvasBitmap()
                    QW.QApplication.processEvents()
                    time.sleep(.005)
                assert widget._current_frame_drawn, 'real WebP reader did not supply initial frame'
                rendered = {'index': widget.CurrentFrame(), 'status': widget.GetAnimationBarStatus(),
                            'paused': widget._paused, 'bytes': Path(path).read_bytes().hex()}
                pixmap = widget.grab()
                rendered['pixel'] = pixmap.toImage().pixelColor(40, 30).getRgb()
                pixmap.save(str(HERE / 'fixtures/animation_start_qt.png'))
                widget.ClearMedia()
                widget.hide()
            c.client_files_manager.GetFilePath = lambda *args, **kwargs: str(HERE / 'fixtures/media/ugoira_json.zip')
            widget = animation()
            widget.SetMedia(media(5, HC.ANIMATION_UGOIRA), start_paused=True)
            widget.SetMedia(media(5, HC.ANIMATION_UGOIRA), start_paused=True)
            widget.show()
            deadline = time.monotonic() + 5
            while not widget._current_frame_drawn and time.monotonic() < deadline:
                widget._TryToDrawCanvasBitmap()
                QW.QApplication.processEvents()
                time.sleep(.005)
            assert widget._current_frame_drawn, 'real ugoira reader did not supply initial frame'
            pixmap = widget.grab()
            ugoira = {'index': widget.CurrentFrame(), 'status': widget.GetAnimationBarStatus(),
                      'paused': widget._paused, 'pixel': pixmap.toImage().pixelColor(40, 30).getRgb()}
            pixmap.save(str(HERE / 'fixtures/animation_start_ugoira_qt.png'))
            widget.ClearMedia()
            widget.hide()
            HC.options[key] = .619
            legacy = yaml.safe_dump({key: HC.options[key]})
            return {'defaults': defaults, 'controls': controls, 'states': states,
                    'after_clear': after_clear, 'rendered': rendered, 'ugoira': ugoira, 'legacy_yaml': legacy,
                    'limits': 'SetMedia samples previous widget frame count before replacing it; fresh/cleared widgets use1. Metadata0/None becomes1 afterward. The actual valid reused WebP decoded and stayed paused. MPV/Qt-media-player consumers do not read this setting. Hidden metadata edge cases do not drive invalid decoder requests.'}
        finally:
            HC.options[key] = old
            c.client_files_manager.GetFilePath = saved_path
            HydrusTime.GetNowPrecise = saved_clock
            for widget in animations:
                widget.ClearMedia()
                c.gui.UnregisterAnimationUpdateWindow(widget)
                widget.hide()
                widget.deleteLater()
            canvas.deleteLater()

    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    output = HERE / 'fixtures/animation_start.json'
    output.write_text(json.dumps(result, indent=2) + '\n')
    print('wrote ' + str(output))


if __name__ == '__main__':
    main()
