#!/usr/bin/env python3
"""Record what the preview window's background draws besides the ratings: the
trash and inbox icons and the location strings of `_DrawTopRight`, for the
first file of a search page in each of its states (in the inbox, archived,
trashed), and what each draw option does to them.

A private basic-fixture client opens a search page over its local files and
focuses the first file, so the page's real preview canvas shows it.
`_DrawBackgroundDetails` is run on an image the canvas's size with a painter
that records every pixmap drawn (the client's own trash or inbox pixmap, told
by identity) and `ClientGUIFunctions.DrawText` calls (the location strings).
`draw_top_right_hover_in_preview_window_background` is turned off and on.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_preview_background_details.py
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'preview_background_details.json')


def record(session):
    controller = session.controller
    gui = controller.gui
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    from qtpy import QtGui as QG
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIRatings
    from hydrus.client.gui.canvas import ClientGUICanvasHoverFrames as Hover
    from hydrus.client.metadata import ClientContentUpdates
    ContentUpdates = ClientContentUpdates

    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files']]
    qt(lambda: gui.resize(1100, 700))
    page = qt(lambda: gui._notebook.NewPageQuery(ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), initial_hashes=hashes))
    for _ in range(400):
        panel = qt(page.GetMediaResultsPanel)
        if hasattr(panel, '_thumbnail_layout') and len(panel._sorted_media) > 0:
            break
        time.sleep(0.05)
    else:
        raise RuntimeError('page did not load')
    time.sleep(0.5)
    canvas = page._preview_canvas
    hover = canvas._top_right_hover
    services = controller.services_manager
    likes = services.GetServices((HC.LOCAL_RATING_LIKE,))
    numericals = services.GetServices((HC.LOCAL_RATING_NUMERICAL,))
    incdecs = services.GetServices((HC.LOCAL_RATING_INCDEC,))
    media = qt(lambda: panel._sorted_media[0])
    file_hash = media.GetHash()

    def show():
        qt(lambda: panel._HitMedia(media, False, False))
        time.sleep(0.3)
        for _ in range(100):
            if qt(lambda: canvas.GetMedia()) is not None:
                break
            time.sleep(0.05)
        else:
            raise RuntimeError('the preview did not take the file')
        qt(lambda: QW.QApplication.processEvents())

    # what else the background draws: the trash and inbox icons and the
    # location strings, for the file in each of its states (the icons are
    # told by which of the client's pixmaps is drawn)
    from hydrus.client.gui import ClientGUIFunctions

    def details(media):
        draws = []
        pixmaps = {'trash': CC.global_pixmaps().trash, 'inbox': CC.global_pixmaps().inbox}

        class Proxy:
            def __init__(self, painter):
                self._painter = painter

            def __getattr__(self, name):
                return getattr(self._painter, name)

            def drawPixmap(self, x, y, pixmap, *a):
                name = next((k for (k, v) in pixmaps.items() if v.cacheKey() == pixmap.cacheKey()), 'other')
                draws.append({'kind': 'icon', 'which': name, 'x': x, 'y': y, 'size': [pixmap.width(), pixmap.height()]})
                return self._painter.drawPixmap(x, y, pixmap, *a)
        original_text = ClientGUIFunctions.DrawText

        def draw_text(painter, x, y, text, *a, **k):
            draws.append({'kind': 'text', 'x': x, 'y': y, 'text': text})
            return original_text(painter, x, y, text, *a, **k)
        ClientGUIFunctions.DrawText = draw_text
        try:
            image = QG.QImage(canvas.width(), canvas.height(), QG.QImage.Format.Format_ARGB32)
            image.fill(0)
            painter = QG.QPainter(image)
            try:
                canvas._DrawBackgroundDetails(Proxy(painter))
            finally:
                painter.end()
        finally:
            ClientGUIFunctions.DrawText = original_text
        return {'canvas': [canvas.width(), canvas.height()], 'inbox': bool(media.HasInbox()), 'trashed': bool(media.GetLocationsManager().IsTrashed()),
                'locations': list(media.GetLocationsManager().GetLocationStrings()), 'draws': draws}

    def local_file_domains():
        return [sv.GetServiceKey() for sv in services.GetServices((HC.LOCAL_FILE_DOMAIN,))]

    def set_state(update):
        # (the database's own update, then the file read again and shown)
        if update == 'trash':
            for key in local_file_domains():
                action = ContentUpdates.ContentUpdate(HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_DELETE, {file_hash}, reason='test')
                controller.WriteSynchronous('content_updates', ContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(key, action))
        else:
            action = ContentUpdates.ContentUpdate(HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_ARCHIVE if update == 'archive' else HC.CONTENT_UPDATE_INBOX, {file_hash})
            controller.WriteSynchronous('content_updates', ContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, action))
        fresh = controller.Read('media_results', [file_hash])[0]
        return ClientMediaSingle.MediaSingle(fresh)

    states = []
    from hydrus.client.media import ClientMediaSingle
    shown_media = [media]
    for name in ('inbox', 'archive', 'trash'):
        shown_media[0] = set_state(name)
        qt(lambda: canvas.SetMedia(shown_media[0]))
        states.append(dict(state=name, **qt(lambda: details(shown_media[0]))))
    shown_media[0] = set_state('inbox')

    opts = controller.new_options
    switches = []
    for draw in (True, False):
        opts.SetBoolean('draw_top_right_hover_in_preview_window_background', draw)
        qt(show)
        data = qt(lambda: details(media))
        switches.append(dict(draw=draw, drawn=len(data['draws'])))
    opts.SetBoolean('draw_top_right_hover_in_preview_window_background', True)

    # the pop-in: the hover is asked to show or hide itself (`DoRegularHideShow`)
    # with the mouse at points around where it pops in, the switch on and off
    from qtpy import QtCore as QC
    from hydrus.client.gui import ClientGUIFunctions as F
    opts.SetBoolean('hover_windows_need_window_focus_to_pop_in', False)
    opts.SetBoolean('preview_window_hover_top_right_shows_popup', True)
    qt(show)

    def rect_on():
        hover.adjustSize()
        (should_resize, size, pos) = hover._GetIdealSizeAndPosition()
        return QC.QRect(pos.x(), pos.y(), size.width(), size.height())
    rect = qt(rect_on)
    points = [(0, 0), (rect.width() - 1, rect.height() - 1), (rect.width() // 2, rect.height() // 2), (-1, 0), (rect.width(), rect.height() - 1), (-30, rect.height() + 30)]
    original_mouse = F.GetMousePos
    decisions = []
    try:
        for pop in (True, False):
            opts.SetBoolean('preview_window_hover_top_right_shows_popup', pop)
            qt(show)
            for (dx, dy) in points:
                def decide():
                    hover._LowerHover()
                    F.GetMousePos = lambda: canvas.mapToGlobal(QC.QPoint(rect.x() + dx, rect.y() + dy))
                    hover.DoRegularHideShow()
                    up = bool(hover._is_currently_up)
                    hover._LowerHover()
                    return up
                decisions.append({'pop_in': pop, 'from_left': dx, 'from_top': dy, 'pops_in': qt(decide)})
    finally:
        F.GetMousePos = original_mouse
    opts.SetBoolean('preview_window_hover_top_right_shows_popup', True)
    return {'file': file_hash.hex(), 'states': states, 'switches': switches, 'hover_rect': [rect.x(), rect.y(), rect.width(), rect.height()], 'canvas': [canvas.width(), canvas.height()], 'pop_in_decisions': decisions}


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
