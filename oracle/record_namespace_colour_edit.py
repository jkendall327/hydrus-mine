#!/usr/bin/env python3
"""Record Options > tag presentation > namespace colours' "edit".

Fresh temporary `basic` client; the real `TagPresentationPanel` over the
client's namespace colours. For several selections (none; two namespaces;
the protected unnamespaced and namespaced rows) "edit" (`_EditNamespaceColour`)
is run with the real `EditColour` and Qt's own `QColorDialog`, whose `exec` is
answered by script: a dialog that starts on a given colour is accepted with a
given new colour or rejected. Every dialog's start colour is recorded in the
order the dialogs came (the order of a Python set of the selected terms, so
not stable between runs: compare as a set), with the list afterwards, in the
order it shows, and what `UpdateOptions` writes. A panel left uncommitted
changes nothing.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'namespace_colour_edit.json')


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.client.gui.panels.options import TagPresentationPanel as T
    c = session.controller
    gui = c.gui

    def work():
        panel = T.TagPresentationPanel(gui, c.new_options)
        lb = panel._namespace_colours
        started = []
        script = {}

        def fake_exec(self):
            colour = self.currentColor()
            rgb = [colour.red(), colour.green(), colour.blue()]
            started.append(rgb)
            reply = script.get(tuple(rgb))
            if reply is None:
                return QW.QDialog.DialogCode.Rejected
            self.setCurrentColor(type(colour)(*reply))
            self.accept()
            return QW.QDialog.DialogCode.Accepted
        QW.QColorDialog.exec_ = fake_exec
        QW.QColorDialog.exec = fake_exec

        def listing():
            return [dict(namespace=t.GetNamespace(), colour=list(t.GetNamespaceAndColour()[1]), text=t.GetCopyableTexts(), selected=t in lb._selected_terms) for t in lb._ordered_terms]

        def select(namespaces):
            lb._selected_terms = {t for t in lb._ordered_terms if t.GetNamespace() in namespaces}

        out = dict(initial=listing(), steps=[])

        def step(label, namespaces, replies):
            select(namespaces)
            script.clear()
            script.update({tuple(k): v for k, v in replies})
            started.clear()
            before = {t.GetNamespace(): list(t.GetNamespaceAndColour()[1]) for t in lb._ordered_terms}
            panel._EditNamespaceColour()
            out['steps'].append(dict(label=label, selected=sorted(namespaces, key=str), replies=[[list(k), v] for k, v in replies], pickers_started_on=sorted(started), before=before, after=listing()))

        step('nothing selected', [], [])
        step('character accepted, series cancelled', ['character', 'series'], [((0, 170, 0), (10, 20, 30))])
        step('unnamespaced (protected) accepted', [''], [((0, 111, 250), (255, 0, 0))])
        step('namespaced (protected) cancelled', [None], [])
        before = dict(HC.options['namespace_colours'])
        out['options_before_commit'] = {str(k): list(v) for k, v in before.items()}
        panel.UpdateOptions()
        after = dict(HC.options['namespace_colours'])
        out['options_after_commit'] = {str(k): list(v) for k, v in after.items()}
        return out
    return c.CallBlockingToQt(gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
