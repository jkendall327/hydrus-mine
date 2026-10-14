#!/usr/bin/env python3
"""Record what "Fade thumbnails" (`fade_thumbnails`) does in both thumbnail panels.

Fresh temporary `basic` client; a page over its files is opened with the old
renderer (`MediaResultsPanelThumbnails`) and with the new one
(`MediaResultsPanelThumbnailsGraphicsViewTest`, `test_thumbnails_graphics_view`).
The time the fades read is held and stepped.

New renderer: a thumbnail whose cell has changed (it is selected) is told to
fade (`_FadeThumbnails`, what the panel calls) and its opacity
(`_GetFadeInOpacity`) is read at each elapsed time, and whether it still holds
the old picture. Old renderer: the panel's own `_FadeThumbnails` makes the
thumbnail's drawing object (white thumbnails over a black canvas), which is
drawn as the panel draws it, each time it is due; the canvas's brightness is
the opacity. Each with the option on and off.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'thumbnail_fade_option.json')
ELAPSED = [0, 0.01, 0.043, 0.087, 0.131, 0.2, 0.27, 0.45, 0.6]


def record(session):
    from qtpy import QtGui as QG, QtCore as QC
    from hydrus.core import HydrusTime
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.pages import ClientGUIMediaResultsPanelThumbnails as T
    c = session.controller
    qt = lambda f: c.CallBlockingToQt(c.gui, f)
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files']]
    opts = c.new_options

    def open_page(new):
        qt(lambda: opts.SetBoolean('test_thumbnails_graphics_view', new))
        page = qt(lambda: c.gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY), initial_hashes=hashes))
        for _ in range(400):
            panel = qt(page.GetMediaResultsPanel)
            if len(panel._sorted_media) == len(hashes) and (not new or len(panel._media_to_thumbnails) == len(hashes)):
                return page, panel
            time.sleep(0.05)
        raise RuntimeError('thumbnail page did not load')

    def with_clock(now, func):
        old = HydrusTime.GetNowFloat
        HydrusTime.GetNowFloat = lambda: now[0]
        try:
            return func()
        finally:
            HydrusTime.GetNowFloat = old

    def new_fade(panel, fade):
        now = [100.0]
        opts.SetBoolean('fade_thumbnails', fade)
        media = list(panel._sorted_media)[0]
        item = panel._media_to_thumbnails[media]

        def cell():
            image = QG.QImage(item.width, item.height, QG.QImage.Format.Format_ARGB32_Premultiplied)
            image.fill(QC.Qt.GlobalColor.transparent)
            p = QG.QPainter(image)
            item._PaintThumbnailContent(p, item.media, item._view)
            p.end()
            return image

        def go():
            old = cell()
            item.is_selected = not item.is_selected
            item._cached_pixmap = QG.QPixmap.fromImage(old)
            item._cached_old_pixmap_for_fade = None
            panel.isVisible()
            visible = bool(panel.isVisible())
            was = c.gui.IsCurrentPage
            c.gui.IsCurrentPage = lambda key: True
            try:
                panel._FadeThumbnails([media])
            finally:
                c.gui.IsCurrentPage = was
            samples = []
            for elapsed in ELAPSED:
                now[0] = 100.0 + elapsed
                samples.append(dict(elapsed=elapsed, opacity=item._GetFadeInOpacity(), old_retained=item._cached_old_pixmap_for_fade is not None))
            item.is_animating = False
            return dict(fade=fade, panel_visible=visible, samples=samples)
        return with_clock(now, go)

    def old_fade(panel, fade):
        now = [100.0]
        opts.SetBoolean('fade_thumbnails', fade)
        media = list(panel._sorted_media)[0]

        def go():
            white = QG.QImage(20, 20, QG.QImage.Format.Format_ARGB32_Premultiplied)
            white.fill(QG.QColor(255, 255, 255))
            original = T.ThumbnailMediaSingle.GetQtImage
            T.ThumbnailMediaSingle.GetQtImage = lambda self, *a, **k: white
            was = c.gui.IsCurrentPage
            c.gui.IsCurrentPage = lambda key: True
            panel._clean_canvas_pages = {0: QG.QPixmap(10, 10)}
            panel._hashes_to_thumbnails_waiting_to_be_drawn.clear()
            try:
                panel._FadeThumbnails([media])
            finally:
                c.gui.IsCurrentPage = was
                T.ThumbnailMediaSingle.GetQtImage = original
            obj = panel._hashes_to_thumbnails_waiting_to_be_drawn.get(media.GetDisplayMediaResult().GetHash())
            canvas = QG.QImage(20, 20, QG.QImage.Format.Format_ARGB32_Premultiplied)
            canvas.fill(QG.QColor(0, 0, 0))
            samples = []
            kind = type(obj).__name__ if obj is not None else None
            for elapsed in ELAPSED:
                now[0] = 100.0 + elapsed
                due = obj.DrawDue()
                if due:
                    p = QG.QPainter(canvas)
                    obj.DrawToPainter(0, 0, p)
                    p.end()
                samples.append(dict(elapsed=elapsed, due=due, opacity=QG.QColor(canvas.pixel(10, 10)).red() / 255.0, complete=obj.DrawComplete()))
            panel._hashes_to_thumbnails_waiting_to_be_drawn.clear()
            return dict(fade=fade, object=kind, samples=samples)
        return with_clock(now, go)

    out = {}
    page_new, panel_new = open_page(True)
    out['new'] = [qt(lambda fade=fade: new_fade(panel_new, fade)) for fade in (True, False)]
    out['new_page_type'] = type(panel_new).__name__
    page_old, panel_old = open_page(False)
    out['old'] = [qt(lambda fade=fade: old_fade(panel_old, fade)) for fade in (True, False)]
    out['old_page_type'] = type(panel_old).__name__
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
