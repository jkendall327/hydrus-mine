#!/usr/bin/env python3
"""Record the archive/delete filter canvas showing files that play.

A real `CanvasMediaListFilterArchiveDelete` is shown over three real files
(`ugoira_json.zip` and `webp_anim.webp` from fixtures/media, and the fixture's
`audio.mp3`); for each, once it is presented: the kind of window presenting it,
whether the scanbar and volume control are shown, the scanbar's text; then the
shortcut set's pause-play command is sent twice (what space does): whether the
media is paused after each. The tags the left hover lists and the information
line (the top hover's, and the one the canvas draws behind the file) are read off the canvas too.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_archive_delete_playback.py
       (writes fixtures/archive_delete_playback.json)
"""
import json
import os
import sys
import tempfile
import time
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

FILES = [('ugoira', 'ugoira_json.zip', 'media'), ('webp', 'webp_anim.webp', 'media'), ('audio', 'audio.mp3', None)]


def record(session):
    try:
        return _record(session)
    except BaseException:
        import traceback
        traceback.print_exc()
        sys.stderr.flush()
        os._exit(1)


def _record(session):
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC, HydrusTime
    from hydrus.core.files import HydrusFileHandling
    from hydrus.client import ClientApplicationCommand as CAC, ClientConstants as CC, ClientLocation
    from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
    c = session.controller
    gui = c.gui
    manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
    by_name = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}
    original = c.Read('media_results', [by_name['jpeg_00.jpg']])[0]
    paths = {}
    serial = [100]

    def fake_result(path):
        mime = HydrusFileHandling.GetMime(path)
        (size, mime, width, height, duration_ms, num_frames, has_audio, num_words) = HydrusFileHandling.GetFileInfo(path, mime)
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
        paths[info.hash] = path
        return value
    results = {}
    for key, filename, folder in FILES:
        if folder is None:
            real = c.Read('media_results', [by_name[filename]])[0]
            results[key] = real
            paths[real.GetHash()] = c.client_files_manager.GetFilePath(real.GetHash(), real.GetMime())
        else:
            results[key] = fake_result(str(HERE / 'fixtures' / folder / filename))
    # (the information line counts "ago" from a held time)
    held_now = int(time.time())
    saved_times = (HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat)
    HydrusTime.GetNow = lambda: held_now
    HydrusTime.GetNowMS = lambda: held_now * 1000
    HydrusTime.GetNowFloat = lambda: float(held_now)
    # (the information line needs these times)
    for key, value in results.items():
        if key == 'audio':
            continue  # (the fixture's own file keeps its own times)
        manager = value.GetLocationsManager()
        times = value.GetTimesManager()
        times.SetFileModifiedTimestampMS(1234567890000)
        for key in manager.GetCurrent():
            times.SetImportedTimestampMS(key, 1234567890000)
    saved_path = c.client_files_manager.GetFilePath
    c.client_files_manager.GetFilePath = lambda h, m, *a, **k: paths.get(h) or saved_path(h, m, *a, **k)

    def qt(f):
        return c.CallBlockingToQt(gui, f)

    def make():
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListFilterArchiveDelete(
            frame, os.urandom(32), ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),
            [results[k] for k, _, _ in FILES])
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 700)
        QW.QApplication.processEvents()
        return frame, canvas
    frame, canvas = qt(make)
    out = {'files': []}
    try:
        for key, filename, folder in FILES:
            def show():
                media = [m for m in canvas._media_list.GetSortedMedia() if m.GetHash() == results[key].GetHash()][0]
                canvas.SetMedia(media)
                QW.QApplication.processEvents()
            qt(show)
            time.sleep(3)

            def look():
                QW.QApplication.processEvents()
                container = canvas._media_container
                window = container._media_window
                bar = container._animation_bar
                entry = {
                    'file': filename,
                    'window': type(window).__name__,
                    'controls bar shown': not container._controls_bar.isHidden(),
                    'volume shown': not container._volume_control.isHidden(),
                    'has duration': bool(container.CurrentlyPresentingMediaWithDuration()),
                    'paused at first': bool(container.IsPaused()),
                    'info': canvas._top_hover._info_text.text(),
                    'background info': canvas._GetInfoString(),
                    'tags': [t.GetTag() for t in canvas._tags_hover._tags._ordered_terms if hasattr(t, 'GetTag')],
                }
                steps = []
                for _ in range(3):
                    canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_PAUSE_PLAY_MEDIA))
                    QW.QApplication.processEvents()
                    steps.append(bool(container.IsPaused()))
                entry['paused after each pause-play'] = steps
                return entry
            out['files'].append(qt(look))
    finally:
        (HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat) = saved_times
        c.client_files_manager.GetFilePath = saved_path
        try:
            qt(lambda: (canvas.ClearMedia(), frame.deleteLater()))
        except Exception:
            pass
    out['now'] = held_now
    return out


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        Path(output).write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'out.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'archive_delete_playback.json').write_text(json.dumps(result, indent=2) + '\n')
    print('wrote archive_delete_playback.json')


if __name__ == '__main__':
    main()
