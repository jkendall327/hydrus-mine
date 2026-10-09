#!/usr/bin/env python3
"""Record the reference's "Slideshow durations" option: how its text is read,
and what the viewer's slideshow menu and play/pause make of the saved list.

A private basic-fixture client builds the real `MediaViewerPanel`, types each
of a list of texts into its slideshow durations box and runs its
`UpdateOptions`, recording the durations the options hold afterwards (a text
that doesn't parse, or has none above zero, leaves the earlier list). With
that list saved, a media viewer (`CanvasMediaListBrowser`) is opened on the
fixture's files and its own `ShowMenuFromSignal` builds the slideshow submenu
(captured, not shown): each entry's text, separators as "---", checkable
entries as `{"check": text, "checked": bool}`. The slideshow is then
started at the list's first duration, as choosing that entry of the menu does,
and the menu recorded again; with an empty list, the viewer's play/pause is
pressed from fresh instead (the slideshow starts at one second).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_slideshow_durations.py
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'slideshow_durations.json')
MANIFEST = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))

TEXTS = [
    '1.0,5.0,10.0,30.0,60.0',
    '2.5,7',
    '0.1',
    '3',
    ' 4 , 5 ',
    '1,,2',
    '0',
    '-1, 5',
    '0, 3, -2, 8',
    'abc',
    '',
    '1e2',
    '0.0001,3600',
    '1.5,2.25,3.125,86400',
    '5,5,5',
    '59,60,61,90,120,3599,3600,3601',
    '0.5,0.25,0.75,0.9',
    '2,1',
    '7.0',
    '100000',
    '１',
    '1_0',
    '1__0',
    '_1',
    '1_',
    '+3',
    '.5',
    '5.',
    '٣',
    '1e',
    '2\u00a0',
    'nan',
    '1 2',
]


def tree(menu):
    out = []
    for action in menu.actions():
        if action.isSeparator():
            out.append('---')
        elif action.menu() is not None:
            out.append({'menu': action.text(), 'entries': tree(action.menu())})
        elif action.isCheckable():
            out.append({'check': action.text(), 'checked': action.isChecked()})
        else:
            out.append(action.text())
    return out


def record(session):
    controller = session.controller
    gui = controller.gui
    qt = lambda f: controller.CallBlockingToQt(gui, f)

    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUICore as CGC
    from hydrus.client.gui.canvas import ClientGUICanvas
    from hydrus.client.gui.canvas import ClientGUICanvasFrame
    from hydrus.client.gui.panels.options.MediaViewerPanel import MediaViewerPanel

    options = controller.new_options
    default = options.GetSlideshowDurations()
    hashes = [bytes.fromhex(f['hash']) for f in MANIFEST['files']]
    names = {bytes.fromhex(f['hash']): f['name'] for f in MANIFEST['files']}
    media_results = controller.Read('media_results', hashes)
    location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    in_viewer = [m for m in media_results if CC.LOCAL_FILE_SERVICE_KEY in m.GetLocationsManager().GetCurrent()]
    first = [m.GetHash() for m in in_viewer if names[m.GetHash()] == 'jpeg_00.jpg'][0]
    captured = []
    CGC.core().PopupMenu = lambda widget, menu: captured.append(menu)

    results = []
    for text in TEXTS:
        options.SetSlideshowDurations(list(default))
        # (an earlier list stays when this one can't be used)
        options.SetSlideshowDurations([4.0, 8.0])
        before = options.GetSlideshowDurations()

        def type_it():
            panel = MediaViewerPanel(gui)
            panel._slideshow_durations.setText(text)
            panel.UpdateOptions()
            panel.deleteLater()
            return options.GetSlideshowDurations()

        saved = qt(type_it)
        holder = {}

        def open_viewer():
            frame = ClientGUICanvasFrame.CanvasFrame(gui)
            canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, in_viewer, first)
            frame.SetCanvas(canvas)
            holder['frame'] = frame
            holder['canvas'] = canvas

        qt(open_viewer)
        time.sleep(0.6)
        canvas = holder['canvas']

        def slideshow_menu():
            captured.clear()
            canvas.ShowMenuFromSignal(None)
            for action in captured[0].actions():
                if action.menu() is not None and action.text() in ('start slideshow', 'slideshow running'):
                    return {'menu': action.text(), 'entries': tree(action.menu())}
            return None

        fresh = qt(slideshow_menu)

        def play():
            # (what choosing the menu's first duration does)
            canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_START_SLIDESHOW, saved[0]))
            return slideshow_menu()

        played = qt(play)

        def close():
            canvas._StopSlideshow()
            holder['frame'].close()

        qt(close)
        time.sleep(0.2)
        results.append({'text': text, 'before': before, 'saved': saved, 'fresh': fresh, 'played': played})

    # an empty list, as the options can be left by an old save
    options.SetSlideshowDurations([])
    holder = {}

    def open_viewer():
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, in_viewer, first)
        frame.SetCanvas(canvas)
        holder['frame'] = frame
        holder['canvas'] = canvas

    qt(open_viewer)
    time.sleep(0.6)
    canvas = holder['canvas']

    def menus():
        captured.clear()
        canvas.ShowMenuFromSignal(None)
        fresh = [{'menu': a.text(), 'entries': tree(a.menu())} for a in captured[0].actions() if a.menu() is not None and a.text() in ('start slideshow', 'slideshow running')][0]
        canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_PAUSE_PLAY_SLIDESHOW))
        captured.clear()
        canvas.ShowMenuFromSignal(None)
        played = [{'menu': a.text(), 'entries': tree(a.menu())} for a in captured[0].actions() if a.menu() is not None and a.text() in ('start slideshow', 'slideshow running')][0]
        canvas._StopSlideshow()
        holder['frame'].close()
        return {'fresh': fresh, 'played': played}

    empty = qt(menus)
    options.SetSlideshowDurations(default)
    return {'default': default, 'results': results, 'empty': empty}


def child(out):
    import hydrus_driver
    import record_api
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
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
