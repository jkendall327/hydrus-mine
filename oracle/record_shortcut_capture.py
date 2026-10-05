#!/usr/bin/env python3
"""Drive real Qt shortcut/command keyboard and mouse capture controls."""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def qt():
        from qtpy import QtCore as C, QtGui as G, QtWidgets as W
        from hydrus.client.gui import ClientGUIShortcuts as S, ClientGUIShortcutControls as E
        from hydrus.client import ClientApplicationCommand as A
        cases = []
        all_modifiers = C.Qt.KeyboardModifier.AltModifier | C.Qt.KeyboardModifier.ControlModifier | C.Qt.KeyboardModifier.ShiftModifier | C.Qt.KeyboardModifier.MetaModifier | C.Qt.KeyboardModifier.GroupSwitchModifier
        for merge in (True, False):
            panel = E.EditShortcutAndCommandPanel(session.controller.gui, S.Shortcut(), A.ApplicationCommand.STATICCreateSimpleCommand(A.SIMPLE_REFRESH), 'main_gui', False, merge)
            widget = panel._shortcut._keyboard_shortcut
            signals = [0]
            widget.valueChanged.connect(lambda: signals.__setitem__(0, signals[0]+1))
            steps = []
            try:
                for label, key, mods in [
                    ('letter all modifiers', C.Qt.Key.Key_A, all_modifiers),
                    ('uppercase unicode', C.Qt.Key.Key_Adiaeresis, C.Qt.KeyboardModifier.NoModifier),
                    ('casefold expansion first character', C.Qt.Key.Key_ssharp, C.Qt.KeyboardModifier.NoModifier),
                    ('numpad number', C.Qt.Key.Key_7, C.Qt.KeyboardModifier.KeypadModifier | C.Qt.KeyboardModifier.ControlModifier),
                    ('numpad return', C.Qt.Key.Key_Return, C.Qt.KeyboardModifier.KeypadModifier),
                    ('numpad enter', C.Qt.Key.Key_Enter, C.Qt.KeyboardModifier.KeypadModifier),
                    ('numpad arrow', C.Qt.Key.Key_Left, C.Qt.KeyboardModifier.KeypadModifier | C.Qt.KeyboardModifier.ShiftModifier),
                    ('modifier alone', C.Qt.Key.Key_Control, C.Qt.KeyboardModifier.ControlModifier),
                    ('backtab', C.Qt.Key.Key_Backtab, C.Qt.KeyboardModifier.ShiftModifier),
                    ('function 24', C.Qt.Key.Key_F24, C.Qt.KeyboardModifier.NoModifier),
                    ('null ignored', 0, C.Qt.KeyboardModifier.NoModifier),
                ]:
                    event = G.QKeyEvent(C.QEvent.Type.KeyPress, key, mods)
                    consumed = W.QApplication.sendEvent(widget, event)
                    shortcut, command = panel.GetValue()
                    steps.append(dict(label=label, qt_key=int(key), modifiers=int(mods.value), effective_modifiers=int(event.modifiers().value), consumed=consumed, signals=signals[0], value=shortcut.GetSerialisableTuple(), text=widget.text(), keyboard_selected=panel._shortcut._keyboard_radio.isChecked(), command=command.GetSerialisableTuple()))
                cases.append(dict(kind='keyboard', merge_numpad=merge, steps=steps))
            finally:
                panel.deleteLater()
        for primary in (False, True):
            panel = E.EditShortcutAndCommandPanel(session.controller.gui, S.Shortcut(), A.ApplicationCommand.STATICCreateSimpleCommand(A.SIMPLE_CLOSE_MEDIA_VIEWER), 'media_viewer', primary, True)
            widget = panel._shortcut._mouse_shortcut
            button = widget._button
            signals = [0]
            widget.valueChanged.connect(lambda: signals.__setitem__(0, signals[0]+1))
            S.GLOBAL_MOUSE_SCROLL_DELTA_FOR_TRACKPADS = 0
            steps = []
            try:
                events = [
                    ('left all modifiers', 'press', C.Qt.MouseButton.LeftButton, all_modifiers, 0),
                    ('right', 'press', C.Qt.MouseButton.RightButton, C.Qt.KeyboardModifier.NoModifier, 0),
                    ('middle', 'press', C.Qt.MouseButton.MiddleButton, C.Qt.KeyboardModifier.NoModifier, 0),
                    ('back', 'press', C.Qt.MouseButton.BackButton, C.Qt.KeyboardModifier.ControlModifier, 0),
                    ('forward', 'press', C.Qt.MouseButton.ForwardButton, C.Qt.KeyboardModifier.NoModifier, 0),
                    ('task', 'press', C.Qt.MouseButton.TaskButton, C.Qt.KeyboardModifier.NoModifier, 0),
                    ('release choice', 'choice', C.Qt.MouseButton.NoButton, C.Qt.KeyboardModifier.NoModifier, 1),
                    ('press ignored while release chosen', 'press', C.Qt.MouseButton.LeftButton, C.Qt.KeyboardModifier.NoModifier, 0),
                    ('left release', 'release', C.Qt.MouseButton.LeftButton, C.Qt.KeyboardModifier.ShiftModifier, 0),
                    ('right double', 'double', C.Qt.MouseButton.RightButton, C.Qt.KeyboardModifier.AltModifier, 0),
                    ('small wheel 1', 'wheel', C.Qt.MouseButton.NoButton, C.Qt.KeyboardModifier.NoModifier, 30),
                    ('small wheel 2', 'wheel', C.Qt.MouseButton.NoButton, C.Qt.KeyboardModifier.NoModifier, 30),
                    ('small wheel 3', 'wheel', C.Qt.MouseButton.NoButton, C.Qt.KeyboardModifier.NoModifier, 30),
                    ('small wheel exact threshold', 'wheel', C.Qt.MouseButton.NoButton, C.Qt.KeyboardModifier.NoModifier, 30),
                    ('small wheel exceeds threshold', 'wheel', C.Qt.MouseButton.NoButton, C.Qt.KeyboardModifier.ControlModifier, 30),
                    ('normal wheel down', 'wheel', C.Qt.MouseButton.NoButton, C.Qt.KeyboardModifier.ShiftModifier, -120),
                    ('horizontal wheel ignored', 'horizontal', C.Qt.MouseButton.NoButton, C.Qt.KeyboardModifier.NoModifier, 120),
                ]
                for label, kind, mouse_button, mods, delta in events:
                    if kind == 'choice':
                        widget._press_or_release.setCurrentIndex(delta)
                    elif kind in ('wheel', 'horizontal'):
                        angle = C.QPoint(0, delta) if kind == 'wheel' else C.QPoint(delta, 0)
                        event = G.QWheelEvent(C.QPointF(5,5), C.QPointF(5,5), C.QPoint(), angle, C.Qt.MouseButton.NoButton, mods, C.Qt.ScrollPhase.NoScrollPhase, False)
                        W.QApplication.sendEvent(button, event)
                    else:
                        event_type = {'press': C.QEvent.Type.MouseButtonPress, 'release': C.QEvent.Type.MouseButtonRelease, 'double': C.QEvent.Type.MouseButtonDblClick}[kind]
                        buttons = C.Qt.MouseButton.NoButton if kind == 'release' else mouse_button
                        event = G.QMouseEvent(event_type, C.QPointF(5,5), C.QPointF(5,5), mouse_button, buttons, mods)
                        W.QApplication.sendEvent(button, event)
                    shortcut, command = panel.GetValue()
                    steps.append(dict(label=label, kind=kind, button=int(mouse_button.value), modifiers=int(mods.value), delta=delta, signals=signals[0], value=shortcut.GetSerialisableTuple(), text=button.text(), mouse_selected=panel._shortcut._mouse_radio.isChecked(), press_release_enabled=widget._press_or_release.isEnabled(), choice=widget._press_or_release.currentIndex(), command=command.GetSerialisableTuple()))
                cases.append(dict(kind='mouse', primary_labels=primary, steps=steps))
            finally:
                panel.deleteLater()
        return dict(cases=cases, special_names=S.special_key_shortcut_str_lookup, mouse_names=S.shortcut_mouse_string_lookup, default=S.Shortcut().GetSerialisableTuple(), qt_special_keys={str(int(key)):value for key,value in S.special_key_shortcut_enum_lookup.items()}, modifier_masks={name:int(getattr(C.Qt.KeyboardModifier,name+'Modifier').value) for name in ('Control','Alt','Shift','Keypad','GroupSwitch','Meta')}, double_click_interval=W.QApplication.styleHints().mouseDoubleClickInterval(), double_click_distance=W.QApplication.styleHints().mouseDoubleClickDistance())
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv)>1:
        import record_api
        output=Path(sys.argv[2])
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as directory:
        output=Path(directory)/'capture.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result=json.loads(output.read_text())
    (HERE/'fixtures/shortcut_capture.json').write_text(json.dumps(result,indent=2)+'\n')


if __name__=='__main__':
    main()
