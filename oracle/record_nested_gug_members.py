#!/usr/bin/env python3
"""Record the nested gallery URL generator editor's members list.

Creates the real `EditNGUGPanel` over a set of available generators and
drives its members list the way a user does: the rows it opens with (and
which of them are `available`), the `add` button (the generators it offers,
in the chooser's own order, the ones ticked, what the chooser returns and
the rows after), the warning when none is left, a cancelled chooser, the
`delete` button's question (declined and accepted), and the value the panel
hands back (`RepairGUGs` drops generators that are gone and re-keys ones
found by name).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_nested_gug_members.py
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    controller = session.controller
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusExceptions
    from hydrus.client.gui import ClientGUIDownloaders as G
    from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUIDialogsMessage
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsSelectFromList as SL
    from hydrus.client.networking import ClientNetworkingGUG as NG

    def qt():
        parent = QW.QWidget()

        def gug(name, byte):
            return NG.GalleryURLGenerator(
                name,
                gug_key=bytes([byte]) * 32,
                url_template='https://example.com/search?q=%tags%',
                replacement_phrase='%tags%',
                search_terms_separator='+',
                initial_search_text='search tags',
                example_search_text='blue_eyes',
            )

        alpha = gug('alpha search', 0x21)
        beta = gug('beta search', 0x22)
        gamma_10 = gug('Gamma 10', 0x23)
        gamma_2 = gug('Gamma 2', 0x24)
        delta = gug('delta search', 0x25)
        available = [delta, gamma_10, alpha, gamma_2, beta]
        names = {g.GetGUGKey(): g.GetName() for g in available}

        # alpha by key; 'beta search' under a key that is gone (found by
        # name); one that is gone altogether
        ngug = NG.NestedGalleryURLGenerator(
            'combined search',
            gug_key=b'\x33' * 32,
            initial_search_text='combined tags',
            gug_keys_and_names=[
                (alpha.GetGUGKey(), 'alpha search'),
                (b'\x99' * 32, 'beta search'),
                (b'\x00' * 32, 'missing search'),
            ],
        )
        panel = G.EditNGUGPanel(parent, ngug, available)
        ctrl = panel._gug_list_ctrl

        def rows():
            return [list(panel._ConvertGUGDataToDisplayTuple(d)) for d in ctrl.GetData()]

        def selected():
            chosen = ctrl.GetData(only_selected=True)
            return [d[1] for d in ctrl.GetData() if d in chosen]

        def value():
            v = panel.GetValue()
            return {
                'name': v.GetName(),
                'initial_search_text': v.GetInitialSearchText(),
                'members': [[k.hex(), n] for (k, n) in v.GetGUGKeysAndNames()],
            }

        result = {
            'available': [[g.GetGUGKey().hex(), g.GetName()] for g in available],
            'opening_rows': rows(),
            'opening_value': value(),
        }

        choosers = []
        warnings = []
        script = []

        def chooser(win, title, choice_tuples):
            # the real chooser panel builds the list the user sees
            sel = SL.EditSelectMultiple(parent, choice_tuples)
            offered = [sel._checkboxes.item(i).text() for i in range(sel._checkboxes.count())]
            ticked = list(sel._checkboxes.GetCheckedIndices())
            answer = script.pop(0)
            if answer is None:
                choosers.append({'title': title, 'offered': offered, 'ticked': ticked, 'answer': None})
                raise HydrusExceptions.CancelledException('Dialog cancelled.')
            for label in answer:
                sel._checkboxes.Check(offered.index(label))
            chosen = sel.GetValue()
            choosers.append({
                'title': title,
                'offered': offered,
                'ticked': ticked,
                'answer': answer,
                'returned': [g.GetName() for g in chosen],
            })
            return chosen

        orig_chooser = ClientGUIDialogsQuick.SelectMultipleFromList
        orig_warning = ClientGUIDialogsMessage.ShowWarning
        ClientGUIDialogsQuick.SelectMultipleFromList = chooser
        ClientGUIDialogsMessage.ShowWarning = lambda win, text, *a, **k: warnings.append(text)
        steps = []
        try:
            # a cancelled chooser changes nothing
            script.append(None)
            panel._AddGUGButtonClick()
            steps.append({'step': 'add, cancelled', 'rows': rows(), 'selected': selected(), 'chooser': len(choosers), 'warnings': len(warnings)})
            # two picked (they come back in the chooser's order)
            script.append(['Gamma 2', 'delta search'])
            panel._AddGUGButtonClick()
            steps.append({'step': 'add two', 'rows': rows(), 'selected': selected(), 'chooser': len(choosers), 'warnings': len(warnings)})
            # the rest
            script.append(['Gamma 10'])
            panel._AddGUGButtonClick()
            steps.append({'step': 'add the last', 'rows': rows(), 'selected': selected(), 'chooser': len(choosers), 'warnings': len(warnings)})
            # nothing left: a warning, and no chooser
            panel._AddGUGButtonClick()
            steps.append({'step': 'add with none left', 'rows': rows(), 'selected': selected(), 'chooser': len(choosers), 'warnings': len(warnings)})
        finally:
            ClientGUIDialogsQuick.SelectMultipleFromList = orig_chooser
            ClientGUIDialogsMessage.ShowWarning = orig_warning
        result['choosers'] = choosers
        result['warnings'] = warnings
        result['add_steps'] = steps
        result['after_add_value'] = value()

        # delete: declined, then accepted (a plain 'Remove all selected?')
        questions = []
        orig_yes_no = ClientGUIDialogsQuick.GetYesNo
        delete_steps = []
        try:
            for answer, label in ((QW.QDialog.DialogCode.Rejected, 'declined'), (QW.QDialog.DialogCode.Accepted, 'accepted')):
                def ask(win, message, *a, _answer=answer, **k):
                    questions.append(message)
                    return _answer
                ClientGUIDialogsQuick.GetYesNo = ask
                ctrl.SelectDatas([d for d in ctrl.GetData() if d[1] in ('Gamma 2', 'missing search', 'delta search')], deselect_others=True)
                selected = [d[1] for d in ctrl.GetData() if d in ctrl.GetData(only_selected=True)]
                ctrl.ProcessDeleteAction()
                delete_steps.append({'step': label, 'selected': selected, 'rows': rows()})
        finally:
            ClientGUIDialogsQuick.GetYesNo = orig_yes_no
        result['delete_questions'] = questions
        result['delete_steps'] = delete_steps
        result['final_value'] = value()
        parent.deleteLater()
        return result

    return controller.CallBlockingToQt(controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        return
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(__file__, '--child', out)
        with open(out) as f:
            result = json.load(f)
    target = os.path.join(HERE, 'fixtures', 'nested_gug_members.json')
    with open(target, 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=2)
    print('wrote ' + target)


if __name__ == '__main__':
    main()
