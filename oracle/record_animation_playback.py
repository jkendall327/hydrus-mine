#!/usr/bin/env python3
"""Record the reference's own animation player on a frozen clock.

The real `Animation` widget (the player for ugoiras and animated WebP) and the
real `AnimationBar` play two real files (`ugoira_json.zip`, `webp_anim.webp`;
their metadata from the reference's own `GetFileInfo`):

- `timeline`: playing from the first frame through two loops, one tick at the
  moment each frame is due; the bar's status and text after each;
- `goto`: `GotoFrame` to frames, the same frame, the ends; and unpausing;
- `seek_delta`: `SeekDelta` by amounts forwards and back, paused and playing;
- `frame_step`: the media container's real `GotoPreviousOrNextFrame`;
- `scan`: the real `AnimationBar` pressed, dragged and released at positions;
- `stop_for_slideshow`: coming round to the first frame with it set.

Time is held still (`HydrusTime.GetNowPrecise`), and each step waits for the
reference's render thread to have the frame it wants, so the order is the
logic's, not the threads'. The bar's text is made with the calls its
`_Redraw` makes. The statuses are `GetAnimationBarStatus()`.
"""
import json
import sys
import tempfile
import time
import types
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

FILES = [('ugoira', 'ugoira_json.zip'), ('webp', 'webp_anim.webp')]
BAR_WIDTH = 210


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.core import HydrusConstants as HC, HydrusNumbers, HydrusTime
        from hydrus.core.files import HydrusFileHandling
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasMedia
        from hydrus.client.media import ClientMediaSingle

        c = session.controller
        manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
        h = next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name'] == 'webp_animated.webp')
        original = c.Read('media_results', [h])[0]
        canvas = ClientGUICanvas.Canvas(c.gui, ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))
        generator = ClientGUICanvas.CanvasBackgroundColourGenerator(canvas)
        canvas.resize(300, 200)
        canvas.show()  # (an Animation only updates while visible)
        saved_path = c.client_files_manager.GetFilePath
        saved_clock = HydrusTime.GetNowPrecise
        saved_mouse = ClientGUIFunctions.GetMousePos
        clock = [1000.0]
        HydrusTime.GetNowPrecise = lambda: clock[0]
        widgets = []
        serial = [100]
        nub = c.new_options.GetInteger('animated_scanbar_nub_width')

        def media_for(path, mime):
            (size, mime, width, height, duration_ms, num_frames, has_audio, num_words) = \
                HydrusFileHandling.GetFileInfo(path, mime)
            value = original.Duplicate()
            info = value.GetFileInfoManager().Duplicate()
            info.hash = serial[0].to_bytes(32, 'big')
            serial[0] += 1
            info.mime = mime
            info.size = size
            info.width = width
            info.height = height
            info.duration_ms = duration_ms
            info.num_frames = num_frames
            info.has_audio = has_audio
            value.SetFileInfoManager(info)
            return ClientMediaSingle.MediaSingle(value)

        def settle(widget):
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                QW.QApplication.processEvents()
                if widget._current_frame_drawn:
                    return
                widget._TryToDrawCanvasBitmap()
                time.sleep(.003)
            raise AssertionError('the reference never drew the frame wanted: index %r, container frames %r, buffer %r' % (
                widget._current_frame_index, sorted(widget._video_container._frames), widget._video_container.GetBufferIndices()))

        def new_animation(media, paused):
            widget = ClientGUICanvasMedia.Animation(canvas, CC.CANVAS_MEDIA_VIEWER, generator)
            widget.resize(80, 60)
            widgets.append(widget)
            widget.show()
            widget.SetMedia(media, start_paused=paused)
            deadline = time.monotonic() + 10
            settle(widget)
            while not widget._video_container.IsInitialised() and time.monotonic() < deadline:
                time.sleep(.003)
            bar = ClientGUICanvasMedia.AnimationBar(canvas)
            bar.resize(BAR_WIDTH, 20)
            widgets.append(bar)
            bar.SetMediaAndWindow(media, widget)
            return widget, bar

        def status(widget, bar):
            (index, timestamp, paused, buffer) = widget.GetAnimationBarStatus()
            parts = []
            if bar._num_frames > 1:
                parts.append(HydrusNumbers.ValueRangeToPrettyString(index + 1, bar._num_frames))
            if timestamp is not None:
                parts.append(HydrusTime.ValueRangeToScanbarTimestampsMS(timestamp, bar._duration_ms))
            return {'index': index, 'timestamp_ms': timestamp, 'paused': paused,
                    'played_through': widget.HasPlayedOnceThrough(), 'text': ' - '.join(parts)}

        def close(widget, bar):
            widget.hide()
            QW.QApplication.processEvents()
            bar.ClearMedia()
            widget.ClearMedia()

        out = {}
        try:
            for (name, filename) in FILES:
                path = str(HERE / 'fixtures/media' / filename)
                c.client_files_manager.GetFilePath = lambda *args, _p=path, **kwargs: _p
                mime = HydrusFileHandling.GetMime(path)
                media = media_for(path, mime)
                n = media.GetNumFrames()
                case = {'file': filename, 'mime': mime, 'num_frames': n,
                        'duration_ms': media.GetDurationMS(), 'bar_width': BAR_WIDTH, 'nub_width': nub}

                # playing from the first frame, a tick as each is due
                widget, bar = new_animation(media, True)
                case['initial'] = status(widget, bar)
                case['durations_ms'] = [widget._video_container.GetDurationMS(i) for i in range(n)]
                widget.Play()
                timeline = []
                for _ in range(2 * n + 2):
                    clock[0] = widget._next_frame_due_at + 0.001
                    widget.TIMERAnimationUpdate()
                    settle(widget)
                    step = status(widget, bar)
                    step['due_in_ms'] = round((widget._next_frame_due_at - clock[0]) * 1000)
                    timeline.append(step)
                case['timeline'] = timeline
                close(widget, bar)

                # going to a frame
                widget, bar = new_animation(media, True)
                goto = []
                for (index, play_first, pause_afterwards) in [
                        (n - 1, False, True), (n // 2, False, True), (n // 2, False, True), (0, False, True),
                        (1, False, True), (n - 1, False, True), (0, False, True),
                        (n - 2, True, True), (1, True, False), (n - 2, False, False)]:
                    if play_first:
                        widget.Play()
                    widget.GotoFrame(index, pause_afterwards=pause_afterwards)
                    settle(widget)
                    goto.append({'index': index, 'play_first': play_first,
                                 'pause_afterwards': pause_afterwards, **status(widget, bar)})
                case['goto'] = goto
                close(widget, bar)

                # seeking by time, from the frame before the last
                seeks = []
                for start_paused in (True, False):
                    widget, bar = new_animation(media, start_paused)
                    widget.GotoFrame(n - 2, pause_afterwards=start_paused)
                    settle(widget)
                    status(widget, bar)  # (a timestamp, as the bar asks for it)
                    seeks.append({'start': status(widget, bar), 'steps': []})
                    for (direction, ms) in [(-1, 50), (-1, 10), (1, 10), (1, 100), (-1, 2500), (-1, 1),
                                            (1, 140), (1, 5000), (1, 1), (-1, 1000000), (1, 300),
                                            (1, 0), (-1, 0)]:
                        widget.SeekDelta(direction, ms)
                        settle(widget)
                        status(widget, bar)
                        seeks[-1]['steps'].append({'direction': direction, 'ms': ms, **status(widget, bar)})
                    close(widget, bar)
                case['seek_delta'] = seeks

                # the media container's frame step (ctrl+b, ctrl+n)
                widget, bar = new_animation(media, False)
                container = types.SimpleNamespace(_media=media, _media_window=widget,
                                                  _show_action=CC.MEDIA_VIEWER_ACTION_SHOW_WITH_NATIVE)
                steps = []
                for direction in [-1, 1, 1, 1, -1, -1] + [1] * (n - 1) + [1, 1]:
                    ClientGUICanvasMedia.MediaContainer.GotoPreviousOrNextFrame(container, direction)
                    settle(widget)
                    status(widget, bar)
                    steps.append({'direction': direction, **status(widget, bar)})
                case['frame_step'] = steps
                close(widget, bar)

                # the bar pressed, dragged and released
                scans = []
                for start_paused in (True, False):
                    widget, bar = new_animation(media, start_paused)
                    for x in [nub / 2, nub / 2 + 0.25 * (BAR_WIDTH - nub), nub / 2 + 0.6 * (BAR_WIDTH - nub),
                              nub / 2 + 0.65 * (BAR_WIDTH - nub), nub / 2 + 0.75 * (BAR_WIDTH - nub),
                              BAR_WIDTH - nub / 2, 500, -20, 0, 1]:
                        ClientGUIFunctions.GetMousePos = lambda _x=x: bar.mapToGlobal(QC.QPoint(int(_x), 10))
                        bar.mousePressEvent(None)
                        settle(widget)
                        status(widget, bar)
                        pressed = status(widget, bar)
                        bar.mouseReleaseEvent(None)
                        status(widget, bar)
                        scans.append({'x': x, 'start_paused': start_paused, 'pressed': pressed,
                                      'released': status(widget, bar)})
                    ClientGUIFunctions.GetMousePos = saved_mouse
                    close(widget, bar)
                case['scan'] = scans

                # coming round with the slideshow wanting it to stop
                widget, bar = new_animation(media, True)
                widget.StopForSlideshow(True)
                widget.GotoFrame(n - 1)
                settle(widget)
                widget.Play()
                stops = [status(widget, bar)]
                for _ in range(3):
                    clock[0] = widget._next_frame_due_at + 0.001
                    widget.TIMERAnimationUpdate()
                    QW.QApplication.processEvents()
                    if not widget._current_frame_drawn:
                        settle(widget)
                    stops.append(status(widget, bar))
                case['stop_for_slideshow'] = stops
                close(widget, bar)
                out[name] = case
            return {**out,
                    'limits': ('Time is held still; each step waits for the reference render thread. '
                               'The bar text is made with the calls AnimationBar._Redraw makes, from '
                               'the real Animation.GetAnimationBarStatus(). Frame step runs the real '
                               'MediaContainer.GotoPreviousOrNextFrame on a stand-in for self holding '
                               'the media, window and show action. Scans press and release the real '
                               'AnimationBar with the mouse position scripted.')}
        finally:
            c.client_files_manager.GetFilePath = saved_path
            HydrusTime.GetNowPrecise = saved_clock
            ClientGUIFunctions.GetMousePos = saved_mouse
            for widget in widgets:
                try:
                    widget.hide()
                    widget.deleteLater()
                except Exception:
                    pass
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
    output = HERE / 'fixtures/animation_playback.json'
    output.write_text(json.dumps(result, indent=2) + '\n')
    print('wrote ' + str(output))


if __name__ == '__main__':
    main()
