#!/usr/bin/env python3
"""Actual Qt saved formatting and the remaining backend diagnostics/waits.

Real NetworkJob wait/body handlers, FileFilteringImportOptions and the file
manager's low-space branch run unchanged. Owned gate/response/free-space inputs
and sleeps/critical side effects are scripted; no site contacted or disk filled.
Qt GUIPanel staging/Cancel/Apply and serialisation/reopen are real.
"""
import io
import json
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
NOW = 1700000000
SIZE = 243201
LIMIT = 243200


def record(session):
    def drive():
        from hydrus.core import HydrusConstants as HC, HydrusData, HydrusTime, HydrusSerialisable
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui.panels.options.GUIPanel import GUIPanel
        from hydrus.client.importing.options.FileFilteringImportOptions import FileFilteringImportOptions
        from hydrus.client.networking import ClientNetworkingJobs as J, ClientNetworkingContexts as C
        c = session.controller
        original = c.new_options.Duplicate()
        old_now = HydrusTime.GetNow
        old_show = HydrusData.ShowText
        HydrusTime.GetNow = lambda: NOW
        panels = []
        sleeps = []
        class Gate:
            def TryToStartRequest(self, contexts): return False
            def GetWaitingEstimateAndContext(self, contexts): return (90, C.NetworkContext(CC.NETWORK_CONTEXT_GLOBAL))
            def TryToConsumeAGalleryToken(self, domain, kind): return (False, NOW + 90)
        def job():
            j = J.NetworkJob('GET', 'https://format.example/file')
            j.engine = SimpleNamespace(bandwidth_manager=Gate())
            j._Sleep = lambda seconds: sleeps.append(seconds)
            return j
        def error(fn):
            try: fn()
            except Exception as e: return str(e)
            raise AssertionError('expected diagnostic')
        class Response:
            def __init__(self, ranged):
                self.headers = {'Content-Range': 'bytes 0-243199/188213746'} if ranged else {}
                self.request = SimpleNamespace(headers={})
                self.raw = SimpleNamespace(tell=lambda: SIZE)
            def iter_content(self, chunk_size): yield b'x' * SIZE
        def prefs(options):
            return dict(iso=options.GetBoolean('always_show_iso_time'), figures=options.GetInteger('human_bytes_sig_figs'))
        def consumers():
            b = job(); assert not b.TryToStartBandwidth()
            override = job(); override._bandwidth_manual_override_delayed_timestamp = NOW + 45
            assert not override.TryToStartBandwidth()
            gallery = job(); gallery._gallery_token_name = 'download page'
            assert not gallery.TokensOK()
            near = job(); near.engine.bandwidth_manager.GetWaitingEstimateAndContext = lambda contexts: (2, C.NetworkContext(CC.NETWORK_CONTEXT_GLOBAL))
            assert not near.TryToStartBandwidth()
            gallery_near = job(); gallery_near._gallery_token_name = 'download page'
            gallery_near.engine.bandwidth_manager.TryToConsumeAGalleryToken = lambda domain, kind: (False, NOW + 2)
            assert not gallery_near.TokensOK()
            three = job(); three.engine.bandwidth_manager.GetWaitingEstimateAndContext = lambda contexts: (3, C.NetworkContext(CC.NETWORK_CONTEXT_GLOBAL))
            assert not three.TryToStartBandwidth()
            gallery_three = job(); gallery_three._gallery_token_name = 'download page'
            gallery_three.engine.bandwidth_manager.TryToConsumeAGalleryToken = lambda domain, kind: (False, NOW + 3)
            assert not gallery_three.TokensOK()
            validation = {}
            for name, mime, size, limit in [('minimum', HC.IMAGE_PNG, 1536, 243200), ('maximum', HC.IMAGE_PNG, 188213746, 243200), ('gif', HC.ANIMATION_GIF, 188213746, 243200)]:
                f = FileFilteringImportOptions()
                if name == 'minimum': f.SetMinSize(limit)
                elif name == 'maximum': f.SetMaxSize(limit)
                else: f.SetMaxGifSize(limit)
                validation[name] = error(lambda: f.CheckFileIsValid(size, mime, 10, 10))
            network = {}
            for ranged in [False, True]:
                j = job()
                j._num_bytes_to_read = None if ranged else LIMIT
                network['range' if ranged else 'whole'] = error(lambda: j._ReadResponse(Response(ranged), io.BytesIO()))
                assert j._num_bytes_read == SIZE
            manager = c.client_files_manager
            names = ['_GenerateExpectedFilePath', '_GetFileStorageFreeSpaceForHash', '_HandleCriticalDriveError']
            originals = [getattr(manager, name) for name in names]
            critical = []
            try:
                manager._GenerateExpectedFilePath = lambda h, m: '/synthetic/media/file.png'
                manager._GetFileStorageFreeSpaceForHash = lambda h: 0
                manager._HandleCriticalDriveError = lambda: critical.append(True)
                HydrusData.ShowText = lambda message: None
                with tempfile.NamedTemporaryFile() as source:
                    source.truncate(188213746)
                    disk = error(lambda: manager._AddFile(bytes([17]) * 32, HC.IMAGE_PNG, source.name))
                assert critical == [True]
            finally:
                for name, value in zip(names, originals): setattr(manager, name, value)
                HydrusData.ShowText = old_show
            return dict(bandwidth=b.GetStatus()[0], override=override.GetStatus()[0], gallery=gallery.GetStatus()[0], imminently=near.GetStatus()[0], checking=gallery_near.GetStatus()[0], bandwidth_three=three.GetStatus()[0], gallery_three=gallery_three.GetStatus()[0], validation=validation, network=network, critical_drive=disk)
        try:
            events = []
            for iso, figures in [(False, 3), (True, 1), (False, 6), (True, 2)]:
                p = GUIPanel(c.gui); panels.append(p)
                before = prefs(c.new_options)
                before_strings = consumers()
                p._always_show_iso_time.setChecked(iso); p._human_bytes_sig_figs.setValue(figures)
                assert prefs(c.new_options) == before
                staged = consumers(); assert staged == before_strings
                p.UpdateOptions(); c.ReinitGlobalSettings()
                encoded = HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
                reopened = GUIPanel(c.gui); panels.append(reopened)
                events.append(dict(input=[iso, figures], before=before, staged=staged, saved=prefs(c.new_options), reopened=dict(iso=reopened._always_show_iso_time.isChecked(), figures=reopened._human_bytes_sig_figs.value()), round_trip=prefs(encoded), consumers=consumers()))
            cancelled = GUIPanel(c.gui); panels.append(cancelled)
            cancelled._always_show_iso_time.setChecked(False); cancelled._human_bytes_sig_figs.setValue(5)
            cancelled.deleteLater(); panels.remove(cancelled)
            assert consumers() == events[-1]['consumers']
            return dict(now=NOW, limit=LIMIT, read=SIZE, events=events, cancelled=events[-1]['consumers'])
        finally:
            c.new_options = original; c.ReinitGlobalSettings()
            HydrusTime.GetNow = old_now; HydrusData.ShowText = old_show
            for p in panels: p.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = Path(sys.argv[2])
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(value)); return
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(path))
        value = json.loads(path.read_text())
    (HERE / 'fixtures/gui_format_backend.json').write_text(json.dumps(value, indent=2) + '\n')
    print('wrote gui_format_backend.json')
if __name__ == '__main__': main()
