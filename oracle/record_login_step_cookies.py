#!/usr/bin/env python3
"""Record the actual Qt embedded login-step cookie list and direct actions.

Runs original Add/Edit and confirmed list deletion, scripting only matcher
dialog answers and confirmation. Records original/default matcher values,
sorted rows, extended selection, step serialization, control ancestry and
buttons. Independent duplicate-looking cookie names are preserved. No HTTP.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'login_step_cookies.json')


def record(session):
    def run():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui.networking import ClientGUILogin as G
        from hydrus.client.networking import ClientNetworkingLogin as L
        from hydrus.core import HydrusSerialisable
        name = S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='session', example_string='session')
        value = S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='ok', example_string='ok')
        original = L.LoginStep('cookie request', 'http', 'GET', None, '/start')
        original.SetComplicatedVariables({}, {}, {}, {name: value}, [])
        original_value = original.GetSerialisableTuple()
        panel = G.EditLoginStepPanel(session.controller.gui, original)
        control = panel._required_cookies_info
        listing = control._listctrl
        model = listing.model()
        answers = []
        dialogs = []
        initial = []
        questions = []
        answer = True
        class Dialog(QW.QWidget):
            def __init__(self, parent, title, *args, **kwargs):
                super().__init__(session.controller.gui)
                dialogs.append(title)
            def __enter__(self): return self
            def __exit__(self, *args): return False
            def SetPanel(self, child):
                self.panel = child
                initial.append(child.GetValue().GetSerialisableTuple())
            def exec(self):
                text = answers.pop(0)
                if text is None: return QW.QDialog.DialogCode.Rejected
                self.panel._match_type.SetValue(S.STRING_MATCH_FIXED)
                self.panel._match_value_fixed_input.setText(text)
                return QW.QDialog.DialogCode.Accepted
        def yes_no(win, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected
        def rows():
            return [[model.data(model.index(row, col)) for col in range(model.columnCount())]
                    for row in range(model.rowCount())]
        def state():
            return {'rows': rows(), 'value': panel.GetValue().GetSerialisableTuple(),
                'selected': [listing._ConvertDataToDisplayTuple(item) if hasattr(listing, '_ConvertDataToDisplayTuple')
                             else control._ConvertDataToDisplayTuple(item) for item in listing.GetData(only_selected=True)]}
        ancestry = []
        box_title = None
        parent = control.parentWidget()
        while parent is not None and parent is not panel:
            ancestry.append(type(parent).__name__)
            if hasattr(parent, '_title_st'):
                box_title = parent._title_st.text()
            parent = parent.parentWidget()
        topology = {'embedded': parent is panel, 'ancestry': ancestry,
            'box': box_title,
            'columns': [model.headerData(col, QC.Qt.Orientation.Horizontal, QC.Qt.ItemDataRole.DisplayRole) for col in range(model.columnCount())],
            'buttons': [b.text() for b in control.findChildren(QW.QPushButton)]}
        real_dialog = G.ClientGUITopLevelWindowsPanels.DialogEdit
        real_yes_no = ClientGUIDialogsQuick.GetYesNo
        G.ClientGUITopLevelWindowsPanels.DialogEdit = Dialog
        ClientGUIDialogsQuick.GetYesNo = yes_no
        states = [{'state': state()}]
        try:
            for action, values in [('add', ['token', 'ready']), ('edit', ['edited', 'changed']),
                    ('add', ['discarded', None]), ('add', ['session', 'ok']), ('add', [None]),
                    ('edit', ['discarded edit', None])]:
                if action == 'edit': listing.SelectDatas([listing.GetData()[-1]], deselect_others=True)
                answers[:] = values
                dialogs.clear(); initial.clear()
                if action == 'add': control._Add()
                else: control._Edit()
                assert not answers
                states.append({'action': action, 'answers': values, 'dialogs': list(dialogs),
                    'initial': list(initial), 'state': state()})
            listing.SelectDatas(listing.GetData()[:2], deselect_others=True)
            for answer in [False, True]:
                questions.clear()
                listing.ShowDeleteSelectedDialog()
                states.append({'action': 'delete', 'answer': answer,
                    'questions': list(questions), 'state': state()})
            assert original.GetSerialisableTuple() == original_value
            return {'topology': topology, 'states': states, 'original': original.GetSerialisableTuple()}
        finally:
            G.ClientGUITopLevelWindowsPanels.DialogEdit = real_dialog
            ClientGUIDialogsQuick.GetYesNo = real_yes_no
            panel.deleteLater()
    return session.controller.CallBlockingToQt(session.controller.gui, run)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f: result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(f'wrote {OUT}')
if __name__ == '__main__': main()
