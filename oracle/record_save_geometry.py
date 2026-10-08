#!/usr/bin/env python3
"""Record the reference's `SaveTLWSizeAndPosition` (what a window remembers of
its size, place, maximised and fullscreen state when it closes).

The real function runs on a stand-in top-level window (it asks only
`isMinimized`, `isVisible`, `isMaximized`, `isFullScreen`, `frameGeometry`,
`size` and `screen`), over a synthetic two-monitor topology supplied through
`QApplication.screenAt` and `screens` (the same seam as `record_window_rescue.py`;
the offscreen platform has one screen). Each case sets the frame's remembered
options, the window's state, runs the function, and records the frame after.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_save_geometry.py
       (writes fixtures/save_geometry.json)
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def drive():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client.gui import ClientGUITopLevelWindows as W

        c = session.controller
        options = c.new_options
        original = options.Duplicate()
        topology = [
            dict(geometry=[0, 0, 800, 600], available=[0, 24, 800, 576]),
            dict(geometry=[-1000, 100, 800, 600], available=[-1000, 124, 800, 576]),
        ]

        class Screen:
            def __init__(self, v):
                self.value = v

            def geometry(self):
                return QC.QRect(*self.value['geometry'])

            def availableGeometry(self):
                return QC.QRect(*self.value['available'])

        screens = [Screen(v) for v in topology]

        class Window:
            def __init__(self, at, size, screen, minimised=False, visible=True, maximised=False, fullscreen=False):
                self.at, self._size, self._screen = at, size, screen
                self.minimised, self.visible = minimised, visible
                self.maximised, self.fullscreen = maximised, fullscreen

            def isMinimized(self):
                return self.minimised

            def isVisible(self):
                return self.visible

            def isMaximized(self):
                return self.maximised

            def isFullScreen(self):
                return self.fullscreen

            def frameGeometry(self):
                return QC.QRect(QC.QPoint(*self.at), QC.QSize(*self._size))

            def size(self):
                return QC.QSize(*self._size)

            def screen(self):
                return None if self._screen is None else screens[self._screen]

        key = 'manage_options_dialog'
        cases = []
        old_at, old_screens = QW.QApplication.screenAt, QW.QApplication.screens
        QW.QApplication.screenAt = staticmethod(lambda p: next((s for s in screens if s.geometry().contains(p)), None))
        QW.QApplication.screens = staticmethod(lambda: screens)
        try:
            def run(label, frame, window, disabled=False):
                options.SetBoolean('disable_get_safe_position_test', disabled)
                options.SetFrameLocation(key, *frame)
                W.SaveTLWSizeAndPosition(window, key)
                after = options.GetFrameLocation(key)
                cases.append(dict(
                    label=label,
                    before=list(frame),
                    window=dict(
                        position=list(window.at), size=list(window._size), screen=window._screen,
                        minimised=window.minimised, visible=window.visible,
                        maximised=window.maximised, fullscreen=window.fullscreen,
                    ),
                    disabled_position_test=disabled,
                    after=[list(v) if isinstance(v, (tuple, list)) else v for v in after],
                ))

            # (remember_size, remember_position, last_size, last_position, gravity, default_position, maximised, fullscreen)
            def f(rs=True, rp=True, size=(800, 600), pos=(20, 20), maximised=False, fullscreen=False):
                return (rs, rp, size, pos, (-1, -1), 'topleft', maximised, fullscreen)

            run('windowed on the first screen', f(), Window((100, 120), (640, 480), 0))
            run('windowed, remembers nothing', f(False, False), Window((100, 120), (640, 480), 0))
            run('windowed, remembers size only', f(True, False), Window((100, 120), (640, 480), 0))
            run('windowed, remembers position only', f(False, True), Window((100, 120), (640, 480), 0))
            run('windowed, no earlier size or place', f(size=None, pos=None), Window((100, 120), (640, 480), 0))
            run('windowed on the second screen', f(), Window((-900, 150), (640, 480), 1))
            run('was maximised, now windowed', f(maximised=True), Window((100, 120), (640, 480), 0))
            run('was maximised, now windowed, remembers no size', f(False, True, maximised=True), Window((100, 120), (640, 480), 0))
            run('off-screen, rescued onto a corner', f(), Window((-5000, -5000), (640, 480), 0))
            run('off-screen, rescued onto the first screen', f(), Window((-5000, -5000), (100, 100), 0))
            run('partly off the top left, kept', f(), Window((-30, -30), (640, 480), 0))
            run('off-screen with the position test disabled', f(), Window((-5000, -5000), (640, 480), 0), disabled=True)
            run('minimised: not saved', f(), Window((100, 120), (640, 480), 0, minimised=True))
            run('hidden: not saved', f(), Window((100, 120), (640, 480), 0, visible=False))
            run('maximised on the screen it was on', f(), Window((0, 0), (800, 600), 0, maximised=True))
            run('maximised with no earlier place', f(pos=None), Window((0, 0), (800, 600), 0, maximised=True))
            run('maximised with no earlier place or size', f(size=None, pos=None), Window((0, 0), (800, 600), 0, maximised=True))
            run('fullscreen on the screen it was on', f(), Window((0, 0), (800, 600), 0, fullscreen=True))
            run('maximised, remembers no position', f(True, False), Window((0, 0), (800, 600), 0, maximised=True))
            run('maximised, remembers no size', f(False, True), Window((0, 0), (800, 600), 0, maximised=True))
            run('maximised after moving to the second screen', f(pos=(100, 150)), Window((-1000, 100), (800, 600), 1, maximised=True))
            run('maximised on the second screen, its place fits there', f(pos=(100, 150), size=(300, 200)), Window((-1000, 100), (800, 600), 1, maximised=True))
            run('maximised on the second screen, its place cannot fit there', f(pos=(700, 500), size=(300, 200)), Window((-1000, 100), (800, 600), 1, maximised=True))
            run('maximised, earlier place on no screen', f(pos=(5000, 5000)), Window((-1000, 100), (800, 600), 1, maximised=True))
            run('fullscreen after moving to the second screen', f(pos=(100, 150), size=(300, 200)), Window((-1000, 100), (800, 600), 1, fullscreen=True))
            run('maximised and screenless', f(pos=(100, 150)), Window((0, 0), (800, 600), None, maximised=True))
            return dict(
                key=key,
                padding=options.GetInteger('forgive_frame_gubbins_fuzzy_padding'),
                fuzzy_relocate=options.GetBoolean('fuzzy_relocate_on_get_safe_position_test'),
                topology=topology,
                columns=['remember_size', 'remember_position', 'last_size', 'last_position', 'default_gravity', 'default_position', 'maximised', 'fullscreen'],
                cases=cases,
            )
        finally:
            QW.QApplication.screenAt, QW.QApplication.screens = old_at, old_screens
            c.new_options = original

    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as d:
        output = Path(d) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/save_geometry.json').write_text(json.dumps(result, indent=1) + '\n')
    print('wrote save_geometry.json')


if __name__ == '__main__':
    main()
