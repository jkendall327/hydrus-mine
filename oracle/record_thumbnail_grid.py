#!/usr/bin/env python3
"""Record the reference's thumbnail grid under the thumbnail size, border and
margin options.

A private basic-fixture client opens a search page over every local fixture file (not trashed) in
its real thumbnail panel (the graphics-view one, the
reference's default). For each of a range of option sets (the
thumbnail box from its minimum 20x20 to 512 wide, borders 0 to 20 and margins
0 to 20, as the options dialog's Apply leaves them: the thumbnail cache cleared
and the layout rules renewed), the main window is set to a series of widths
and, at each, the panel's viewport, span, columns, rows per page, scene size and first
thumbnails' rectangles are recorded. At the fifth width, the file (or none) under each of a set
of points around the margins of the first two columns and rows is recorded, as
the panel's own `_GetThumbnailUnderMouse` finds it. For each thumbnail box, the
size of every file's thumbnail as the thumbnail cache gives it to the grid is
recorded too.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_thumbnail_grid.py
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'thumbnail_grid.json')

# ( width, height, border, margin )
CONFIGS = [
    (150, 125, 1, 2),
    (150, 125, 0, 0),
    (150, 125, 3, 20),
    (150, 125, 20, 0),
    (150, 125, 0, 20),
    (20, 20, 0, 0),
    (20, 20, 20, 20),
    (300, 250, 1, 2),
    (100, 200, 5, 7),
    (512, 64, 2, 1),
]

WIDTHS = [700, 853, 1000, 1100, 1317, 1600]


def record(session):
    controller = session.controller
    gui = controller.gui
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation

    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    # (the files a search of local files shows: not trashed, not deleted)
    results = controller.Read('media_results', [bytes.fromhex(f['hash']) for f in manifest['files']])
    hashes = [r.GetHash() for r in results if r.GetLocationsManager().IsLocal() and not r.GetLocationsManager().IsTrashed()]
    qt(lambda: gui.resize(WIDTHS[0], 700))
    page = qt(lambda: gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), initial_hashes=hashes))
    panel = None
    for _ in range(400):
        panel = qt(page.GetMediaResultsPanel)
        if hasattr(panel, '_thumbnail_layout') and len(panel._sorted_media) == len(hashes):
            break
        time.sleep(0.05)
    else:
        raise RuntimeError('thumbnail page did not load')
    time.sleep(0.5)
    media = qt(lambda: list(panel._sorted_media))
    index = {m: i for (i, m) in enumerate(media)}

    def settle():
        for _ in range(5):
            QW.QApplication.processEvents()

    def geometry():
        settle()
        layout = panel._thumbnail_layout
        viewport = panel.viewport().size()
        scene = panel.sceneRect()
        first = [panel._media_to_thumbnails[m] for m in media[:3]]
        return {
            'viewport': [viewport.width(), viewport.height()],
            'span': [layout._col_width, layout._row_height],
            'columns': layout._thumbs_in_a_row,
            'rows_per_page': layout._thumbs_in_a_col,
            'scene': [scene.width(), scene.height()],
            'first_thumbnails': [[t.pos().x(), t.pos().y(), t.width, t.height] for t in first],
        }

    def under(x, y):
        # (what the panel's mouse handling finds: the item at a viewport point)
        item = panel.itemAt(QC.QPoint(x, y))
        if item is None:
            return None
        return index[item.media]

    configs = []
    sizes_by_box = {}
    for (w, h, border, margin) in CONFIGS:

        def apply():
            HC.options['thumbnail_dimensions'] = [w, h]
            controller.new_options.SetInteger('thumbnail_border', border)
            controller.new_options.SetInteger('thumbnail_margin', margin)
            controller.thumbnails_cache.Clear()
            panel.NotifyNewThumbnailLayoutRules()
            panel.ThumbnailsReset()

        qt(apply)
        time.sleep(0.2)
        layouts = []
        hits = []
        for width in WIDTHS:
            qt(lambda: gui.resize(width, 700))
            time.sleep(0.1)
            g = qt(geometry)
            g['window'] = width
            layouts.append(g)
            if width == WIDTHS[4]:
                (sw, sh) = g['span']

                def hit_test():
                    qt_m = margin
                    xs = sorted({k * sw + d for k in (0, 1) for d in (0, qt_m - 1, qt_m, qt_m + 1, sw // 2, sw - qt_m - 1, sw - qt_m, sw - qt_m + 1) if k * sw + d >= 0})
                    ys = sorted({k * sh + d for k in (0, 1) for d in (0, qt_m - 1, qt_m, qt_m + 1, sh // 2, sh - qt_m - 1, sh - qt_m, sh - qt_m + 1) if k * sh + d >= 0})
                    panel.verticalScrollBar().setValue(0)
                    settle()
                    return [[x, y, under(x, y)] for x in xs for y in ys]

                hits = qt(hit_test)
        configs.append({'width': w, 'height': h, 'border': border, 'margin': margin, 'layouts': layouts, 'hits': hits})
        if (w, h) not in sizes_by_box:

            def sizes():
                out = []
                for m in media:
                    bitmap = controller.thumbnails_cache.GetThumbnail(m.GetDisplayMedia().GetMediaResult())
                    out.append(list(bitmap.GetSize()))
                return out

            sizes_by_box[(w, h)] = qt(sizes)
    hashes_shown = [m.GetDisplayMedia().GetHash().hex() for m in media]
    return {
        'media': hashes_shown,
        'configs': configs,
        'thumbnail_sizes': [{'width': w, 'height': h, 'sizes': s} for ((w, h), s) in sizes_by_box.items()],
    }


def child(out):
    import hydrus_driver
    import record_api
    from hydrus.client.gui.canvas import ClientGUIMPV
    ClientGUIMPV.MPV_IS_AVAILABLE = False
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
