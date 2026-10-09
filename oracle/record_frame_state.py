#!/usr/bin/env python3
"""Record the reference opening and closing a window with its frame location's
"start maximised" and "start fullscreen" switches.

In the running client, on the `basic` fixture, a window (standing in for the
media viewer's, with the `media_viewer` frame key's setting) is opened by
`SetInitialTLWSizeAndPosition` for each of: every combination of the two
switches, remembering the size and not, with a remembered size and place, and
each default gravity. After it is shown its maximised and fullscreen state and
size are recorded. Then, as the window closes, `SaveTLWSizeAndPosition` runs
(first as it is, then after the user has restored it to a normal window, or
maximised or fullscreened it by hand) and the frame location is recorded as it
is left.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_frame_state.py
"""
import itertools
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'frame_state.json')

KEY = 'media_viewer'
MAIN = (120, 80, 900, 700)


def record(session):
    controller = session.controller
    gui = controller.gui
    out = {'cases': []}

    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUITopLevelWindows as T

        gui.showNormal()
        gui.resize(MAIN[2], MAIN[3])
        gui.move(MAIN[0], MAIN[1])
        QW.QApplication.processEvents()
        screen = gui.screen().geometry()
        out['screen'] = [screen.width(), screen.height()]
        options = controller.new_options

        cases = out['cases']
        for (remember_size, maximised, fullscreen, last_position) in itertools.product((True, False), (True, False), (True, False), (None, (33, 44))):
            for restore in ('as it is', 'restored', 'maximised by hand', 'fullscreened by hand'):
                if restore != 'as it is' and not remember_size:
                    continue
                options.SetFrameLocation(KEY, remember_size, last_position is not None, (700, 500), last_position, (-1, -1), 'topleft', maximised, fullscreen)
                window = QW.QWidget()
                window.setMinimumSize(300, 200)
                T.SetInitialTLWSizeAndPosition(window, KEY)
                window.show()
                for _ in range(5):
                    QW.QApplication.processEvents()
                opened = {'maximised': window.isMaximized(), 'fullscreen': window.isFullScreen(), 'size': [window.width(), window.height()]}
                if restore == 'restored':
                    window.showNormal()
                elif restore == 'maximised by hand':
                    window.showMaximized()
                elif restore == 'fullscreened by hand':
                    window.showFullScreen()
                for _ in range(5):
                    QW.QApplication.processEvents()
                after_user = {'maximised': window.isMaximized(), 'fullscreen': window.isFullScreen(), 'size': [window.width(), window.height()]}
                T.SaveTLWSizeAndPosition(window, KEY)
                (rs, rp, ls, lp, gravity, position, m, f) = options.GetFrameLocation(KEY)
                cases.append({
                    'frame': {'remember_size': remember_size, 'maximised': maximised, 'fullscreen': fullscreen, 'last_size': [700, 500], 'last_position': list(last_position) if last_position else None},
                    'opened': opened,
                    'user': restore,
                    'after_user': after_user,
                    'saved': {'remember_size': rs, 'last_size': list(ls) if ls else None, 'last_position': list(lp) if lp else None, 'maximised': m, 'fullscreen': f},
                })
                window.close()
                window.deleteLater()
        options.SetFrameLocation(KEY, True, True, (640, 480), None, (-1, -1), 'topleft', True, False)

    controller.CallBlockingToQt(gui, work)
    return out


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
        json.dump(result, f, indent=1)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
