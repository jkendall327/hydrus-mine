#!/usr/bin/env python3
"""Record actual live EnterCatchingRadioButton and parent default-dialog handling.

Real filesize/hash/deletion radio lists receive QTest Enter/Return with the saved
preference false/true and changed after opening. GUIPanel staging, cancellation,
UpdateOptions and reopening are recorded. Key acceptance is observed after the
unchanged reference handler, separately from actual parent dialog acceptance.
"""
import json
import sqlite3
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
KEY = 'force_enter_on_radio_buttons_to_do_dialog_ok'


def record(session):
    def drive():
        from qtpy import QtCore as C, QtGui as G, QtTest as T, QtWidgets as W
        from hydrus.client import ClientOptions
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as D
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsEdit as E
        from hydrus.client.gui.panels.options import GUIPanel as O
        from hydrus.client.gui.search import ClientGUIPredicatesSingle as S, ClientGUISearch as Search
        from hydrus.client.gui.widgets import ClientGUICommon as Common
        from hydrus.client.media import ClientMediaSingle as M
        from hydrus.client.search import ClientSearchPredicate as P
        c = session.controller
        saved = c.new_options.GetBoolean(KEY)
        original = Common.EnterCatchingRadioButton
        events = []
        class Observed(original):
            def keyPressEvent(self, event):
                super().keyPressEvent(event)
                if event.key() in (C.Qt.Key.Key_Enter, C.Qt.Key.Key_Return):
                    events.append(dict(key='Enter' if event.key() == C.Qt.Key.Key_Enter else 'Return',
                                       force=c.new_options.GetBoolean(KEY), accepted=event.isAccepted()))
        out = dict(default=ClientOptions.ClientOptions().GetBoolean(KEY), loaded=saved,
                   label='Force that hitting Enter/Return on radio button lists triggers a dialog ok: ',
                   options=[], cases=[])
        Common.EnterCatchingRadioButton = Observed
        def panel():
            owner = W.QWidget(c.gui)
            p = O.GUIPanel(owner)
            return owner, p
        try:
            owner, p = panel()
            out['options'].append(dict(case='opened', checked=p._force_enter_on_radio_buttons_to_do_dialog_ok.isChecked(), saved=c.new_options.GetBoolean(KEY)))
            p._force_enter_on_radio_buttons_to_do_dialog_ok.setChecked(not saved)
            out['options'].append(dict(case='edited', checked=p._force_enter_on_radio_buttons_to_do_dialog_ok.isChecked(), saved=c.new_options.GetBoolean(KEY)))
            owner.close(); owner.deleteLater()
            owner, p = panel()
            out['options'].append(dict(case='cancel_reopened', checked=p._force_enter_on_radio_buttons_to_do_dialog_ok.isChecked(), saved=c.new_options.GetBoolean(KEY)))
            p._force_enter_on_radio_buttons_to_do_dialog_ok.setChecked(not saved)
            p.UpdateOptions()
            owner.close(); owner.deleteLater()
            owner, p = panel()
            out['options'].append(dict(case='applied_reopened', checked=p._force_enter_on_radio_buttons_to_do_dialog_ok.isChecked(), saved=c.new_options.GetBoolean(KEY)))
            owner.close(); owner.deleteLater()
            out['legacy'] = c.new_options.GetSerialisableTuple()
            results = c.Read('media_results_from_ids', list(range(1, 41)))
            for kind in ['filesize', 'hash_sign', 'hash_type', 'delete_action', 'delete_reason']:
                for initial, live in [(False, False), (True, True), (True, False), (False, True)]:
                    for keyname, key in [('Return', C.Qt.Key.Key_Return), ('Enter', C.Qt.Key.Key_Enter)]:
                        c.new_options.SetBoolean(KEY, initial)
                        dialog = D.DialogEdit(c.gui, 'input predicate' if not kind.startswith('delete') else 'Delete files?')
                        if kind.startswith('delete'):
                            p = E.EditDeleteFilesPanel(dialog, [M.MediaSingle(results[0])], 'Synthetic radio-key probe')
                            group = p._action_radio if kind == 'delete_action' else p._reason_radio
                        else:
                            blank = P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIZE if kind == 'filesize' else P.PREDICATE_TYPE_SYSTEM_HASH)
                            p = Search.FleshOutPredicatePanel(dialog, blank)
                            leaf = p.findChild(S.PanelPredicateSystemSize if kind == 'filesize' else S.PanelPredicateSystemHash)
                            if kind.startswith('hash'):
                                leaf._hashes.setPlainText('ab' * 32)
                            group = leaf._hash_type if kind == 'hash_type' else leaf._sign
                        dialog.SetPanel(p)
                        button = next((b for b in group._radio_buttons if b.isEnabled() and b.isChecked()), group._radio_buttons[0])
                        before = group.GetValue()
                        observation = {}
                        timeout = C.QTimer(dialog)
                        timeout.setSingleShot(True)
                        timeout.timeout.connect(dialog.reject)
                        deliver = C.QTimer(dialog)
                        deliver.setSingleShot(True)
                        def press():
                            button.setFocus(); W.QApplication.processEvents()
                            c.new_options.SetBoolean(KEY, live)
                            events.clear()
                            observation.update(default=dialog._apply.isDefault(), auto_default=dialog._apply.autoDefault(), focus=button.hasFocus())
                            T.QTest.keyClick(button, key)
                        deliver.timeout.connect(press)
                        timeout.start(100)
                        deliver.start(10)
                        result = dialog.exec()
                        timeout.stop(); deliver.stop()
                        out['cases'].append(dict(kind=kind, key=keyname, at_open=initial, saved_at_key=live,
                                                 handler=list(events), dialog_result=int(result),
                                                 visible=dialog.isVisible(), focus=observation.get('focus'),
                                                 default=observation.get('default'), auto_default=observation.get('auto_default'),
                                                 selection_changed=group.GetValue() != before,
                                                 enabled=button.isEnabled()))
                        if kind == 'filesize' and initial is False and live is True and keyname == 'Return':
                            dialog.show(); dialog.grab().save(str(HERE / 'fixtures/radio_return.png'))
                        dialog.reject(); dialog.deleteLater(); W.QApplication.processEvents()
        finally:
            Common.EnterCatchingRadioButton = original
            c.new_options.SetBoolean(KEY, saved)
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, drive)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        database = record_api.unpack_fixture('basic')
        with sqlite3.connect(str(Path(database) / 'client.db')) as connection:
            raw = json.loads(connection.execute('SELECT dictionary_string FROM services WHERE service_type=18').fetchone()[0])
            for key, value in raw[2]:
                if key == [0, 'port']: value[1] = None
            connection.execute('UPDATE services SET dictionary_string=? WHERE service_type=18', (json.dumps(raw),))
        connection.close()
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(database, record)))
        return
    with tempfile.TemporaryDirectory() as folder:
        result = Path(folder) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(result))
        value = json.loads(result.read_text())
    (HERE / 'fixtures/radio_return.json').write_text(json.dumps(value, indent=2) + '\n')
    print('wrote radio_return.json')

if __name__ == '__main__': main()
