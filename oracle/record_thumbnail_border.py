#!/usr/bin/env python3
"""Record how the reference draws a thumbnail's border.

The real thumbnail panel on the `basic` fixture's local files, with the
thumbnail size fixed and `thumbnail_border` set to each of a series of
widths: each of the first few thumbnails is painted by the reference's own
`Thumbnail.GetQtImage` (the image the panel draws into its grid), and the
width of the border-coloured band is measured on each side along the middle
of the image (the run of pixels of the border colour from the edge), with the
image's size and the size of the thumbnail picture inside it. Both a
selected and an unselected thumbnail are painted.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_thumbnail_border.py
       (writes fixtures/thumbnail_border.json)
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'thumbnail_border.json')

BORDERS = [0, 1, 2, 3, 5, 20]
THUMBNAIL = (150, 125)
FILES = 3


def record(session):
    controller = session.controller
    gui = controller.gui
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui.pages import ClientGUIMediaResultsPanelThumbnails

    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    results = controller.Read('media_results', [bytes.fromhex(f['hash']) for f in manifest['files']])
    hashes = [r.GetHash() for r in results if r.GetLocationsManager().IsLocal() and not r.GetLocationsManager().IsTrashed()]
    qt(lambda: gui.resize(1000, 700))
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

    def work():
        media = list(panel._sorted_media)[:FILES]
        HC.options['thumbnail_dimensions'] = list(THUMBNAIL)
        controller.new_options.SetInteger('thumbnail_margin', 2)
        out = {'thumbnail': list(THUMBNAIL), 'media': [m.GetDisplayMedia().GetHash().hex() for m in media], 'borders': []}
        for border in BORDERS:
            controller.new_options.SetInteger('thumbnail_border', border)
            controller.thumbnails_cache.Clear()
            panel.NotifyNewThumbnailLayoutRules()
            panel.ThumbnailsReset()
            entry = {'border': border, 'images': []}
            for m in media:
                thumbnail = ClientGUIMediaResultsPanelThumbnails.ThumbnailMediaSingle(m.GetDisplayMedia().GetMediaResult())
                size = controller.thumbnails_cache.GetThumbnail(m.GetDisplayMedia().GetMediaResult()).GetSize()
                for selected in (False, True):
                    thumbnail._selected = selected
                    image = thumbnail.GetQtImage(thumbnail, panel, 1.0)
                    colour = panel.GetColour(CC.COLOUR_THUMB_BORDER_SELECTED if selected else CC.COLOUR_THUMB_BORDER).rgb()
                    (w, h) = (image.width(), image.height())

                    def run(points):
                        n = 0
                        for (x, y) in points:
                            if image.pixelColor(x, y).rgb() != colour:
                                break
                            n += 1
                        return n

                    entry['images'].append({
                        'selected': selected,
                        'size': [w, h],
                        'picture': list(size),
                        'left': run([(x, h // 2) for x in range(w)]),
                        'right': run([(w - 1 - x, h // 2) for x in range(w)]),
                        'top': run([(w // 2, y) for y in range(h)]),
                        'bottom': run([(w // 2, h - 1 - y) for y in range(h)]),
                    })
                thumbnail._selected = False
            out['borders'].append(entry)
        return out
    return qt(work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        from hydrus.client.gui.canvas import ClientGUIMPV
        ClientGUIMPV.MPV_IS_AVAILABLE = False
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
        json.dump(result, f, indent=1)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
