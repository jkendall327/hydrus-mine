#!/usr/bin/env python3
"""Record actual Preview default zoom controls and CanvasPanel geometry.

Uses copied basic stills with synthetic resolutions, real MediaPlaybackPanel
choices/UpdateOptions and real preview MediaContainer SetMedia/resize. No network,
replacement zoom arithmetic or reference edits. Full-viewer default stays distinct.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    c = session.controller

    def drive():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasMedia as CM
        from hydrus.client.gui.panels.options.MediaPlaybackPanel import MediaPlaybackPanel
        from hydrus.client.media import ClientMediaSingle
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable

        original = c.new_options
        options = original.Duplicate()
        c.new_options = options
        old_hidden = HC.options['hide_preview']
        HC.options['hide_preview'] = False
        panel = MediaPlaybackPanel(c.gui)
        choice = panel._preview_default_zoom_type_override
        default = options.GetInteger('preview_default_zoom_type_override')
        choices = [dict(label=choice.itemText(i), code=choice.itemData(i)) for i in range(choice.count())]
        controls = []
        parent = QW.QWidget(c.gui)
        layout = QW.QVBoxLayout(parent)
        layout.setContentsMargins(0, 0, 0, 0)
        canvas = ClientGUICanvas.CanvasPanel(parent, bytes([91]) * 32, ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY))
        layout.addWidget(canvas)
        parent.resize(360, 240)
        parent.show()
        QW.QApplication.processEvents()
        manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
        file = next(f for f in manifest['files'] if f['name'] == 'jpeg_00.jpg')
        still = c.Read('media_results', [bytes.fromhex(file['hash'])])[0]
        cases = []
        dpi_cases = []
        try:
            for item in choices:
                before = options.GetInteger('preview_default_zoom_type_override')
                choice.SetValue(item['code'])
                staged = options.GetInteger('preview_default_zoom_type_override')
                panel.UpdateOptions()
                encoded = HydrusSerialisable.CreateFromString(options.DumpToString())
                c.new_options = encoded
                reopened = MediaPlaybackPanel(c.gui)
                c.new_options = options
                controls.append(dict(code=item['code'], before=before, staged=staged, saved=encoded.GetInteger('preview_default_zoom_type_override'), reopened=reopened._preview_default_zoom_type_override.GetValue()))
                reopened.deleteLater()
            cancel_before = options.GetInteger('preview_default_zoom_type_override')
            choice.SetValue(default)
            cancel_after = options.GetInteger('preview_default_zoom_type_override')
            panel.resize(1050, 750)
            panel.show()
            QW.QApplication.processEvents()
            panel.grab().save(str(HERE / 'fixtures/preview_default_zoom_qt.png'))
            panel.hide()
            # Use conflicting full-viewer scaling to prove preview rules win.
            views = options.GetMediaViewOptions()
            view = list(views.get(still.GetMime(), views[HC.GENERAL_IMAGE]))
            zoom = list(view[-1])
            zoom[0] = CC.MEDIA_VIEWER_SCALE_100
            zoom[1] = CC.MEDIA_VIEWER_SCALE_100
            zoom[2] = CC.MEDIA_VIEWER_SCALE_MAX_REGULAR
            zoom[3] = CC.MEDIA_VIEWER_SCALE_TO_CANVAS
            view[-1] = tuple(zoom)
            views[still.GetMime()] = tuple(view)
            options.SetMediaViewOptions(views)
            options.SetInteger('media_viewer_default_zoom_type_override', CM.MEDIA_VIEWER_ZOOM_TYPE_100)
            # Synthetic metadata probes exercise real SetMedia geometry without
            # asking Qt's tile renderer to draw mismatched physical file sizes.
            parent.setUpdatesEnabled(False)
            for resolution in [(120, 80), (1200, 800), (1200, 160), (160, 1200)]:
                result = still.Duplicate()
                result.GetFileInfoManager().width, result.GetFileInfoManager().height = resolution
                media = ClientMediaSingle.MediaSingle(result)
                for dimensions in [(360, 240), (241, 179)]:
                    parent.resize(*dimensions)
                    QW.QApplication.processEvents()
                    for item in choices:
                        canvas.ClearMedia()
                        options.SetInteger('preview_default_zoom_type_override', item['code'])
                        canvas.SetMedia(media)
                        QW.QApplication.processEvents()
                        container = canvas._media_container
                        size = canvas.size()
                        cases.append(dict(resolution=resolution, canvas=[size.width(), size.height()], dpr=canvas.devicePixelRatio(), code=item['code'], zoom=container._current_zoom, rect=[container.x(), container.y(), container.width(), container.height()]))
                        zooms = CM.CalculateCanvasZooms(size, CC.CANVAS_PREVIEW, 2.0, media, CC.MEDIA_VIEWER_ACTION_SHOW_WITH_NATIVE)
                        dpi_size = CM.CalculateMediaContainerSize(media, 2.0, zooms[item['code']], CC.MEDIA_VIEWER_ACTION_SHOW_WITH_NATIVE)
                        dpi_cases.append(dict(resolution=resolution, canvas=[size.width(), size.height()], dpr=2.0, code=item['code'], size=[dpi_size.width(), dpi_size.height()]))
            # Saving a choice alone leaves accepted media untouched; new SetMedia
            # and real resize consume the newly saved default.
            canvas.ClearMedia()
            parent.resize(360, 240)
            QW.QApplication.processEvents()
            options.SetInteger('preview_default_zoom_type_override', CM.MEDIA_VIEWER_ZOOM_TYPE_100)
            canvas.SetMedia(media)
            QW.QApplication.processEvents()
            def rect():
                w = canvas._media_container
                return [w.x(), w.y(), w.width(), w.height()]
            before_change = rect()
            options.SetInteger('preview_default_zoom_type_override', CM.MEDIA_VIEWER_ZOOM_TYPE_CANVAS)
            QW.QApplication.processEvents()
            after_save = rect()
            parent.resize(361, 241)
            QW.QApplication.processEvents()
            after_resize = rect()
            canvas.ClearMedia()
            canvas.SetMedia(ClientMediaSingle.MediaSingle(still))
            parent.setUpdatesEnabled(True)
            QW.QApplication.processEvents()
            canvas.grab().save(str(HERE / 'fixtures/preview_default_zoom_canvas_qt.png'))
            return dict(default=default, label='Preview Viewer default zoom:', choices=choices, controls=controls, cancel_before=cancel_before, cancel_after=cancel_after, cases=cases, dpi_cases=dpi_cases, file=file['hash'], mime=still.GetMime(), full_viewer_default=options.GetInteger('media_viewer_default_zoom_type_override'), media_view_options=list(view), media_zooms=options.GetMediaZooms(), live=dict(before=before_change, after_save=after_save, after_resize=after_resize), legacy_options=json.loads(options.DumpToString()))
        finally:
            canvas.ClearMedia()
            parent.close()
            parent.deleteLater()
            panel.deleteLater()
            c.new_options = original
            HC.options['hide_preview'] = old_hidden

    return c.CallBlockingToQt(c.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = Path(sys.argv[2])
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(value))
        return
    with tempfile.TemporaryDirectory() as temporary:
        output = Path(temporary) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        value = json.loads(output.read_text())
    output = HERE / 'fixtures/preview_default_zoom.json'
    output.write_text(json.dumps(value, indent=2) + '\n')
    print(f'wrote {output}')


if __name__ == '__main__':
    main()
