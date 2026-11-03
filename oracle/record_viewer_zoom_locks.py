#!/usr/bin/env python3
"""Record the media viewer top hover's zoom options (cog) menu and its move
button's resize menu, and what their entries do, on a real
`CanvasMediaListBrowser` (1000x750) over the basic fixture's files.

- `menu`: the zoom options menu's entries as chosen in turn (each by
  triggering its action, as a click does): each zoom type, with and without
  "set default zoom type from this menu" (the default zoom option after
  each), the three locks (the size and zoom type locks are exclusive) and
  recenter; the menu as it then shows.
- `next`: for each set of locks, the first file filling the window's width
  (`fill horizontally`), zoomed in and panned, then
  the next file shown: its zoom, zoom type and position.
- `resize`: the same, then the window resized, with recentering on resize
  on and off.
- `resize_menu`: the move button's right-click menu, and each entry chosen
  with the left button and with the right (the menu remembers which), from
  the first file zoomed in and panned: the window's geometry, the canvas,
  and the file's zoom and position after.

Time is held still. Usage:
QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_viewer_zoom_locks.py
(writes fixtures/viewer_zoom_locks.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'viewer_zoom_locks.json')

NOW = 1900000000

# the files, in the order the viewer shows them (by size, smallest first)
FILES = ['jpeg_00.jpg', 'png_00.png', 'jpeg_05.jpg', 'bitmap.bmp']

LOCKS = {
    'size': 'media_viewer_lock_current_zoom',
    'type': 'media_viewer_lock_current_zoom_type',
    'pan': 'media_viewer_lock_current_pan',
}
OPTIONS = dict(LOCKS, from_menu='media_viewer_set_default_viewer_zoom_type_from_menu', recenter_on_resize='media_viewer_recenter_media_on_window_resize')

MODES = [[], ['size'], ['type'], ['pan'], ['size', 'pan'], ['type', 'pan']]


def hold_time():
    from hydrus.core import HydrusTime
    HydrusTime.GetNow = lambda: NOW
    HydrusTime.GetNowMS = lambda: NOW * 1000
    HydrusTime.GetNowFloat = lambda: float(NOW)


def settle(seconds=0.3):
    from qtpy import QtWidgets as QW
    deadline = time.time() + seconds
    while time.time() < deadline:
        QW.QApplication.processEvents()
        time.sleep(0.02)


def describe_menu(menu):
    items = []
    for action in menu.actions():
        if action.isSeparator():
            items.append({'separator': True})
            continue
        item = {'text': action.text()}
        if action.isCheckable():
            item['checked'] = action.isChecked()
        if not action.isEnabled():
            item['enabled'] = False
        items.append(item)
    return items


def record(session):
    controller = session.controller
    gui = controller.gui
    hold_time()

    from qtpy import QtCore as QC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client.gui import ClientGUICore as CGC
    from hydrus.client.gui import ClientGUIMenus
    from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame

    with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
        manifest = json.load(stream)
    by_name = {f['name']: bytes.fromhex(f['hash']) for f in manifest['files']}
    hashes = [by_name[n] for n in FILES]
    options = controller.new_options

    def drive():
        menus = []
        core = CGC.core()
        core.PopupMenu = lambda widget, menu: menus.append(menu)
        results = sorted(controller.Read('media_results', hashes), key=lambda r: r.GetFileInfoManager().size)
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY)
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, results, results[0].GetHash())
        frame.SetCanvas(canvas)
        frame.showNormal()
        frame.resize(1000, 750)
        frame.move(100, 100)
        settle(1.0)
        hover = canvas._top_hover
        container = canvas._media_container
        out = {
            'files': [{'hash': r.GetHash().hex(), 'resolution': list(r.GetResolution()), 'mime': r.GetMime()} for r in results],
        }

        def opts():
            o = {k: options.GetBoolean(v) for (k, v) in OPTIONS.items()}
            o['default_zoom_type'] = options.GetInteger('media_viewer_default_zoom_type_override')
            return o

        def state():
            pos = container.pos()
            size = container.size()
            return {
                'shown': canvas._current_media.GetHash().hex(),
                'zoom': container._current_zoom,
                'zoom_type': container._current_zoom_type,
                'zoom_text': hover._zoom_text.text(),
                'position': [pos.x(), pos.y()],
                'size': [size.width(), size.height()],
                'canvas': [canvas.width(), canvas.height()],
            }

        def geometry():
            g = frame.geometry()
            return [g.x(), g.y(), g.width(), g.height()]

        def command(action, data=None):
            canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(action, data))
            settle(0.2)

        def show(i):
            canvas.SetMedia(canvas._media_list.GetMediaByHashes({results[i].GetHash()}).pop())
            settle(0.4)

        def zoom_menu():
            menus.clear()
            hover._ShowZoomOptionsMenu()
            return menus.pop()

        def choose(text, button=QC.Qt.MouseButton.LeftButton, menu=None):
            menu = menu or zoom_menu()
            for action in menu.actions():
                if action.text() == text:
                    ClientGUIMenus.LAST_MOUSE_CLICK_BUTTON = button
                    action.trigger()
                    ClientGUIMenus.LAST_MOUSE_CLICK_BUTTON = QC.Qt.MouseButton.LeftButton
                    settle(0.3)
                    return
            raise Exception('no ' + text)

        def set_locks(names):
            for (k, v) in LOCKS.items():
                options.SetBoolean(v, k in names)

        def worked():
            # the first file, filling the window's width, zoomed in and panned
            set_locks([])
            show(1)
            show(0)
            command(CAC.SIMPLE_ZOOM_CANVAS_FILL_X_VIEWER_CENTER)
            command(CAC.SIMPLE_ZOOM_IN_VIEWER_CENTER)
            command(CAC.SIMPLE_ZOOM_IN_VIEWER_CENTER)
            command(CAC.SIMPLE_PAN_RIGHT)
            command(CAC.SIMPLE_PAN_DOWN)

        # the menu's entries chosen in turn
        steps = [{'menu': describe_menu(zoom_menu()), 'state': state(), 'options': opts()}]
        for text in ['fill horizontally', 'set default zoom type from this menu', 'canvas fit', '100% zoom',
                     'default for filetype', 'set default zoom type from this menu', 'fill vertically',
                     'try to lock current size', 'lock current zoom type', 'try to lock current size',
                     'lock current pan', 'try to lock current size', 'lock current pan',
                     'recenter media on window resize', 'recenter media on window resize']:
            choose(text)
            steps.append({'chose': text, 'state': state(), 'options': opts(), 'menu': describe_menu(zoom_menu())})
        command(CAC.SIMPLE_PAN_RIGHT)
        steps.append({'panned': True, 'state': state()})
        choose('recenter media')
        steps.append({'chose': 'recenter media', 'state': state(), 'options': opts()})
        out['menu'] = steps
        options.SetInteger('media_viewer_default_zoom_type_override', 0)
        options.SetBoolean(OPTIONS['from_menu'], False)

        # going to the next file under each set of locks
        out['next'] = []
        for mode in MODES:
            worked()
            before = state()
            set_locks(mode)
            command(CAC.SIMPLE_VIEW_NEXT)
            after = state()
            command(CAC.SIMPLE_VIEW_NEXT)
            out['next'].append({'locks': mode, 'before': before, 'after': after, 'then': state()})
        set_locks([])

        # resizing the window under each set of locks
        out['resize'] = []
        for recenter in (True, False):
            for mode in MODES:
                options.SetBoolean(OPTIONS['recenter_on_resize'], True)
                frame.resize(1000, 750)
                settle(0.5)
                worked()
                before = state()
                set_locks(mode)
                options.SetBoolean(OPTIONS['recenter_on_resize'], recenter)
                frame.resize(800, 600)
                settle(0.5)
                out['resize'].append({'recenter_on_resize': recenter, 'locks': mode, 'before': before, 'after': state()})
                set_locks([])
        options.SetBoolean(OPTIONS['recenter_on_resize'], True)
        frame.resize(1000, 750)
        settle(0.5)

        # the move button's resize menu
        menus.clear()
        hover._ShowWindowResizeOptionsMenu()
        resize_menu = menus.pop()
        out['resize_menu'] = {'menu': describe_menu(resize_menu), 'choices': []}
        for text in ['resize to fit', 'resize to 50%', 'resize to 75%', 'resize to 100%', 'resize to 150%', 'resize to 200%']:
            for (name, button) in (('left', QC.Qt.MouseButton.LeftButton), ('right', QC.Qt.MouseButton.RightButton)):
                frame.showNormal()
                frame.resize(1000, 750)
                frame.move(100, 100)
                settle(0.5)
                worked()
                before = {'state': state(), 'geometry': geometry()}
                menus.clear()
                hover._ShowWindowResizeOptionsMenu()
                choose(text, button, menus.pop())
                settle(0.5)
                out['resize_menu']['choices'].append({'chose': text, 'button': name, 'before': before, 'state': state(), 'geometry': geometry()})
        frame.hide()
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
    import tempfile
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as stream:
            recorded = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(recorded, stream, indent=1)
        stream.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
