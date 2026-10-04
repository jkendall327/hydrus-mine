#!/usr/bin/env python3
"""Record real RegexInput favourites menu copies and accepted/cancelled owned dialogs."""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def qt():
        from qtpy import QtCore as QC
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui import ClientGUIStringPanels as P, ClientGUICore as G, ClientGUITopLevelWindowsPanels as W
        old = list(HC.options['regex_favourites']); initial = [('a+', 'letters'), ('[', 'fragment')]
        old_popup = G.core().PopupMenu; old_pub = c.pub; old_dialog = W.DialogEdit
        out = {'initial': initial, 'menus': [], 'copied': [], 'steps': []}
        requested = ['copy']; accepted = [False]
        def pub(*args, **kwargs):
            if args and args[0] == 'clipboard': out['copied'].append(args[2]); return
            return old_pub(*args, **kwargs)
        c.pub = pub; HC.options['regex_favourites'] = list(initial)
        panel = P.EditStringMatchPanel(c.gui, S.StringMatch(match_type=S.STRING_MATCH_REGEX, match_value='original+', example_string='original'))
        control = panel._match_value_regex_input
        class Dialog(old_dialog):
            def exec(self):
                def finish():
                    self._panel._regexes.AddData(('z+', 'new choice'), select_sort_and_scroll=True)
                    out['dialog_title'] = self.windowTitle()
                    if accepted[0]: self.DoOK()
                    else: self._cancel.click()
                QC.QTimer.singleShot(0, finish)
                return super().exec()
        W.DialogEdit = Dialog
        def popup(widget, menu):
            favourites = next(a.menu() for a in menu.actions() if a.text() == 'favourites')
            out['menus'].append([{'separator': True} if a.isSeparator() else {'label': a.text(), 'enabled': a.isEnabled()} for a in favourites.actions()])
            if requested[0] == 'copy':
                for action in favourites.actions():
                    if action.isEnabled() and not action.isSeparator() and action.text() != 'manage favourites': action.trigger()
            elif requested[0] == 'manage': next(a for a in favourites.actions() if a.text() == 'manage favourites').trigger()
        G.core().PopupMenu = popup
        try:
            control._regex_button.click()
            out['unchanged_input'] = control.GetValue()
            requested[0] = 'manage'
            for value in (False, True):
                accepted[0] = value; control._regex_button.click()
                out['steps'].append({'accepted': value, 'saved': list(HC.options['regex_favourites']), 'input': control.GetValue()})
            requested[0] = 'copy'; control._regex_button.click()
            out['final_input'] = control.GetValue()
        finally:
            panel.deleteLater(); HC.options['regex_favourites'] = old; c.pub = old_pub; G.core().PopupMenu = old_popup; W.DialogEdit = old_dialog
        return out
    return c.CallBlockingToQt(c.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        value = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f: json.dump(value, f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: value = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'matcher_favourites.json'), 'w') as f:
        json.dump(value, f, indent=1); f.write('\n')
    print('recorded actual matcher favourites popup and real owned dialog accept/cancel')


if __name__ == '__main__': main()
