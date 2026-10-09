#!/usr/bin/env python3
"""What the reference's mpv players are set to by the mpv options.

In the reference client, values are typed into the real "media playback"
options panel's mpv rows ("DEBUG: Loop Playlist instead of Loop File in mpv",
"Preferred audio output device") and applied (`UpdateOptions`). Then the real
`MPVWidget.UpdateConfAndCoreOptions` (run as a player is made, and on every
options OK for the players already open) runs with the default polite mediator
on a real libmpv player (python-mpv, no video or audio output), and mpv's own
`loop-file`, `loop-playlist` and `audio-device` properties are read back.
Writes `fixtures/mpv_playback_options.json`.

Run: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_mpv_playback_options.py
"""
import json
import sys
import tempfile
import time
import types
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

# ( preferred device or None, loop playlist instead of file )
CASES = [(None, False), ('alsa/hw:1', True), ('pulse', False), (None, True)]


def record(session):
    def qt():
        import mpv
        from hydrus.client.gui.canvas import ClientGUIMPV
        from hydrus.client.gui.panels.options.MediaPlaybackPanel import MediaPlaybackPanel

        c = session.controller
        original = c.new_options.Duplicate()
        out = {'cases': []}
        players = []

        def make_player():
            player = mpv.MPV(vo='null', ao='null')
            players.append(player)
            widget = types.SimpleNamespace(
                _player=player,
                _current_mpv_player_state=ClientGUIMPV.MPV_WIDGET_STATE_WORKING,
                _mpv_mediator=ClientGUIMPV.MPVMediatorPolite(player),
                _previous_conf_content_bytes=b'',
            )
            return widget

        def update(widget):
            ClientGUIMPV.MPVWidget.UpdateConfAndCoreOptions(widget)
            # (the polite mediator sets the device asynchronously)
            time.sleep(0.5)

        def read(player):
            return {
                'loop-file': str(player['loop-file']),
                'loop-playlist': str(player['loop-playlist']),
                'audio-device': player['audio-device'],
            }

        try:
            # one player open all along, updated on each OK
            open_widget = make_player()
            update(open_widget)
            out['open_player_at_start'] = read(open_widget._player)
            for (device, loop_playlist) in CASES:
                panel = MediaPlaybackPanel(c.gui)
                panel._mpv_preferred_audio_device.SetValue(device)
                panel._mpv_loop_playlist_instead_of_file.setChecked(loop_playlist)
                panel.UpdateOptions()
                saved = {
                    'mpv_preferred_audio_device': c.new_options.GetNoneableString('mpv_preferred_audio_device'),
                    'mpv_loop_playlist_instead_of_file': c.new_options.GetBoolean('mpv_loop_playlist_instead_of_file'),
                }
                # a player made now
                new_widget = make_player()
                update(new_widget)
                # and the one already open, as 'notify_new_options' updates it
                update(open_widget)
                out['cases'].append({
                    'typed': {'device': device, 'loop_playlist': loop_playlist},
                    'saved': saved,
                    'new_player': read(new_widget._player),
                    'open_player': read(open_widget._player),
                })
            return out
        finally:
            for player in players:
                player.terminate()
            c.new_options = original

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
    path = HERE / 'fixtures/mpv_playback_options.json'
    path.write_text(json.dumps(out, indent=1) + '\n')
    print('wrote ' + str(path))


if __name__ == '__main__':
    main()
