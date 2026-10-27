#!/usr/bin/env python3
"""Record the embed button (`EmbedButton`) of the media viewer and the
preview: a file whose kind starts "with embed button" waits behind it.

JPEGs and animated GIFs are set to start with the embed button in the media
viewer and in the preview (the options' media view entries); PNGs are not.
In a real CanvasMediaListBrowser over a JPEG, a GIF and a PNG, sized
1000x750: for each file shown, whether the embed button shows, its place
against the media container's, and which media window there is (none while
it waits). Then real presses (`QTest.mousePress`) on the embed button: a
right press (does the viewer's menu open, and does the button stay?) and a
left press (the button goes and the file's media window is made). Showing
another file and coming back shows the button again. Then the same in a real
search page's preview canvas, its thumbnail clicked.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_embed_button.py
(writes fixtures/embed_button.json)
"""

import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'embed_button.json')

FILES = ['jpeg_00.jpg', 'gif_animated.gif', 'png_00.png']


def settle(seconds=0.5):
    from qtpy import QtWidgets as QW
    deadline = time.time() + seconds
    while time.time() < deadline:
        QW.QApplication.processEvents()
        time.sleep(0.02)


def press(widget, button):
    from qtpy import QtCore as QC
    from qtpy import QtTest
    qt_button = {'left': QC.Qt.MouseButton.LeftButton, 'right': QC.Qt.MouseButton.RightButton}[button]
    QtTest.QTest.mousePress(widget, qt_button, QC.Qt.KeyboardModifier.NoModifier, widget.rect().center())
    QtTest.QTest.mouseRelease(widget, qt_button, QC.Qt.KeyboardModifier.NoModifier, widget.rect().center())


def record(session):
    controller = session.controller
    gui = controller.gui

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUICore as CGC
    from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame

    options = controller.new_options
    view = options.GetMediaViewOptions()
    for mime in (HC.IMAGE_JPEG, HC.ANIMATION_GIF):
        (m_action, m_paused, m_embed, p_action, p_paused, p_embed, zoom) = options._GetMediaViewOptions(mime)
        view[mime] = (m_action, m_paused, True, p_action, p_paused, True, zoom)
    options.SetMediaViewOptions(view)

    with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
        manifest = json.load(stream)
    by_name = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}
    hashes = [by_name[n] for n in FILES]

    menus = []
    CGC.core().PopupMenu = lambda widget, menu: menus.append([a.text() for a in menu.actions() if not a.isSeparator()])

    def container_state(canvas):
        container = canvas._media_container
        embed = container._embed_button
        media = canvas._current_media
        geometry = container.geometry()
        return {
            'shown': None if media is None else media.GetHash().hex(),
            'embed': not embed.isHidden(),
            'embed_rect': [embed.x(), embed.y(), embed.width(), embed.height()] if not embed.isHidden() else None,
            'container': [geometry.x(), geometry.y(), geometry.width(), geometry.height()],
            'media_window': None if container._media_window is None else type(container._media_window).__name__,
            'media_window_shown': container._media_window is not None and not container._media_window.isHidden(),
        }

    def drive():
        out = {'files': {n: by_name[n].hex() for n in FILES}}
        # the media viewer
        results = sorted(controller.Read('media_results', hashes), key=lambda r: r.GetFileInfoManager().size)
        out['order'] = [r.GetHash().hex() for r in results]
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, results, results[0].GetHash())
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 750)
        settle(1.0)

        def show(name):
            canvas.SetMedia(list(canvas._media_list.GetMediaByHashes({by_name[name]}))[0])
            settle()

        steps = []
        for name in FILES:
            show(name)
            steps.append(dict(do='show', file=name, **container_state(canvas)))
            embed = canvas._media_container._embed_button
            if embed.isHidden():
                continue
            for button in ('right', 'left'):
                menus.clear()
                press(embed, button)
                settle()
                steps.append(dict(do='press', button=button, file=name, menu=list(menus), **container_state(canvas)))
        # back to the first: it waits again
        show(FILES[0])
        steps.append(dict(do='show again', file=FILES[0], **container_state(canvas)))
        out['viewer'] = steps
        frame.hide()

        # the preview
        page = gui.NewPageQuery(location, initial_hashes=hashes, page_name='embed button', select_page=True)
        for _ in range(40):
            settle(0.1)
            if len(page.GetMediaResultsPanel().GetFlatMedia()) == len(hashes):
                break
        preview = page._preview_canvas
        panel = page.GetMediaResultsPanel()
        preview_steps = []
        for name in FILES:
            media = [m for m in panel.GetFlatMedia() if m.GetHash() == by_name[name]][0]
            preview.SetMedia(media)
            settle()
            preview_steps.append(dict(do='show', file=name, **container_state(preview)))
            embed = preview._media_container._embed_button
            if embed.isHidden():
                continue
            press(embed, 'left')
            settle()
            preview_steps.append(dict(do='press', button='left', file=name, **container_state(preview)))
        out['preview'] = preview_steps
        gui._notebook.CloseCurrentPage(polite=False)
        settle()
        return out
    return controller.CallBlockingToQt(gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'embed.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as stream:
            result = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(result, stream, indent=1)
        stream.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
