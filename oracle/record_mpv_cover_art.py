#!/usr/bin/env python3
"""What the reference's mpv shows for an audio file with embedded cover art.

In the reference client, a real libmpv player (python-mpv, with no video or
audio output: the reference's embedded player needs a display) is set up as
the reference's `MPVWidget` sets its own: `UpdateConfAndCoreOptions` with the
client's mpv.conf (the default one, `static/mpv-conf/default_mpv.conf`, as a
new client has it), then each file is loaded through the reference's
`MPVMediatorPolite.LoadFile`. Once mpv has started playing it, its
`current-tracks/video/albumart` (whether the picture it shows is the file's
attached picture), the picture's `video-params/w` and `/h`, and the kinds of
its tracks are read back, for a flac and an mp3 with a cover and a flac
without one. Writes `fixtures/mpv_cover_art.json`.

Run: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_mpv_cover_art.py
"""
import json
import sys
import tempfile
import time
import types
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

FILES = ['flac_cover.flac', 'mp3_cover.mp3', 'flac.flac']


def record(session):
    def qt():
        import mpv
        from hydrus.client.gui.canvas import ClientGUIMPV

        out = {'files': []}
        for name in FILES:
            player = mpv.MPV(vo='null', ao='null')
            try:
                widget = types.SimpleNamespace(
                    _player=player,
                    _current_mpv_player_state=ClientGUIMPV.MPV_WIDGET_STATE_WORKING,
                    _mpv_mediator=ClientGUIMPV.MPVMediatorPolite(player),
                    _previous_conf_content_bytes=b'',
                )
                ClientGUIMPV.MPVWidget.UpdateConfAndCoreOptions(widget)
                widget._mpv_mediator.LoadFile(str(HERE / 'fixtures' / 'media' / name))

                def get(key):
                    try:
                        # (a property path, not an option)
                        value = player._get_property(key)
                    except Exception:
                        return None
                    if isinstance(value, bool):
                        return 'yes' if value else 'no'
                    return None if value is None else str(value)

                # (mpv has started once the audio plays on)
                for _ in range(200):
                    time.sleep(0.05)
                    if get('playback-time') not in (None, '0.0', '0'):
                        break
                time.sleep(0.3)
                out['files'].append({
                    'file': name,
                    'audio-display': get('audio-display'),
                    'albumart': get('current-tracks/video/albumart'),
                    'width': get('video-params/w'),
                    'height': get('video-params/h'),
                    'tracks': sorted(
                        (t.get('type'), bool(t.get('albumart')), bool(t.get('selected')))
                        for t in player.track_list
                    ),
                })
            finally:
                player.terminate()
        return out

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
        result = Path(directory) / 'record.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(result))
        out = json.loads(result.read_text())
    path = HERE / 'fixtures/mpv_cover_art.json'
    path.write_text(json.dumps(out, indent=1) + '\n')
    print('wrote ' + str(path))


if __name__ == '__main__':
    main()
