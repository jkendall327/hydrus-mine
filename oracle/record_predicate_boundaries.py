#!/usr/bin/env python3
"""Record real MIME inherited check states and forced hash cleanup answers."""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from qtpy import QtCore as C, QtWidgets as W
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from hydrus.client.gui.search import ClientGUIPredicatesSingle as G
        from hydrus.client.search import ClientSearchPredicate as P

        mime = G.PanelPredicateSystemMime(session.controller.gui,
            P.Predicate(P.PREDICATE_TYPE_SYSTEM_MIME, ()))
        group = mime._mimes._my_tree.topLevelItem(0)
        out = {'mime': [], 'hash': []}

        def snapshot(label):
            out['mime'].append({'action': label, 'state': group.checkState(0).value,
                'children': [group.child(i).checkState(0).value for i in range(group.childCount())]})

        snapshot('empty')
        group.child(1).setCheckState(0, C.Qt.CheckState.Checked)
        snapshot('one child')
        group.setCheckState(0, C.Qt.CheckState.Checked)
        snapshot('whole group')
        group.child(0).setCheckState(0, C.Qt.CheckState.Unchecked)
        snapshot('remove one child')
        group.setCheckState(0, C.Qt.CheckState.Unchecked)
        snapshot('clear group')

        hashes = G.PanelPredicateSystemHash(session.controller.gui,
            P.Predicate(P.PREDICATE_TYPE_SYSTEM_HASH, ((), 'sha256')))
        hashes._hashes.setPlainText('d41d8cd98f00b204e9800998ecf8427e\nnot a hash')
        original_question = Q.GetYesNo
        try:
            for accepted in [False, True]:
                questions = []

                def answer(parent, question, **kwargs):
                    questions.append(question)
                    return W.QDialog.DialogCode.Accepted if accepted else W.QDialog.DialogCode.Rejected

                Q.GetYesNo = answer
                hashes._DoParseAndCleanUpTextForced()
                out['hash'].append({'accepted': accepted, 'questions': questions,
                    'text': hashes._hashes.toPlainText(), 'type': hashes._hash_type.GetValue()})
        finally:
            Q.GetYesNo = original_question
            hashes.deleteLater()
            mime.deleteLater()
        return out

    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output_path = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output_path, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'boundaries.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'predicate_boundaries.json'), 'w') as f:
        json.dump(result, f, indent=2)
        f.write('\n')
    print('Recorded MIME mixed states and hash cleanup confirmation')


if __name__ == '__main__':
    main()
