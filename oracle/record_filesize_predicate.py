#!/usr/bin/env python3
"""Drive the real filesize radio/BytesControl and query its typed predicates.

Record all five operators and binary units, raw numeric bounds, explicit-value
reopening (without unit renormalisation), real arrow/Space keys and a cancelled
EditPredicatesPanel. Queries use the unchanged synthetic basic-fixture files.
The TB ToString defect is recorded separately from the valid typed value/query.
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
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
        from hydrus.client.gui.search import ClientGUIPredicatesSingle as S, ClientGUISearch as G
        from hydrus.client.search import ClientSearchPredicate as P, ClientSearchFileSearchContext as F

        c = session.controller
        blank = P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIZE)
        location = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        units = [('B', 1), ('KB', 1024), ('MB', 1024**2), ('GB', 1024**3), ('TB', 1024**4)]
        operators = ['<', HC.UNICODE_APPROX_EQUAL, '=', HC.UNICODE_NOT_EQUAL, '>']
        dialog = W.DialogEdit(c.gui, 'input predicate')
        panel = G.FleshOutPredicatePanel(dialog, blank)
        dialog.SetPanel(panel)
        dialog.resize(950, 400)
        dialog.show()
        QW.QApplication.processEvents()
        size = panel.findChild(S.PanelPredicateSystemSize)

        def snapshot(p, query=True):
            predicate = p.GetPredicates()[0]
            value = predicate.GetValue()
            context = F.FileSearchContext(location_context=location, predicates=[predicate])
            out = {'value': value, 'text': predicate.ToString(),
                   'serialised': predicate.GetSerialisableTuple(),
                   'checked': [b.isChecked() for b in p._sign._radio_buttons],
                   'amount': p._bytes._spin.value(), 'unit': p._bytes._unit.currentText()}
            if query:
                ids = c.Read('file_query_ids', context)
                out['hashes'] = sorted(m.GetHash().hex() for m in c.Read('media_results_from_ids', ids))
            return out

        out = {'default': snapshot(size), 'operators': operators, 'units': units,
               'minimum': size._bytes._spin.minimum(), 'maximum': size._bytes._spin.maximum(),
               'cases': [], 'boundary_queries': [], 'bounds': [], 'reopened': [], 'keys': []}
        dialog.grab().save(str(HERE / 'fixtures/filesize-predicate-qt.png'))
        for sign in operators:
            for name, multiplier in units:
                size._sign.SetValue(sign)
                size._bytes.SetSeparatedValue(17, multiplier)
                out['cases'].append(snapshot(size))
        known_size = c.Read('media_results_from_ids', [1])[0].GetSize()
        for sign in operators:
            size._sign.SetValue(sign)
            size._bytes.SetSeparatedValue(known_size, 1)
            out['boundary_queries'].append(snapshot(size))
        size._sign.SetValue('>')
        for value in [-1, 0, 1048576, 1048577]:
            size._bytes.SetSeparatedValue(value, 1024)
            out['bounds'].append({'input': value, 'result': snapshot(size)})
        for sign, amount, multiplier in [('=', 1024, 1), ('>', 0, 1024), ('≠', 7, 1024**4), ('<', 1048577, 1024**2)]:
            initial = P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIZE, (sign, amount, multiplier))
            reopened = S.PanelPredicateSystemSize(c.gui, initial)
            out['reopened'].append({'input': initial.GetValue(), 'result': snapshot(reopened)})
            reopened.deleteLater()
        size._sign.SetValue('<')
        size._sign._radio_buttons[0].setFocus()
        for name, key in [('left-edge', QC.Qt.Key.Key_Left), ('right', QC.Qt.Key.Key_Right),
                          ('right', QC.Qt.Key.Key_Right), ('space', QC.Qt.Key.Key_Space),
                          ('right', QC.Qt.Key.Key_Right), ('right', QC.Qt.Key.Key_Right),
                          ('right-edge', QC.Qt.Key.Key_Right), ('left', QC.Qt.Key.Key_Left)]:
            QT.QTest.keyClick(QW.QApplication.focusWidget(), key)
            out['keys'].append({'action': name, 'value': size._sign.GetValue(),
                                'focused': next(i for i, b in enumerate(size._sign._radio_buttons) if b.hasFocus())})
        original = P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIZE, ('=', 1024, 1))
        cancel_dialog = W.DialogEdit(c.gui, 'edit predicates')
        cancelled = G.EditPredicatesPanel(cancel_dialog, [original])
        cancel_dialog.SetPanel(cancelled)
        cancelled.findChild(S.PanelPredicateSystemSize)._bytes.SetSeparatedValue(3, 1024**3)
        cancel_dialog.reject()
        out['cancel'] = {'original': original.GetValue(), 'result': int(cancel_dialog.result())}
        cancel_dialog.deleteLater()
        dialog.reject()
        dialog.deleteLater()
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
    (HERE / 'fixtures/filesize_predicate.json').write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')
    print('wrote filesize_predicate.json')


if __name__ == '__main__':
    main()
