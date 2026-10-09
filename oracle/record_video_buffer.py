#!/usr/bin/env python3
"""Record how "Memory for video buffer" sizes the animation frame buffer.

Three things, from the real reference client:
- the options panel's BytesControl and its live "(about N frames of 720p
  video)" label as amounts and units are typed, and what apply saves;
- the frames RasterContainerVideo keeps behind and ahead of the playhead for
  a matrix of buffer sizes, resolutions, durations and frame counts;
- which frames its render thread decodes (each AnimationRendererPIL.read_frame)
  while a nine-frame WebP is played twice through, a frame at a time as the
  Animation widget asks for them, with a buffer that holds the whole loop and
  with buffers that do not. Each step waits for the render thread to finish
  what it has to do, so the order is the logic's, not the thread timing's.
"""
import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

FRAME_SIZE = (24, 20)
DURATIONS = [40, 50, 60, 50, 40, 50, 60, 50, 50]
COLOURS = [(240, 10, 20), (10, 240, 20), (10, 20, 240), (240, 240, 20), (20, 240, 240),
           (240, 20, 240), (120, 60, 30), (30, 120, 60), (60, 30, 120)]


def record(session):
    def qt():
        from PIL import Image
        from hydrus.core import HydrusConstants as HC, HydrusData
        from hydrus.client import ClientRendering, ClientVideoHandling
        from hydrus.client.gui.panels.options import SpeedAndMemoryPanel

        c = session.controller
        new_options = c.new_options
        old_size = new_options.GetInteger('video_buffer_size')

        # the options panel
        panel = SpeedAndMemoryPanel.SpeedAndMemoryPanel(c.gui, new_options)
        control = panel._video_buffer_size
        default = {'value': control.GetValue(), 'displayed': list(control.GetSeparatedValue()),
                   'text': panel._estimated_number_video_frames.text(),
                   'spin_max': control._spin.maximum()}
        edits = []
        for (amount, unit) in [(96, 1024 ** 2), (2, 1024 ** 3), (2, 1024 ** 2), (2637, 1024),
                               (2700, 1024), (0, 1), (8, 1024 ** 4), (5, 1)]:
            control._spin.setValue(amount)
            control._unit.SetValue(unit)
            edits.append({'amount': amount, 'unit': unit, 'value': control.GetValue(),
                          'text': panel._estimated_number_video_frames.text()})
        control._spin.setValue(300)
        control._unit.SetValue(1024 ** 2)
        panel.UpdateOptions()
        saved = new_options.GetInteger('video_buffer_size')
        reopened = SpeedAndMemoryPanel.SpeedAndMemoryPanel(c.gui, new_options)
        reopened_state = {'displayed': list(reopened._video_buffer_size.GetSeparatedValue()),
                          'text': reopened._estimated_number_video_frames.text()}
        panel.deleteLater()
        reopened.deleteLater()

        # a nine-frame animated WebP, and the media for it
        directory = tempfile.mkdtemp()
        path = str(Path(directory) / 'nine-frames.webp')
        frames = [Image.new('RGB', FRAME_SIZE, rgb) for rgb in COLOURS]
        frames[0].save(path, format='WEBP', save_all=True, append_images=frames[1:],
                       duration=DURATIONS, loop=0, lossless=True)
        saved_path = c.client_files_manager.GetFilePath
        c.client_files_manager.GetFilePath = lambda *args, **kwargs: path
        manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
        h = next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name'] == 'webp_animated.webp')
        original = c.Read('media_results', [h])[0]
        from hydrus.client.media import ClientMediaSingle
        serial = [1]

        def media(resolution, duration_ms, num_frames):
            value = original.Duplicate()
            info = value.GetFileInfoManager().Duplicate()
            info.hash = serial[0].to_bytes(32, 'big')
            serial[0] += 1
            info.mime = HC.ANIMATION_WEBP
            (info.width, info.height) = resolution
            info.duration_ms = duration_ms
            info.num_frames = num_frames
            value.SetFileInfoManager(info)
            return ClientMediaSingle.MediaSingle(value)

        shown = []
        saved_show = HydrusData.ShowText
        HydrusData.ShowText = lambda text, *args, **kwargs: shown.append(text)
        decoded = []
        current = [None]
        saved_read = ClientVideoHandling.AnimationRendererPIL.read_frame

        def read_frame(renderer):
            if current[0] is not None:
                decoded.append(current[0]._next_render_index)
            return saved_read(renderer)

        ClientVideoHandling.AnimationRendererPIL.read_frame = read_frame
        try:
            # the buffer's size
            sizes = []
            for (buffer, media_resolution, target, duration_ms, num_frames, frame_durations) in [
                (100663296, (1920, 1080), (1280, 720), 10000, 300, None),
                (100663296, (1920, 1080), (640, 360), 60000, 1800, None),
                (100663296, (1920, 1080), (640, 360), 3000, 100, None),
                (100663296, (1920, 1080), (1920, 1080), 1000, 1000, None),
                (100663296, (1920, 1080), (1920, 1080), 100000, 50, None),
                (100663296, (640, 360), (1280, 720), 60000, 1800, None),
                (100663296, (24, 20), (24, 20), 450, 9, None),
                (8640, (24, 20), (24, 20), 450, 9, None),
                (17280, (24, 20), (24, 20), 450, 9, None),
                (100663296, (1920, 1080), (640, 360), None, 1800, None),
                (100663296, (1920, 1080), (640, 360), 0, 1800, None),
                (100663296, (1920, 1080), (640, 360), 60000, None, None),
                (100663296, (1920, 1080), (640, 360), None, 1800, [50] * 1800),
                (2 * 1024 ** 3, (1920, 1080), (1920, 1080), 600000, 36000, None),
                (0, (1920, 1080), (1920, 1080), 600000, 36000, None),
            ]:
                new_options.SetInteger('video_buffer_size', buffer)
                del shown[:]
                container = ClientRendering.RasterContainerVideo(
                    media(media_resolution, duration_ms, num_frames), target,
                    frame_durations_ms=frame_durations)
                container.Stop()
                sizes.append({'buffer': buffer, 'media_resolution': list(media_resolution),
                              'target': list(target), 'duration_ms': duration_ms,
                              'num_frames': num_frames,
                              'frame_durations_sum': None if frame_durations is None else sum(frame_durations),
                              'target_resolution': list(container._target_resolution),
                              'backwards': container._num_frames_backwards,
                              'forwards': container._num_frames_forwards,
                              'messages': len(shown)})
            time.sleep(.2)

            # playing twice through, a frame at a time
            def idle(container):
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    with container._lock:
                        if (container._initialised and container._last_index_rendered == container._buffer_end_index):
                            return True
                    time.sleep(.005)
                return False

            loops = []
            count = len(DURATIONS)
            for buffer in [100663296, 17280, 8640, 4320, 2880]:
                new_options.SetInteger('video_buffer_size', buffer)
                del decoded[:]
                container = ClientRendering.RasterContainerVideo(
                    media(FRAME_SIZE, sum(DURATIONS), count), FRAME_SIZE)
                current[0] = container
                assert idle(container), 'first frames were not decoded'
                steps = []
                for step in range(2 * count):
                    index = step % count
                    deadline = time.monotonic() + 5
                    while not container.HasFrame(index) and time.monotonic() < deadline:
                        time.sleep(.005)
                    assert container.HasFrame(index), (buffer, step)
                    before = len(decoded)
                    container.GetFrame(index)
                    assert idle(container), (buffer, step)
                    with container._lock:
                        held = sorted(container._frames)
                        bounds = [container._buffer_start_index, container._buffer_end_index]
                    steps.append({'shown': index, 'decoded': decoded[before:], 'held': held,
                                  'buffer': bounds})
                container.Stop()
                current[0] = None
                loops.append({'buffer': buffer, 'backwards': container._num_frames_backwards,
                              'forwards': container._num_frames_forwards,
                              'decoded_first': decoded[:len(decoded) - sum(len(s['decoded']) for s in steps)],
                              'steps': steps, 'total_decoded': len(decoded)})
                time.sleep(.2)
            return {'panel': {'default': default, 'edits': edits, 'saved': saved,
                              'reopened': reopened_state},
                    'sizes': sizes,
                    'animation': {'bytes': Path(path).read_bytes().hex(), 'durations': DURATIONS,
                                  'size': list(FRAME_SIZE)},
                    'loops': loops,
                    'limits': 'Each loop step waits until the render thread has rendered to its buffer end, so the decode order is the buffer logic\'s. Sizes construct RasterContainerVideo on duplicated WebP media with edited resolution/duration/frame metadata and stop it at once.'}
        finally:
            new_options.SetInteger('video_buffer_size', old_size)
            c.client_files_manager.GetFilePath = saved_path
            HydrusData.ShowText = saved_show
            ClientVideoHandling.AnimationRendererPIL.read_frame = saved_read

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
    output = HERE / 'fixtures/video_buffer.json'
    output.write_text(json.dumps(result, indent=2) + '\n')
    print('wrote ' + str(output))


if __name__ == '__main__':
    main()
