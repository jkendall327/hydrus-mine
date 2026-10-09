#!/usr/bin/env python3
"""Record the reference's 'fetch mpv audio devices' button (media playback
options, MediaPlaybackPanel._FetchMPVAudioDevices).

mpv's device list is stubbed with a fixed list (what mpv reports in a cloud
container, as `audio-device-list` JSON), so the recording does not depend on
the machine. With mpv unavailable the button shows a message; with it, a
chooser offers each device (and 'null') and the choice fills 'Preferred audio
output device:', 'auto' setting it to none. Each choice and a cancel are
recorded, from an empty option and from a set one.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

DEVICE_LIST_JSON = '[{"name":"auto","description":"Autoselect device"},{"name":"alsa","description":"Default (alsa)"},{"name":"pulse","description":"Default (pulse)"}]'


def record(session):
    c = session.controller

    def drive():
        from hydrus.client.gui import ClientGUIDialogsMessage as M
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.canvas import ClientGUIMPV
        from hydrus.client.gui.panels.options import MediaPlaybackPanel as P
        from hydrus.core import HydrusExceptions
        old = (ClientGUIMPV.MPV_IS_AVAILABLE, ClientGUIMPV.GetAudioDeviceTuples, Q.SelectFromListButtons, M.ShowInformation)
        log = []
        answer = {'choice': None}

        def tuples():
            devices = [(d['name'], d['description']) for d in json.loads(DEVICE_LIST_JSON)]
            return devices + [('null', 'DEBUG: Do not use any audio output device')]

        def select(parent, title, choice_tuples=(), message='', **kwargs):
            log.append({'chooser': title, 'message': message, 'choices': [[t, d, tip] for (t, d, tip) in choice_tuples]})
            if answer['choice'] is None:
                raise HydrusExceptions.CancelledException()
            return choice_tuples[answer['choice']][1]

        def information(parent, message, **kwargs):
            log.append({'information': message})

        ClientGUIMPV.GetAudioDeviceTuples, Q.SelectFromListButtons, M.ShowInformation = tuples, select, information
        panels = []
        out = {'device_list_json': DEVICE_LIST_JSON, 'button': None, 'runs': []}
        try:
            for available in (False, True):
                ClientGUIMPV.MPV_IS_AVAILABLE = available
                for start in (None, 'pulse'):
                    for choice in ((None,) if not available else (None, 0, 1, 2, 3)):
                        panel = P.MediaPlaybackPanel(c.gui)
                        panels.append(panel)
                        out['button'] = panel._mpv_preferred_audio_device_fetch_button.text()
                        panel._mpv_preferred_audio_device.SetValue(start)
                        answer['choice'] = choice
                        log.clear()
                        panel._mpv_preferred_audio_device_fetch_button.click()
                        out['runs'].append({'available': available, 'start': start, 'choice': choice, 'log': list(log), 'value': panel._mpv_preferred_audio_device.GetValue()})
            return out
        finally:
            ClientGUIMPV.MPV_IS_AVAILABLE, ClientGUIMPV.GetAudioDeviceTuples, Q.SelectFromListButtons, M.ShowInformation = old
            for panel in panels:
                panel.deleteLater()

    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f:
            result = json.load(f)
        with open(os.path.join(HERE, 'fixtures/mpv_audio_devices.json'), 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
            f.write('\n')


if __name__ == '__main__':
    main()
