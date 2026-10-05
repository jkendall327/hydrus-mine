#!/usr/bin/env python3
"""Real Qt timeout staging and real HydrusFFMPEG subprocess deadlines.

The executable is an authored local transport with valid metadata/version output
and a controlled delay. No reference function or subprocess runner is replaced.
"""
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientOptions
    from hydrus.client.gui.panels.options.MediaPlaybackPanel import MediaPlaybackPanel
    from hydrus.core.files import HydrusFFMPEG as F
    from hydrus.core import HydrusSerialisable
    c = session.controller
    key = 'ffmpeg_subprocess_timeout'
    original = c.new_options.GetInteger(key)
    saved_path = os.environ['PATH']
    saved_prefer = F.PREFER_SYSTEM_FFMPEG
    with tempfile.TemporaryDirectory() as directory:
        directory = Path(directory)
        executable = directory / 'ffmpeg'
        delay = directory / 'delay'
        started = directory / 'started'
        executable.write_text('#!' + sys.executable + '\n' +
            'import pathlib,sys,time\n' +
            f'delay=pathlib.Path({str(delay)!r}); started=pathlib.Path({str(started)!r})\n' +
            'started.write_text(str(__import__("os").getpid()))\n' +
            'time.sleep(float(delay.read_text()))\n' +
            'if "-version" in sys.argv: print("ffmpeg version synthetic-timeout Copyright transport")\n' +
            'else: print("ffmpeg version synthetic-timeout\\nInput #0, synthetic, from \'local\':\\n  Duration: 00:00:02.00, start: 0.000000, bitrate: 8 kb/s",file=sys.stderr)\n')
        executable.chmod(0o755)
        os.environ['PATH'] = str(directory) + os.pathsep + saved_path

        def drive():
            dialog = QW.QDialog(c.gui)
            layout = QW.QVBoxLayout(dialog)
            panel = MediaPlaybackPanel(dialog)
            layout.addWidget(panel)
            spin = panel._ffmpeg_subprocess_timeout
            loaded = spin.value()
            spin.setValue(7)
            cancel = dict(staged=spin.value(), saved=c.new_options.GetInteger(key))
            panel.setParent(None); panel.deleteLater()
            panel = MediaPlaybackPanel(dialog); layout.addWidget(panel)
            spin = panel._ffmpeg_subprocess_timeout
            cancel['reopened'] = spin.value()
            cases = []
            for requested in [1, 600, 15, 0, 601]:
                before = c.new_options.GetInteger(key)
                spin.setValue(requested)
                staged = spin.value()
                assert c.new_options.GetInteger(key) == before
                panel.UpdateOptions(); c.ReinitGlobalSettings()
                reopened_options = HydrusSerialisable.CreateFromString(c.new_options.DumpToString())
                cases.append(dict(requested=requested, staged=staged, saved=c.new_options.GetInteger(key), published=F.FFMPEG_SUBPROCESS_TIMEOUT, reopened=reopened_options.GetInteger(key)))
            raw = []
            for value in [0, 601]:
                c.new_options.SetInteger(key,value)
                raw_panel = MediaPlaybackPanel(dialog)
                shown = raw_panel._ffmpeg_subprocess_timeout.value()
                before = c.new_options.GetInteger(key)
                raw_panel.UpdateOptions()
                raw.append(dict(imported=value,shown=shown,before_apply=before,saved=c.new_options.GetInteger(key)))
                raw_panel.deleteLater()
            spin.setValue(3); panel.UpdateOptions(); c.ReinitGlobalSettings()
            F.PREFER_SYSTEM_FFMPEG = True
            return dialog, panel, dict(default=ClientOptions.ClientOptions().GetInteger(key),loaded=loaded,cancel=cancel,cases=cases,raw=raw,min=spin.minimum(),max=spin.maximum())

        dialog, panel, value = c.CallBlockingToQt(c.gui, drive)
        observations = []
        try:
            for operation in ['metadata','version']:
                c.CallBlockingToQt(c.gui, lambda: (c.new_options.SetInteger(key,6),c.ReinitGlobalSettings(),setattr(F,'PREFER_SYSTEM_FFMPEG',True)))
                delay.write_text('4.0'); started.unlink(missing_ok=True)
                result = {}
                begin = time.monotonic()
                def call():
                    try:
                        result['output'] = F.GetFFMPEGInfoLines('authored-local-input') if operation == 'metadata' else F.GetFFMPEGVersion()
                        result['ok'] = True
                    except Exception as error:
                        result.update(ok=False,error_type=type(error).__name__,error=str(error))
                thread = threading.Thread(target=call); thread.start()
                deadline = time.monotonic()+5
                while not started.exists() and time.monotonic() < deadline: time.sleep(.01)
                assert started.exists(), 'real child must start'
                c.CallBlockingToQt(c.gui, lambda: (c.new_options.SetInteger(key,1),c.ReinitGlobalSettings(),setattr(F,'PREFER_SYSTEM_FFMPEG',True)))
                thread.join(8); assert not thread.is_alive()
                observations.append(dict(operation=operation,admitted_seconds=6,later_saved_seconds=1,current=result,elapsed_ms=round((time.monotonic()-begin)*1000)))
                delay.write_text('2.0')
                begin = time.monotonic()
                below = F.GetFFMPEGInfoLines('authored-local-input') if operation == 'metadata' else F.GetFFMPEGVersion()
                observations[-1]['below_poll'] = dict(configured_seconds=1,delay_seconds=2,output=below,elapsed_ms=round((time.monotonic()-begin)*1000))
                delay.write_text('4.0')
                begin = time.monotonic()
                try:
                    output = F.GetFFMPEGInfoLines('authored-local-input') if operation == 'metadata' else F.GetFFMPEGVersion()
                    next_result = dict(ok=True,output=output)
                except Exception as error:
                    next_result=dict(ok=False,error_type=type(error).__name__,error=str(error))
                observations[-1]['next'] = next_result
                observations[-1]['next_elapsed_ms'] = round((time.monotonic()-begin)*1000)
            def finish():
                panel._ffmpeg_subprocess_timeout.setValue(1)
                dialog.show(); panel._ffmpeg_subprocess_timeout.setFocus()
                QW.QApplication.processEvents()
                # Capture the actual system controls without the very tall panel.
                system = panel._ffmpeg_subprocess_timeout.parentWidget()
                system.grab().save(str(HERE/'fixtures'/'ffmpeg_timeout.png'))
                return c.new_options.GetSerialisableTuple()
            value['legacy'] = c.CallBlockingToQt(c.gui,finish)
            value['runtime'] = observations
            value['transport'] = 'authored local executable; actual HydrusFFMPEG and HydrusSubprocess calls'
            return value
        finally:
            os.environ['PATH'] = saved_path
            def cleanup():
                c.new_options.SetInteger(key,original); c.ReinitGlobalSettings(); F.PREFER_SYSTEM_FFMPEG=saved_prefer
                dialog.close(); dialog.deleteLater()
            c.CallBlockingToQt(c.gui,cleanup)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api, sqlite3
        output = Path(sys.argv[2])
        database = record_api.unpack_fixture('basic')
        with sqlite3.connect(Path(database)/'client.db') as connection:
            raw=json.loads(connection.execute('SELECT dictionary_string FROM services WHERE service_type=18').fetchone()[0])
            for name, value in raw[2]:
                if name == [0,'port']: value[1]=None
            connection.execute('UPDATE services SET dictionary_string=? WHERE service_type=18',(json.dumps(raw),))
        connection.close()
        value = hydrus_driver.run_client(database,record)
        output.write_text(json.dumps(value)); return
    with tempfile.TemporaryDirectory() as directory:
        output=Path(directory)/'value.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(output))
        value=json.loads(output.read_text())
    target=HERE/'fixtures'/'ffmpeg_timeout.json'
    target.write_text(json.dumps(value,indent=2)+'\n')
    print(f'wrote {target}')

if __name__=='__main__': main()
