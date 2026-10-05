#!/usr/bin/env python3
"""Record real preview Options, manager policy and displayed CanvasPanel intervals.

Imported still/video metadata exercises cap-before-minimum and duration x5.
Actual CanvasPanel receives same-file, clear, hidden-page, splitter, invisibility,
inactive statistics and destruction transitions with controlled time. Publication
is isolated, but FinishViewing and the Options controls remain real consumers.
No HTTP requests or replacement canvas implementations are used.
"""
import itertools
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.canvas import ClientGUICanvas
        from hydrus.client.gui.panels.options.FileViewingStatisticsPanel import FileViewingStatisticsPanel
        from hydrus.client.media import ClientMediaSingle
        from hydrus.core import HydrusConstants as HC, HydrusTime, HydrusSerialisable

        controller = session.controller
        options = controller.new_options
        panel = FileViewingStatisticsPanel(controller.gui)
        with open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')) as stream:
            manifest = json.load(stream)
        def result(name):
            file_hash = next(bytes.fromhex(file['hash']) for file in manifest['files'] if file['name'] == name)
            return controller.Read('media_results', [file_hash])[0]
        still = result('jpeg_00.jpg')
        video = result('video_with_audio.mp4')
        controls = []
        for name in ('preview_min_time', 'preview_max_time'):
            widget = getattr(panel, '_file_viewing_statistics_' + name)
            for requested in (None, 0, .049, .05, .999, 1, 1.001, 1.999, 600.125):
                widget.SetValue(requested)
                panel.UpdateOptions()
                reopened = HydrusSerialisable.CreateFromString(options.DumpToString())
                controls.append({'control': name, 'requested': requested, 'value': widget.GetValue(),
                    'persisted_ms': options.GetNoneableInteger('file_viewing_statistics_' + name + '_ms'),
                    'reopened_ms': reopened.GetNoneableInteger('file_viewing_statistics_' + name + '_ms')})
        policies = []
        for minimum, maximum in ((None, None), (.05, 1), (2, 1), (2, 600), (5, 60)):
            panel._file_viewing_statistics_preview_min_time.SetValue(minimum)
            panel._file_viewing_statistics_preview_max_time.SetValue(maximum)
            panel.UpdateOptions()
            for media, elapsed in itertools.product((still, video), (0, 49, 50, 1000, 2000, 4999, 5000, 60001, 600001)):
                row = controller.file_viewing_stats_manager._GenerateViewsRow(media, CC.CANVAS_PREVIEW, 123000, elapsed)
                policies.append({'minimum_ms': options.GetNoneableInteger('file_viewing_statistics_preview_min_time_ms'),
                    'maximum_ms': options.GetNoneableInteger('file_viewing_statistics_preview_max_time_ms'),
                    'duration_ms': media.GetDurationMS(), 'elapsed_ms': elapsed, 'row': row})

        manager = controller.file_viewing_stats_manager
        old_pending = manager._pending_updates
        old_pub = manager._PubSubRow
        old_ms = HydrusTime.GetNowMS
        old_hidden = HC.options['hide_preview']
        clock = [123000]
        manager._pending_updates = {}
        manager._PubSubRow = lambda *args: None
        HydrusTime.GetNowMS = lambda: clock[0]
        HC.options['hide_preview'] = False
        parent = QW.QWidget(controller.gui)
        layout = QW.QVBoxLayout(parent)
        canvas = ClientGUICanvas.CanvasPanel(parent, os.urandom(32), ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))
        layout.addWidget(canvas)
        parent.resize(360, 280)
        parent.show()
        QW.QApplication.processEvents()
        media = ClientMediaSingle.MediaSingle(still)
        other = ClientMediaSingle.MediaSingle(result('jpeg_01.jpg'))
        events = []
        rejections = []
        def capture(name):
            events.append({'event': name, 'at_ms': clock[0], 'shown': None if canvas._current_media is None else canvas._current_media.GetHash().hex(),
                'rows': [{'hash': key[0].hex(), 'canvas': key[1], 'row': list(row)} for key, row in sorted(manager._pending_updates.items())]})
        try:
            panel._file_viewing_statistics_active.setChecked(True)
            panel._file_viewing_statistics_preview_min_time.SetValue(None)
            panel._file_viewing_statistics_preview_max_time.SetValue(1)
            panel.UpdateOptions()
            canvas.SetMedia(media); capture('show')
            QW.QApplication.processEvents()
            parent.grab().save(os.path.join(HERE, 'fixtures/preview_viewing_intervals.png'))
            clock[0] = 123050; canvas.SetMedia(media); capture('same-file')
            clock[0] = 125000; canvas.ClearMedia(); capture('clear')
            canvas.ClearMedia(); capture('clear-again')
            clock[0] = 126000; canvas.SetMedia(media)
            clock[0] = 128000; canvas.PageHidden(); capture('page-hidden')
            clock[0] = 130000; canvas.PageShown(); capture('page-shown')
            clock[0] = 132000; canvas.SetSplitterHiddenStatus(True); capture('splitter-hidden')
            clock[0] = 134000; canvas.SetMedia(other); capture('blocked-splitter')
            canvas.SetSplitterHiddenStatus(False); capture('splitter-shown-empty')
            parent.hide(); clock[0] = 136000; canvas.SetMedia(other); capture('blocked-invisible')
            parent.show(); QW.QApplication.processEvents()
            clock[0] = 138000; canvas.SetMedia(media)
            panel._file_viewing_statistics_active.setChecked(False); panel.UpdateOptions()
            clock[0] = 140000; canvas.ClearMedia(); capture('inactive-at-finish')
            panel._file_viewing_statistics_active.setChecked(True)
            panel._file_viewing_statistics_preview_min_time.SetValue(2)
            panel._file_viewing_statistics_preview_max_time.SetValue(None); panel.UpdateOptions()
            clock[0] = 142000; canvas.SetMedia(media)
            clock[0] = 145000; canvas.SetMedia(other); capture('live-settings-change')
            clock[0] = 148000; canvas.CleanBeforeDestroy(); capture('clean-before-destroy')
            canvas.CleanBeforeDestroy(); capture('clean-again')
            old_views = options.GetMediaViewOptions()
            try:
                nonlocal_result = still.Duplicate()
                nonlocal_result.GetLocationsManager().GetCurrent().clear()
                invalid = still.Duplicate(); invalid.GetFileInfoManager().width = 0
                for name, value in (('nonlocal', nonlocal_result), ('zero-width', invalid)):
                    canvas.SetMedia(ClientMediaSingle.MediaSingle(value))
                    rejections.append({'case': name, 'shown': canvas._current_media is not None})
                views = dict(old_views)
                mime = still.GetMime()
                settings = list(views.get(mime, views[HC.GENERAL_IMAGE]))
                settings[3] = CC.MEDIA_VIEWER_ACTION_DO_NOT_SHOW
                views[mime] = tuple(settings); options.SetMediaViewOptions(views)
                canvas.SetMedia(media)
                rejections.append({'case': 'do-not-show', 'shown': canvas._current_media is not None})
            finally:
                options.SetMediaViewOptions(old_views)
        finally:
            canvas.ClearMedia(); parent.close(); parent.deleteLater(); panel.deleteLater()
            QW.QApplication.sendPostedEvents(None, QC.QEvent.Type.DeferredDelete)
            QW.QApplication.processEvents()
            manager._pending_updates = old_pending
            manager._PubSubRow = old_pub
            HydrusTime.GetNowMS = old_ms
            HC.options['hide_preview'] = old_hidden
        return {'file': still.GetHash().hex(), 'video': video.GetHash().hex(), 'controls': controls, 'policies': policies, 'events': events, 'rejections': rejections}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def child(output):
    import hydrus_driver
    import record_api
    value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(output, 'w') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
    else:
        import subprocess
        subprocess.run([sys.executable, __file__, '--child', os.path.join(HERE, 'fixtures/preview_viewing_intervals.json')], check=True)
