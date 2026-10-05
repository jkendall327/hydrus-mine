#!/usr/bin/env python3
"""Real hash predicate widgets, cleanup warnings/questions and typed queries.

The real multiline editor, radio groups and cleanup buttons run unchanged.
Warning/yes-no presentation is scripted for exact cleanup paths; a final real
QMessageBox warning is captured and acknowledged. Typed predicates execute real
basic-file queries, with explicit reconstruction and parent Cancel also recorded.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def work():
        from qtpy import QtCore as QC, QtTest as QT, QtWidgets as QW
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q, ClientGUITopLevelWindowsPanels as W
        from hydrus.client.gui.search import ClientGUIPredicatesSingle as S, ClientGUISearch as G
        from hydrus.client.search import ClientSearchPredicate as P, ClientSearchFileSearchContext as F

        c = session.controller
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        types = ['sha256', 'md5', 'sha1', 'sha512']
        known = [m.GetHash() for m in c.Read('media_results_from_ids', [1, 2])]
        hashes = {kind: [h.hex() for h in known] if kind == 'sha256' else
                  [c.Read('file_hashes', (h,), 'sha256', kind)[h].hex() for h in known]
                  for kind in types}
        blank = P.Predicate(P.PREDICATE_TYPE_SYSTEM_HASH)
        dialog = W.DialogEdit(c.gui, 'input predicate')
        panel = G.FleshOutPredicatePanel(dialog, blank)
        dialog.SetPanel(panel)
        dialog.resize(1000, 550)
        dialog.show()
        QW.QApplication.processEvents()
        editor = panel.findChild(S.PanelPredicateSystemHash)
        warnings = []
        questions = []
        answer = [True]
        old_warning, old_question = M.ShowWarning, Q.GetYesNo
        M.ShowWarning = lambda owner, message: warnings.append(message)
        def yes_no(owner, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted if answer[0] else QW.QDialog.DialogCode.Rejected
        Q.GetYesNo = yes_no
        def snapshot(widget, query=False):
            value = {'text': widget._hashes.toPlainText(), 'type': widget._hash_type.GetValue(),
                     'inclusive': widget._sign.GetValue(),
                     'type_checked': [b.isChecked() for b in widget._hash_type._radio_buttons]}
            try:
                p = widget.GetPredicates()[0]
                raw, kind = p.GetValue()
                value['predicate'] = {'hashes': [h.hex() for h in raw], 'type': kind, 'inclusive': p.IsInclusive(), 'label': p.ToString()}
                value['serialised'] = p.GetSerialisableTuple()
                if query:
                    ids = c.Read('file_query_ids', F.FileSearchContext(location_context=location, predicates=[p]))
                    value['query_hashes'] = sorted(m.GetHash().hex() for m in c.Read('media_results_from_ids', ids))
            except Exception as error:
                value['error'] = str(error)
            return value
        out = {'types': types, 'known': hashes, 'default': snapshot(editor), 'queries': [], 'cleanup': [], 'reopened': [], 'keys': []}
        dialog.grab().save(str(HERE / 'fixtures/hash-predicate-qt.png'))
        try:
            for kind in types:
                for inclusive in [True, False]:
                    editor._hash_type.SetValue(kind)
                    editor._sign.SetValue(inclusive)
                    editor._hashes.setPlainText(f'  {kind.upper()}:{hashes[kind][0].upper()}  \r\n0x{hashes[kind][1]}\n{hashes[kind][0]}\n')
                    out['queries'].append(snapshot(editor, True))
            for inclusive in [True, False]:
                editor._hash_type.SetValue('sha256'); editor._sign.SetValue(inclusive); editor._hashes.clear()
                out['queries'].append(snapshot(editor, True))
            cases = []
            for kind in types:
                cases.append((kind + '-guess', f'{kind.upper()}:0x{hashes[kind][0].upper()}\n{hashes[kind][0]}', 'normal', True))
            cases.extend([
                ('bad-lines-normal', f'sha256:{hashes["sha256"][0]}\nwat:{hashes["sha256"][0]}\nnot hex\na\nabcd', 'normal', True),
                ('mixed-lengths-normal', hashes['md5'][0] + '\n' + hashes['sha256'][0], 'normal', True),
                ('empty-normal', '', 'normal', True),
                ('forced-cancel', 'not hex\n' + hashes['md5'][0], 'forced', False),
                ('forced-accept', 'not hex\n' + hashes['md5'][0], 'forced', True),
                ('all-bad-clear', 'not hex\nwat:bad', 'forced', True),
                ('mixed-lengths-forced', hashes['md5'][0] + '\n' + hashes['sha256'][0], 'forced', True),
                ('prefix-length-conflict', 'md5:' + hashes['sha256'][0], 'normal', True),
            ])
            for name, text, action, yes in cases:
                editor._hash_type.SetValue('sha512'); editor._sign.SetValue(True); editor._hashes.setPlainText(text)
                warnings.clear(); questions.clear(); answer[0] = yes
                before = snapshot(editor)
                (editor._format_hashes_button if action == 'normal' else editor._format_hashes_button_forced).click()
                out['cleanup'].append({'name': name, 'action': action, 'yes': yes, 'before': before,
                                       'after': snapshot(editor), 'warnings': list(warnings), 'questions': list(questions)})
            for kind in types:
                initial = P.Predicate(P.PREDICATE_TYPE_SYSTEM_HASH, (tuple(bytes.fromhex(h) for h in sorted(hashes[kind])), kind), inclusive=False)
                reopened = S.PanelPredicateSystemHash(c.gui, initial)
                out['reopened'].append(snapshot(reopened, True)); reopened.deleteLater()
            editor._hash_type.SetValue('sha256'); editor._hash_type._radio_buttons[0].setFocus()
            for name, key in [('up-edge', QC.Qt.Key.Key_Up), ('down', QC.Qt.Key.Key_Down), ('down', QC.Qt.Key.Key_Down),
                              ('space', QC.Qt.Key.Key_Space), ('down', QC.Qt.Key.Key_Down), ('down-edge', QC.Qt.Key.Key_Down), ('up', QC.Qt.Key.Key_Up)]:
                QT.QTest.keyClick(QW.QApplication.focusWidget(), key)
                out['keys'].append({'action': name, 'type': editor._hash_type.GetValue()})
            # Drive the real QMessageBox transport once, then acknowledge it.
            message = next(case['warnings'][0] for case in out['cleanup'] if case['warnings'])
            def capture_warning():
                box = next(w for w in QW.QApplication.topLevelWidgets() if isinstance(w, QW.QMessageBox) and w.isVisible())
                out['warning_transport'] = {'title': box.windowTitle(), 'message': box.text(),
                                            'buttons': [b.text() for b in box.buttons()], 'modal': box.isModal()}
                box.grab().save(str(HERE / 'fixtures/hash-predicate-warning-qt.png'))
                box.accept()
            QC.QTimer.singleShot(0, capture_warning)
            old_warning(editor, message)
            editor._hash_type.SetValue('md5'); editor._sign.SetValue(False)
            editor._hashes.setPlainText(hashes['md5'][0])
            out['cancel'] = {'draft': snapshot(editor), 'original_value': blank.GetValue()}
            dialog.reject()
            out['cancel']['result'] = int(dialog.result())
            out['cancel']['original_after'] = blank.GetValue()
        finally:
            M.ShowWarning, Q.GetYesNo = old_warning, old_question
            dialog.reject(); dialog.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as folder:
        result = Path(folder) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(result))
        value = json.loads(result.read_text())
    (HERE / 'fixtures/hash_predicate.json').write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')
    print('wrote hash_predicate.json')


if __name__ == '__main__':
    main()
