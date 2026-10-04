#!/usr/bin/env python3
"""Drive real Qt tag-filter favourites: menus, save/import collisions, cancel,
load/delete, serialized clipboard payload, cleaned rules, immediate settings,
actual bulk-paste buttons and advanced blacklist extra-panel visibility.
Run through the shared with-oracle helper on the basic reference fixture.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'tag_filter_favourites.json')

def record(session):
    from qtpy import QtWidgets as W
    from hydrus.client import ClientGlobals as G
    from hydrus.client.gui import ClientGUIAsync as A
    from hydrus.client.gui import ClientGUICore as C
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.client.gui import ClientGUIDialogsMessage as M
    from hydrus.client.gui.metadata import ClientGUITagFilter as T
    from hydrus.core import HydrusTags as H, HydrusConstants as HC, HydrusExceptions as E
    controller = session.controller
    def qt():
        class Now:
            def __init__(self, parent, work_callable, publish_callable, **kwargs):
                self.work, self.publish = work_callable, publish_callable
            def start(self): self.publish(self.work())
        A.AsyncQtJob = Now
        opts = controller.new_options
        opts.SetFavouriteTagFilters({})
        current = H.TagFilter()
        current.SetRule('goblin', HC.FILTER_BLACKLIST)
        panel = T.EditTagFilterPanel(controller.gui, current, namespaces=[])
        events, menus, questions, errors = [], [], [], []
        answers, names = [], []
        clipboard = [current.DumpToString()]
        C.core().PopupMenu = lambda parent, menu: menus.append(menu)
        def enter(parent, message, **kwargs):
            questions.append(message)
            name = names.pop(0)
            if name is None: raise E.CancelledException()
            return name
        Q.EnterText = enter
        Q.GetYesNo = lambda parent, message, **kw: (questions.append(message), W.QDialog.DialogCode.Accepted if answers.pop(0) else W.QDialog.DialogCode.Rejected)[1]
        M.ShowCritical = lambda parent, title, message: errors.append([title, message])
        Q.PresentClipboardParseError = lambda parent, text, expected, error: errors.append(['parse', expected])
        controller.GetClipboardText = lambda: clipboard[0]
        def rules(f): return sorted([list(r) for r in f.GetTagSlicesToRules().items()])
        def snapshot(action):
            events.append({'action': action, 'questions': questions.copy(), 'errors': errors.copy(), 'draft': rules(panel.GetValue()), 'favourites': {n: rules(f) for n, f in opts.GetFavouriteTagFilters().items()}})
            questions.clear(); errors.clear()
        def menu(method, choose=None):
            method()
            m = menus.pop()
            labels = [a.text() for a in m.actions() if not a.isSeparator()]
            if choose is not None:
                next(a for a in m.actions() if a.text() == choose).trigger()
            return labels
        empty = menu(panel._LoadFavourite)
        names.append(None); panel._SaveFavourite(); snapshot('save_cancel')
        names.append('zebra'); panel._SaveFavourite(); snapshot('save')
        replacement = H.TagFilter(); replacement.SetRule(':', HC.FILTER_BLACKLIST)
        panel.SetValue(replacement)
        names.append('zebra'); answers.append(False); panel._SaveFavourite(); snapshot('save_no')
        names.append('zebra'); answers.append(True); panel._SaveFavourite(); snapshot('save_yes')
        panel.SetValue(current)
        load_menu = menu(panel._LoadFavourite, 'zebra'); snapshot('load')
        dirty = H.TagFilter(); dirty.SetRule('  CREATOR:  ', HC.FILTER_BLACKLIST); dirty.SetRule('  BLUE EYES ', HC.FILTER_WHITELIST)
        clipboard[0] = dirty.DumpToString()
        names.append(None); panel._ImportFavourite(); snapshot('import_cancel')
        names.append('zebra'); answers.append(False); panel._ImportFavourite(); snapshot('import_no')
        names.append('apple'); panel._ImportFavourite(); snapshot('import')
        names.append('zebra'); answers.append(True); panel._ImportFavourite(); snapshot('import_yes')
        export_menu = menu(panel._ExportFavourite)
        payload = panel.GetValue().DumpToString()
        clipboard[0] = 'not json'; panel._ImportFavourite(); snapshot('import_invalid')
        clipboard[0] = '[26, 3, []]'; panel._ImportFavourite(); snapshot('import_wrong_type')
        answers.append(False); delete_menu = menu(panel._DeleteFavourite, 'apple'); snapshot('delete_no')
        answers.append(True); menu(panel._DeleteFavourite, 'apple'); snapshot('delete_yes')
        panel.deleteLater()
        reopened = T.EditTagFilterPanel(controller.gui, current, namespaces=[])
        reopen_menu = menu(reopened._LoadFavourite)
        reopened.deleteLater()
        # Actual TextAndPasteCtrl buttons, including empty/BOM lines.
        pasted = []
        for attr in ['_simple_whitelist_input', '_simple_blacklist_input', '_advanced_blacklist_input', '_advanced_whitelist_input']:
            paste_panel = T.EditTagFilterPanel(controller.gui, replacement, namespaces=[])
            raw = '\ufeff  ORC  \r\n\nSeries:*\norc\n*:*\n'
            clipboard[0] = raw
            getattr(paste_panel, attr)._Paste()
            pasted.append({'input': attr, 'text': raw, 'rules': rules(paste_panel.GetValue())})
            paste_panel.deleteLater()
        extra_panels = []
        for advanced in [False, True]:
            opts.SetBoolean('advanced_mode', advanced)
            extra = T.EditTagFilterPanel(controller.gui, current, only_show_blacklist=True, namespaces=[])
            before = {'tabs': [extra._notebook.tabText(i) for i in range(extra._notebook.count())], 'offered': not extra._show_all_panels_button.isHidden()}
            if advanced: extra._ShowAllPanels()
            after = {'tabs': [extra._notebook.tabText(i) for i in range(extra._notebook.count())], 'offered': not extra._show_all_panels_button.isHidden()}
            extra._test_input.setPlainText('creator:goblin')
            extra_panels.append({'advanced': advanced, 'before': before, 'after': after, 'test': extra._test_result_st.text(), 'rules': rules(extra.GetValue())})
            extra.deleteLater()
        return {'empty_menu': empty, 'load_menu': load_menu, 'delete_menu': delete_menu, 'export_menu': export_menu, 'reopen_menu': reopen_menu, 'payload': payload, 'dirty_payload': dirty.DumpToString(), 'events': events, 'paste': pasted, 'extra_panels': extra_panels}
    return controller.CallBlockingToQt(controller.gui, qt)

def child(out):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f: json.dump(result, f)

def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2]); return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', p)
        with open(p) as f: result = json.load(f)
    with open(OUT, 'w') as f: json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()
