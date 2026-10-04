#!/usr/bin/env python3
"""Record actual Notes Options, Manage Notes cog/cursors/copy and hover copy.

Real QAction checks invert real global options; actual NotesPanel UpdateOptions,
serialization and reopen are recorded. Existing editor cursors survive cog flips
and tab switches while future note controls use the new initial position. Real
copy buttons and middle mouse events on NotePanel emit clipboard outputs. Real
cog mouse buttons record supported opening routes and fresh live checks. Only
popup presentation, clipboard publication/notifications and cancellation are observed;
no note handlers or registered media content are replaced or committed.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'notes_preferences.json')
KEYS = ['copy_notes_dialog_copy_all', 'copy_notes_dialog_copy_json',
        'start_note_editing_at_end', 'copy_notes_quick_click_only_copies_text']
NOTES = {'note10': '  ten\r\nurl https://notes.example/item  \n\n',
         'note2': 'two 🦊', 'empty': '\n   '}


def record(session):
    def work():
        from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW, QtTest as QT
        from hydrus.core import HydrusSerialisable
        from hydrus.client.gui import ClientGUICore, ClientGUIDialogsQuick, ClientGUIMenus, ClientGUITopLevelWindowsPanels
        from hydrus.client.gui.widgets import ClientGUICommon, ClientGUIMenuButton
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit
        from hydrus.client.gui.panels.options.NotesPanel import NotesPanel
        from hydrus.client.gui.canvas.ClientGUICanvasHoverFrames import NotePanel
        c = session.controller
        original = c.new_options.Duplicate()
        old_pub, old_notice = c.pub, ClientGUICommon.IconButton.ShowMicroNotification
        old_clipboard = c.GetClipboardText
        old_question = ClientGUIDialogsQuick.GetYesNo
        clipboard, notices, questions = [], [], []
        def pub(topic, *args, **kwargs):
            if topic == 'clipboard':
                clipboard.append(args[1])
            else:
                old_pub(topic, *args, **kwargs)
        c.pub = pub
        ClientGUICommon.IconButton.ShowMicroNotification = lambda self, text: notices.append(text)
        def question(parent, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted
        ClientGUIDialogsQuick.GetYesNo = question
        def settle():
            QT.QTest.qWait(30)
            QW.QApplication.processEvents()
        def preferences():
            return [c.new_options.GetBoolean(key) for key in KEYS]
        def make_dialog():
            dialog = ClientGUITopLevelWindowsPanels.DialogEdit(c.gui, 'manage notes')
            panel = ClientGUIScrolledPanelsEdit.EditFileNotesPanel(dialog, NOTES, 'note2')
            dialog.SetPanel(panel)
            dialog.setModal(True)
            dialog.show()
            settle()
            return dialog, panel
        def menu(panel):
            result = ClientGUIMenus.GenerateMenu(panel._cog_button)
            ClientGUIMenuButton.PopulateMenuFromTemplateItems(result, panel._cog_button._menu_template_items)
            return result
        def menu_state(panel):
            result = menu(panel)
            rows = [dict(label=a.text(), separator=a.isSeparator(), checkable=a.isCheckable(),
                         checked=a.isChecked(), enabled=a.isEnabled(), tooltip=a.statusTip())
                    for a in result.actions()]
            result.deleteLater()
            return rows
        def set_cog(panel, index, desired):
            result = menu(panel)
            checks = [a for a in result.actions() if a.isCheckable()]
            if checks[index].isChecked() != desired:
                checks[index].trigger()
                settle()
            result.deleteLater()
        def cursor_state(panel):
            notebook = panel._notebook
            return [dict(name=notebook.tabText(i), text=notebook.widget(i).toPlainText(),
                         cursor=notebook.widget(i).textCursor().position(),
                         anchor=notebook.widget(i).textCursor().anchor()) for i in range(notebook.count())]
        dialogs = []
        try:
            initial = preferences()
            option_events = []
            labels = []
            for values in ([False, True], [True, False]):
                page = NotesPanel(c.gui, c.new_options)
                labels = [w.text() for w in page.findChildren(QW.QLabel)]
                before = preferences()
                page._start_note_editing_at_end.setChecked(values[0])
                page._copy_notes_quick_click_only_copies_text.setChecked(values[1])
                staged = preferences()
                page.UpdateOptions()
                encoded = HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
                reopened = NotesPanel(c.gui, c.new_options)
                option_events.append(dict(input=values, before=before, staged=staged, saved=preferences(),
                                          round_trip=[encoded.GetBoolean(key) for key in KEYS],
                                          reopened=[reopened._start_note_editing_at_end.isChecked(),
                                                    reopened._copy_notes_quick_click_only_copies_text.isChecked()]))
                page.deleteLater()
                reopened.deleteLater()
            abandoned = NotesPanel(c.gui, c.new_options)
            abandoned._start_note_editing_at_end.setChecked(False)
            abandoned._copy_notes_quick_click_only_copies_text.setChecked(True)
            abandoned.deleteLater()
            options_cancelled = preferences()
            dialog, panel = make_dialog()
            dialogs.append(dialog)
            initial_menu = menu_state(panel)
            core = ClientGUICore.core()
            old_popup = core.PopupMenu
            opened = []
            def popup(widget, result):
                assert widget is panel._cog_button
                opened.append([a.isChecked() for a in result.actions() if a.isCheckable()])
                result.deleteLater()
            core.PopupMenu = popup
            cog_openings = []
            try:
                for button, changed in ((QC.Qt.MouseButton.RightButton, False),
                                        (QC.Qt.MouseButton.LeftButton, False),
                                        (QC.Qt.MouseButton.RightButton, True),
                                        (QC.Qt.MouseButton.LeftButton, True)):
                    # Simulate another preference owner writing after this editor opened.
                    c.new_options.SetBoolean(KEYS[2], not initial[2] if changed else initial[2])
                    start = len(opened)
                    QT.QTest.mouseClick(panel._cog_button, button)
                    settle()
                    cog_openings.append(dict(button='right' if button == QC.Qt.MouseButton.RightButton else 'left',
                                             changed=changed, preferences=preferences(), menus=opened[start:]))
            finally:
                core.PopupMenu = old_popup
                c.new_options.SetBoolean(KEYS[2], initial[2])
            initial_cursors = cursor_state(panel)
            control = panel._notebook.currentWidget()
            cursor = control.textCursor()
            cursor.setPosition(1)
            control.setTextCursor(cursor)
            panel._notebook.setCurrentIndex(1)
            panel._notebook.setCurrentIndex(2)
            settle()
            switched_cursors = cursor_state(panel)
            set_cog(panel, 2, False)
            cursors_after_flip = cursor_state(panel)
            future_paste = json.dumps({'future': 'future 🦊 note'})
            c.GetClipboardText = lambda: future_paste
            panel._paste_button.click()
            settle()
            paste_notice = notices[-1]
            future_cursor = cursor_state(panel)[-1]
            QT.QTest.keyClicks(panel._notebook.currentWidget(), 'X')
            inserted = cursor_state(panel)[-1]
            panel._notebook.setCurrentIndex(2)
            copies = []
            for all_notes, as_json in ((True, True), (False, True), (True, False), (False, False)):
                set_cog(panel, 0, all_notes)
                set_cog(panel, 1, as_json)
                start_clipboard, start_notices = len(clipboard), len(notices)
                panel._copy_button.click()
                settle()
                copies.append(dict(all=all_notes, json=as_json, notes=panel.GetValue()[0],
                                   current=panel._notebook.tabText(panel._notebook.currentIndex()),
                                   clipboard=clipboard[start_clipboard:], notice=notices[start_notices:]))
            panel._notebook.setCurrentIndex(0)
            for as_json in (True, False):
                set_cog(panel, 1, as_json)
                start_clipboard, start_notices = len(clipboard), len(notices)
                panel._copy_button.click()
                settle()
                copies.append(dict(all=False, json=as_json, notes=panel.GetValue()[0], current='empty',
                                   clipboard=clipboard[start_clipboard:], notice=notices[start_notices:]))
            hover = NotePanel(c.gui, 'note2', 'two 🦊', True)
            hover.show()
            hover_events = []
            for text_only in (False, True):
                set_cog(panel, 3, text_only)
                start_clipboard = len(clipboard)
                QT.QTest.mouseClick(hover._note_name, QC.Qt.MouseButton.MiddleButton)
                settle()
                hover_events.append(dict(text_only=text_only, clipboard=clipboard[start_clipboard:]))
            panel.grab().save(OUT.replace('.json', '.png'))
            before_cancel = preferences()
            dialog._cancel.click()
            settle()
            assert not dialog.isVisible(), 'real modal cancellation must close the owner'
            after_cancel = preferences()
            serialized = HydrusSerialisable.CreateFromSerialisableTuple(c.new_options.GetSerialisableTuple())
            reopened_dialog, reopened_panel = make_dialog()
            dialogs.append(reopened_dialog)
            reopened_menu = menu_state(reopened_panel)
            reopened_cursors = cursor_state(reopened_panel)
            reopened_dialog._cancel.click()
            settle()
            assert not reopened_dialog.isVisible(), 'reopened modal owner must cancel' 
            hover.hide()
            hover.deleteLater()
            return dict(keys=KEYS, initial=initial, labels=labels, options=option_events,
                        options_cancelled=options_cancelled, notes=NOTES, menu=initial_menu,
                        cog_openings=cog_openings,
                        initial_cursors=initial_cursors, switched_cursors=switched_cursors,
                        cursors_after_flip=cursors_after_flip, future_cursor=future_cursor,
                        inserted=inserted, future_paste=future_paste, paste_notice=paste_notice,
                        copies=copies, hover=hover_events,
                        before_cancel=before_cancel, after_cancel=after_cancel,
                        final_round_trip=[serialized.GetBoolean(key) for key in KEYS],
                        reopened_menu=reopened_menu, reopened_cursors=reopened_cursors, questions=questions)
        finally:
            for dialog in dialogs:
                dialog.hide()
                dialog.deleteLater()
            c.new_options = original
            c.pub = old_pub
            c.GetClipboardText = old_clipboard
            ClientGUICommon.IconButton.ShowMicroNotification = old_notice
            ClientGUIDialogsQuick.GetYesNo = old_question
    return session.controller.CallBlockingToQt(session.controller.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as stream:
            result = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(result, stream, indent=2, ensure_ascii=False)
        stream.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
