#!/usr/bin/env python3
"""Record the media viewer's navigation: first, previous, next, last (round the
ends), random and undo random, and remove file from current view.

A real CanvasMediaListBrowser over the basic fixture's JPEGs (every one the viewer displays whole) gets each simple
application command a shortcut would send (`ProcessApplicationCommand`), with
`random.randrange` scripted (a draw equal to the file shown is drawn again).
After each step: the index string the viewer shows ("3/14"), the position in
the list of the file shown, and the random draws used. The last remove empties the list, which closes the window.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_viewer_navigation.py
"""
import json
import os
import random
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

# the draws random navigation gets, in order; the second equals the file
# shown then (index 3), so is drawn again
DRAWS = [3, 3, 5, 0, 4]  # (of 14 files)

STEPS = ['next', 'next', 'previous', 'first', 'previous', 'next', 'last', 'next', 'previous',
         'random', 'random', 'undo random', 'undo random', 'undo random', 'undo random',
         'first', 'next', 'remove', 'last', 'remove'] + ['remove'] * 30


def record(session):
    controller = session.controller
    gui = controller.gui

    def drive():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientApplicationCommand as CAC, ClientConstants as CC, ClientLocation
        from hydrus.client.gui.canvas import ClientGUICanvas, ClientGUICanvasFrame
        with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
            manifest = json.load(stream)
        by_name = {file['name']: bytes.fromhex(file['hash']) for file in manifest['files']}
        names_in_order = [file['name'] for file in manifest['files'] if file['name'].endswith('.jpg')]
        hashes = [by_name[name] for name in names_in_order]
        results = controller.Read('media_results', hashes)
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        frame = ClientGUICanvasFrame.CanvasFrame(gui)
        canvas = ClientGUICanvas.CanvasMediaListBrowser(frame, os.urandom(32), location, results, hashes[0])
        frame.SetCanvas(canvas)
        frame.showNormal()
        QW.QApplication.processEvents()
        closes = []
        canvas._TryToCloseWindow = lambda: closes.append(True)
        commands = {
            'first': CAC.SIMPLE_VIEW_FIRST, 'last': CAC.SIMPLE_VIEW_LAST, 'next': CAC.SIMPLE_VIEW_NEXT,
            'previous': CAC.SIMPLE_VIEW_PREVIOUS, 'random': CAC.SIMPLE_VIEW_RANDOM,
            'undo random': CAC.SIMPLE_UNDO_RANDOM, 'remove': CAC.SIMPLE_REMOVE_FILE_FROM_VIEW,
        }
        draws = list(DRAWS)
        drawn = []

        def scripted(n):
            value = draws.pop(0)
            drawn.append(value)
            return value
        original = random.randrange
        random.randrange = scripted
        steps = []

        def state(step):
            media = canvas.GetMedia()
            empty = canvas._media_list.HasNoMedia()
            steps.append({'do': step, 'index': None if empty else canvas._GetIndexString(),
                          'shown': None if media is None or empty else hashes.index(media.GetHash()),
                          'drawn': list(drawn), 'closed': bool(closes)})
            drawn.clear()
        try:
            state('open')
            for step in STEPS:
                if closes:
                    break
                canvas.ProcessApplicationCommand(CAC.ApplicationCommand.STATICCreateSimpleCommand(commands[step]))
                QW.QApplication.processEvents()
                state(step)
        except Exception:
            print('STEPS SO FAR', json.dumps(steps[-5:]))
            raise
        finally:
            random.randrange = original
        return {'count': len(hashes), 'draws': DRAWS, 'steps': steps}
    return controller.CallBlockingToQt(gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'navigation.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as stream:
            result = json.load(stream)
    with open(os.path.join(HERE, 'fixtures', 'viewer_navigation.json'), 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote viewer_navigation.json')


if __name__ == '__main__':
    main()
