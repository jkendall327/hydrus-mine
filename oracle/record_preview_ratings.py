#!/usr/bin/env python3
"""Record the reference's preview window drawing its ratings at the preview
window's rating icon sizes, and its top-right hover popping in or not.

A private basic-fixture client opens a search page over its local files and
focuses the first file, so the page's real preview canvas
(`CanvasPanelWithHovers`) shows it. The file is given ratings: every like
service liked, every numerical service at a few stars, every inc/dec service at
a value. For each of a range of settings of `preview_window_rating_icon_size_px`
and `preview_window_rating_incdec_height_px` (the options dialog's spin boxes
allow 1 to 255 and 2 to 255, and the settings are floats), and for inc/dec
values of 0, 5 and 12345:

- the background draw: `_DrawTopRight` is run on an image the canvas's size, and
  every call it makes to the rating drawing functions is recorded (the position
  and size it draws each rating at, and the width it reserves for each numerical
  rating);
- the pop-in: the hover's rating controls (`RatingLikeCanvas`,
  `RatingNumericalCanvas`, `RatingIncDecCanvas`) are asked their size after the
  options are re-read, as when the hover pops in.

Then the two switches, `draw_top_right_hover_in_preview_window_background` and
`preview_window_hover_top_right_shows_popup`, are turned off and on, and what
the canvas draws and whether the hover can pop in is recorded.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_preview_ratings.py
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'preview_ratings.json')

# ( icon size, incdec height )
SIZES = [
    (12.0, 12.0),
    (30.0, 17.5),
    (30.0, 16.5),
    (14.4, 14.6),
    (14.5, 15.5),
    (1.0, 2.0),
    (255.0, 255.0),
    (2.5, 3.5),
    (13.49, 13.51),
    (64.0, 8.0),
]
INCDEC_VALUES = [0, 5, 12345]


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

    def rate(incdec_value):
        updates = []
        for s in likes:
            updates.append((s.GetServiceKey(), 1.0))
        for s in numericals:
            updates.append((s.GetServiceKey(), 0.6))
        for s in incdecs:
            updates.append((s.GetServiceKey(), incdec_value))
        for (key, value) in updates:
            update = ClientContentUpdates.ContentUpdate(HC.CONTENT_TYPE_RATINGS, HC.CONTENT_UPDATE_ADD, (value, {file_hash}))
            controller.WriteSynchronous('content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate(key, update))
            # (the media the preview shows is told, as the client's GUI does)
            qt(lambda: media.GetRatingsManager().ProcessContentUpdate(key, update))

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

    names = {s.GetServiceKey(): s.GetName() for s in likes + numericals + incdecs}

    def background():
        calls = []
        originals = (ClientGUIRatings.DrawLike, ClientGUIRatings.DrawNumerical, ClientGUIRatings.DrawIncDec, ClientGUIRatings.GetNumericalWidth)

        def size_of(s):
            return None if s is None else [s.width(), s.height()]

        def draw_like(painter, x, y, service_key, rating_state, size=None, *a, **k):
            calls.append({'kind': 'like', 'service': names[service_key], 'x': x, 'y': y, 'size': size_of(size), 'state': rating_state})
            return originals[0](painter, x, y, service_key, rating_state, size, *a, **k) if size is not None else originals[0](painter, x, y, service_key, rating_state)

        def draw_numerical(painter, x, y, service_key, rating_state, rating, size=None, pad_px=None, *a, **k):
            calls.append({'kind': 'numerical', 'service': names[service_key], 'x': x, 'y': y, 'size': size_of(size), 'pad': pad_px, 'state': rating_state, 'rating': rating})
            return originals[1](painter, x, y, service_key, rating_state, rating, size=size, pad_px=pad_px, *a, **k)

        def draw_incdec(painter, x, y, service_key, rating_state, rating, size=None, *a, **k):
            calls.append({'kind': 'incdec', 'service': names[service_key], 'x': x, 'y': y, 'size': size_of(size), 'state': rating_state, 'rating': rating})
            return originals[2](painter, x, y, service_key, rating_state, rating, size, *a, **k)

        def numerical_width(service_key, star_width, pad_px=None, draw_collapsed=False, rating_state=None, rating=None):
            result = originals[3](service_key, star_width, pad_px, draw_collapsed, rating_state, rating) if rating_state is not None else originals[3](service_key, star_width, pad_px, draw_collapsed)
            calls.append({'kind': 'numerical_width', 'service': names[service_key], 'star_width': star_width, 'width': result})
            return result

        ClientGUIRatings.DrawLike, ClientGUIRatings.DrawNumerical, ClientGUIRatings.DrawIncDec, ClientGUIRatings.GetNumericalWidth = draw_like, draw_numerical, draw_incdec, numerical_width
        try:
            image = QG.QImage(canvas.width(), canvas.height(), QG.QImage.Format.Format_ARGB32)
            image.fill(0)
            painter = QG.QPainter(image)
            try:
                canvas._DrawBackgroundDetails(painter)
            finally:
                painter.end()
        finally:
            ClientGUIRatings.DrawLike, ClientGUIRatings.DrawNumerical, ClientGUIRatings.DrawIncDec, ClientGUIRatings.GetNumericalWidth = originals
        return {'canvas': [canvas.width(), canvas.height()], 'calls': calls}

    def popped():
        controller.pub('notify_new_options')
        for _ in range(10):
            QW.QApplication.processEvents()
        # (the like and numerical controls take their new size as they are painted)
        for _ in range(3):
            hover.grab()
            for _ in range(5):
                QW.QApplication.processEvents()
        hover.layout().activate()
        out = []
        for control in hover.findChildren(Hover.ClientGUIRatings.QW.QWidget):
            kind = None
            if isinstance(control, Hover.RatingLikeCanvas):
                kind = 'like'
            elif isinstance(control, Hover.RatingNumericalCanvas):
                kind = 'numerical'
            elif isinstance(control, Hover.RatingIncDecCanvas):
                kind = 'incdec'
            if kind is None:
                continue
            hint = control.sizeHint()
            minimum = control.minimumSize()
            size = control.size()
            out.append({'kind': kind, 'service': names[control.GetServiceKey()], 'hint': [hint.width(), hint.height()], 'minimum': [minimum.width(), minimum.height()], 'size': [size.width(), size.height()]})
        return out

    opts = controller.new_options
    results = []
    for incdec_value in INCDEC_VALUES:
        rate(incdec_value)
        for (icon, height) in SIZES:
            opts.SetFloat('preview_window_rating_icon_size_px', icon)
            opts.SetFloat('preview_window_rating_incdec_height_px', height)
            qt(show)
            results.append({
                'icon': icon,
                'incdec_height': height,
                'incdec_value': incdec_value,
                'background': qt(background),
                'popped': qt(popped),
            })
    opts.SetFloat('preview_window_rating_icon_size_px', 12.0)
    opts.SetFloat('preview_window_rating_incdec_height_px', 12.0)

    switches = []
    for draw in (True, False):
        for pop in (True, False):
            opts.SetBoolean('draw_top_right_hover_in_preview_window_background', draw)
            opts.SetBoolean('preview_window_hover_top_right_shows_popup', pop)
            qt(show)
            data = qt(background)
            switches.append({
                'draw': draw,
                'pop_in': pop,
                'calls': len(data['calls']),
            })
    opts.SetBoolean('draw_top_right_hover_in_preview_window_background', True)
    opts.SetBoolean('preview_window_hover_top_right_shows_popup', True)

    # the pop-in switch: where the hover would be when the mouse is over it
    # (`_GetIdealSizeAndPosition`, whose rectangle `DoRegularHideShow` tests
    # the mouse against), and which edge-adjacent points it contains
    from qtpy import QtCore as QC
    popins = []
    for pop in (True, False):
        opts.SetBoolean('preview_window_hover_top_right_shows_popup', pop)
        qt(show)

        def ideal():
            hover.adjustSize()
            (should_resize, size, pos) = hover._GetIdealSizeAndPosition()
            rect = QC.QRect(pos.x(), pos.y(), size.width(), size.height())
            points = []
            for (dx, dy) in [(0, 0), (-1, 0), (0, -1), (rect.width() - 1, rect.height() - 1), (rect.width(), rect.height() - 1), (rect.width() - 1, rect.height()), (rect.width() // 2, rect.height() // 2), (-30, rect.height() + 30)]:
                point = QC.QPoint(rect.x() + dx, rect.y() + dy)
                points.append({'from_left': dx, 'from_top': dy, 'contains': rect.contains(point)})
            return {
                'pop_in': pop,
                'canvas': [canvas.width(), canvas.height()],
                'hover': [hover.width(), hover.height()],
                'should_resize': bool(should_resize),
                'ideal_size': [size.width(), size.height()],
                'ideal_from_right': canvas.width() - (pos.x() + size.width()),
                'ideal_y': pos.y(),
                'points': points,
            }

        popins.append(qt(ideal))
    opts.SetBoolean('preview_window_hover_top_right_shows_popup', True)
    return {
        'popins': popins,
        'services': {'likes': [s.GetName() for s in likes], 'numericals': [s.GetName() for s in numericals], 'incdecs': [s.GetName() for s in incdecs]},
        'results': results,
        'switches': switches,
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
