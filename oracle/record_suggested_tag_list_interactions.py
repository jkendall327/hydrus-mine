#!/usr/bin/env python3
"""Record the suggested-tag lists' own mouse, keyboard and right-click menus.

Fresh temporary basic client. 'my tags' gets a parent (alpha:first has
group:alpha) and a sibling (old:sibling shows as new:sibling), synced. The real
SuggestedTagsPanel (columns layout, most used, related and recent on) is built
over two files. Its most-used list (a ListBoxTagsSuggestionsFavourites, a
ListBoxTagsStrings) is driven with real Qt mouse clicks and key presses on the
list widget; after each step the selected tags are recorded. The right-click
menu (shown as the list's customContextMenuRequested would) is recorded as a
tree, every copy entry is triggered (clipboard payloads captured), the open
entries are triggered (new search pages captured, not opened), and favourites
entries are triggered with yes-answers. The rows as drawn (with sibling and
parent decorators) are recorded. The recent and related lists' menus for one
selected tag are recorded too, to show they are the same kind of list.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'suggested_tag_list_interactions.json')

MOST_USED = ['alpha:first', 'series:spaced name', 'plain tag', 'old:sibling', 'parity:漢字', 'zeta:last']


def record(session):
    from qtpy import QtWidgets as QW, QtCore as QC
    from qtpy.QtTest import QTest
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUITagSuggestions as S
    from hydrus.client.gui import ClientGUICore as CGC, ClientGUIAsync, ClientGUIDialogsQuick as Q
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:2]]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(local, [
        U.ContentUpdate(HC.CONTENT_TYPE_TAG_PARENTS, HC.CONTENT_UPDATE_ADD, ('alpha:first', 'group:alpha')),
        U.ContentUpdate(HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_ADD, ('old:sibling', 'new:sibling')),
        U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ('plain tag', {hashes[0]})),
    ]))
    for _ in range(20):
        if not c.WriteSynchronous('sync_tag_display_maintenance', local, 0.5):
            break
    for tag in ['recent:one', 'recent:two']:
        c.WriteSynchronous('push_recent_tags', local, [tag])
        time.sleep(0.02)
    c.new_options.SetSuggestedTagsMostUsed(local, MOST_USED)
    c.new_options.SetBoolean('show_related_tags', True)
    c.new_options.SetBoolean('show_file_lookup_script_tags', False)
    c.new_options.SetNoneableInteger('num_recent_tags', 20)
    c.new_options.SetNoneableString('suggested_tags_layout', 'columns')
    c.new_options.SetStringList('favourite_tags', [])
    medias = [ClientMediaSingle.MediaSingle(r) for r in c.Read('media_results', hashes)]
    state = {}

    def build():
        activated = []
        panel = S.SuggestedTagsPanel(c.gui, local, CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, False,
                                     lambda tags, **kw: activated.append(dict(tags=sorted(tags), only_add=kw.get('only_add'))))
        panel.SetMedia(medias)
        panel.resize(900, 500)
        panel.show()
        state.update(panel=panel, activated=activated)
    c.CallBlockingToQt(c.gui, build)
    time.sleep(1.5)  # the lists' sibling/parent lookups publish on the Qt loop

    def work():
        panel, activated = state['panel'], state['activated']
        box = panel._favourite_tags._favourite_tags
        copied, launched, menus, questions = [], [], [], []
        old_popup, old_pub, old_job, old_yes = CGC.core().PopupMenu, c.pub, ClientGUIAsync.AsyncQtJob, Q.GetYesNo

        def popup(owner, menu):
            menus.append(menu)

        def pub(topic, *args, **kwargs):
            if topic == 'clipboard':
                copied.append(args[1])
            elif topic in ['new_page_query', 'new_page_duplicates']:
                launched.append(dict(topic=topic, predicates=[p.ToString() for p in kwargs['initial_predicates']],
                                     page_name=kwargs.get('page_name'), activate_window=kwargs.get('activate_window')))
            else:
                old_pub(topic, *args, **kwargs)

        class ImmediateJob:
            def __init__(self, owner, callback, publish):
                self.callback, self.publish = callback, publish

            def start(self):
                self.publish(self.callback())

        def yes(win, message, *a, **kw):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted
        CGC.core().PopupMenu, c.pub, ClientGUIAsync.AsyncQtJob, Q.GetYesNo = popup, pub, ImmediateJob, yes
        h = box.fontMetrics().height()

        def settle():
            for _ in range(4):
                QW.QApplication.processEvents()

        def drawn(b=box):
            return [[''.join(text for (text, colour) in row) for row in b._GetRowsOfTextsAndColours(term)] for term in b._ordered_terms]

        def selected(b=box):
            return [t.GetTag() for t in b._ordered_terms if t in b._selected_terms]

        def at(tag, b=box):
            term = next(t for t in b._ordered_terms if t.GetTag() == tag)
            return QC.QPoint(10, b._terms_to_positional_indices[term] * h + h // 2)

        def tree(menu):
            return [dict(label=a.text(), separator=a.isSeparator(), children=tree(a.menu()) if a.menu() is not None else None)
                    for a in menu.actions()]

        def find(menu, label):
            for a in menu.actions():
                if a.text() == label:
                    return a
            return None

        steps = []

        def step(name, **extra):
            settle()
            steps.append(dict(step=name, selected=selected(), **extra))

        def click(tag, button=QC.Qt.MouseButton.LeftButton, mods=QC.Qt.KeyboardModifier.NoModifier, point=None):
            QTest.mouseClick(box.widget(), button, mods, point if point is not None else at(tag))

        def key(k, mods=QC.Qt.KeyboardModifier.NoModifier):
            box.setFocus()
            QTest.keyClick(box, k, mods)

        def menu_at(name, point, trigger_all=True):
            QTest.mouseClick(box.widget(), QC.Qt.MouseButton.RightButton, QC.Qt.KeyboardModifier.NoModifier, point)
            box.ShowMenuFromSignal(point)
            settle()
            if not menus:
                step(name, menu=None)
                return
            menu = menus.pop()
            out = dict(tree=tree(menu), copies=[], launches=[], favourites=[])
            if trigger_all:
                copy = find(menu, 'copy')
                for a in (copy.menu().actions() if copy else []):
                    if a.isSeparator():
                        continue
                    n = len(copied)
                    a.trigger()
                    out['copies'].append(dict(label=a.text(), payload=copied[-1] if len(copied) > n else None))
                opener = find(menu, 'open')
                for a in (opener.menu().actions() if opener else []):
                    if a.isSeparator():
                        continue
                    n = len(launched)
                    a.trigger()
                    out['launches'].append(dict(label=a.text(), launched=launched[n:]))
            step(name, menu=out)
            return menu

        out = dict(service='my tags', most_used=MOST_USED, files=[x.hex() for x in hashes],
                   parents=[['alpha:first', 'group:alpha']], siblings=[['old:sibling', 'new:sibling']],
                   mapped=[['plain tag', [0]]], recent_pushed=['recent:one', 'recent:two'],
                   options=dict(show_parent_decorators=c.new_options.GetBoolean('show_parent_decorators_on_storage_taglists'),
                                show_sibling_decorators=c.new_options.GetBoolean('show_sibling_decorators_on_storage_taglists'),
                                expand_parents=c.new_options.GetBoolean('expand_parents_on_storage_taglists')),
                   tags=[t.GetTag() for t in box._ordered_terms], drawn=drawn())
        try:
            step('initial')
            click('alpha:first'); step('click alpha:first')
            key(QC.Qt.Key.Key_Down); step('down')
            key(QC.Qt.Key.Key_Down, QC.Qt.KeyboardModifier.ShiftModifier); step('shift down')
            click('plain tag', mods=QC.Qt.KeyboardModifier.ControlModifier); step('ctrl click plain tag adds it')
            click('parity:漢字', mods=QC.Qt.KeyboardModifier.ControlModifier); step('ctrl click parity takes it away')
            click('old:sibling'); step('plain click on a selected row keeps the selection')
            key(QC.Qt.Key.Key_C, QC.Qt.KeyboardModifier.ControlModifier); step('ctrl c', copied=copied[-1])
            menu_at('menu on a selected row (two selected)', at('old:sibling'))
            click('alpha:first', mods=QC.Qt.KeyboardModifier.ControlModifier); step('ctrl click alpha:first adds it')
            key(QC.Qt.Key.Key_C, QC.Qt.KeyboardModifier.ControlModifier | QC.Qt.KeyboardModifier.ShiftModifier); step('ctrl shift c includes parents', copied=copied[-1])
            menu_at('menu (three selected, one with a parent)', at('plain tag'))
            click('series:spaced name'); step('plain click on an unselected row selects only it')
            menu_at('menu (one selected, with namespace and spaces)', at('series:spaced name'))
            key(QC.Qt.Key.Key_Escape); step('escape')
            key(QC.Qt.Key.Key_C, QC.Qt.KeyboardModifier.ControlModifier); step('ctrl c with none selected', copied=copied[-1])
            key(QC.Qt.Key.Key_A, QC.Qt.KeyboardModifier.ControlModifier); step('ctrl a')
            menu_at('menu with all selected', at('zeta:last'))
            key(QC.Qt.Key.Key_Escape); step('escape again')
            menu_at('menu on alpha:first (unselected, so it is selected first)', at('alpha:first'))
            m = menu_at('menu on old:sibling', at('old:sibling'))
            menu_at('menu on parity:漢字', at('parity:漢字'))
            # the decorator entries change what is drawn
            for label in ['collapse parent rows', 'expand parent rows', 'hide parent decorators', 'show parent decorators', 'hide sibling decorators', 'show sibling decorators']:
                click('alpha:first')
                QTest.mouseClick(box.widget(), QC.Qt.MouseButton.RightButton, QC.Qt.KeyboardModifier.NoModifier, at('alpha:first'))
                box.ShowMenuFromSignal(at('alpha:first'))
                settle()
                menu = menus.pop()
                top = [a.text() for a in menu.actions() if not a.isSeparator()][:3]
                a = find(menu, label)
                if a is not None:
                    a.trigger()
                    settle()
                step('decorator ' + label, offered=top, chosen=a is not None, drawn=drawn())
            # empty space below the rows
            menu_at('menu on empty space', QC.QPoint(10, (box._total_positional_rows + 2) * h))
            # keyboard navigation
            click('alpha:first'); step('click alpha:first again')
            key(QC.Qt.Key.Key_Up); step('up wraps to the end')
            key(QC.Qt.Key.Key_Down); step('down wraps to the start')
            key(QC.Qt.Key.Key_End); step('end')
            key(QC.Qt.Key.Key_Home); step('home')
            key(QC.Qt.Key.Key_N, QC.Qt.KeyboardModifier.ControlModifier); step('ctrl n')
            key(QC.Qt.Key.Key_P, QC.Qt.KeyboardModifier.ControlModifier); step('ctrl p')
            key(QC.Qt.Key.Key_PageDown); step('page down')
            key(QC.Qt.Key.Key_PageUp); step('page up')
            key(QC.Qt.Key.Key_Down, QC.Qt.KeyboardModifier.ControlModifier); step('ctrl down')
            key(QC.Qt.Key.Key_Down, QC.Qt.KeyboardModifier.ShiftModifier); step('shift down after ctrl down')
            click('zeta:last', mods=QC.Qt.KeyboardModifier.ShiftModifier); step('shift click zeta:last')
            click('series:spaced name', mods=QC.Qt.KeyboardModifier.ShiftModifier | QC.Qt.KeyboardModifier.ControlModifier); step('ctrl shift click series')
            click(None, point=QC.QPoint(10, (box._total_positional_rows + 2) * h)); step('click empty space')
            # middle click opens a search page for the clicked tag
            n = len(launched)
            QTest.mouseClick(box.widget(), QC.Qt.MouseButton.MiddleButton, QC.Qt.KeyboardModifier.NoModifier, at('plain tag'))
            step('middle click plain tag', launched=launched[n:])
            click('series:spaced name', mods=QC.Qt.KeyboardModifier.ControlModifier)
            n = len(launched)
            QTest.mouseClick(box.widget(), QC.Qt.MouseButton.MiddleButton, QC.Qt.KeyboardModifier.ShiftModifier, at('plain tag'))
            step('shift middle click with two selected', launched=launched[n:])
            # enter activates the selection, which leaves the list
            click('zeta:last')
            n = len(activated)
            key(QC.Qt.Key.Key_Return); step('return', activated=activated[n:], tags=[t.GetTag() for t in box._ordered_terms])
            n = len(activated)
            QTest.mouseDClick(box.widget(), QC.Qt.MouseButton.LeftButton, QC.Qt.KeyboardModifier.NoModifier, at('parity:漢字'))
            step('double click', activated=activated[n:], tags=[t.GetTag() for t in box._ordered_terms])
            key(QC.Qt.Key.Key_Delete); step('delete does nothing', tags=[t.GetTag() for t in box._ordered_terms])
            # favourites: add, then the menu again shows remove
            menu = menu_at('menu on plain tag', at('plain tag'), trigger_all=False)
            fav = find(menu, 'favourites')
            add = find(fav.menu(), 'add "plain tag" to favourites') if fav else None
            if add is not None:
                add.trigger()
            step('favourite added', favourites=c.new_options.GetStringList('favourite_tags'))
            menu = menu_at('menu on plain tag after favouriting', at('plain tag'), trigger_all=False)
            fav = find(menu, 'favourites')
            for a in (fav.menu().actions() if fav else []):
                if a.text().startswith('remove "plain tag" from favourites'):
                    a.trigger()
            step('favourite removed', favourites=c.new_options.GetStringList('favourite_tags'), questions=list(questions))
            for a in (fav.menu().actions() if fav else []):
                if a.text() == 'remove "plain tag" from most used for "my tags"':
                    a.trigger()
            step('most used toggled from the menu', most_used=list(c.new_options.GetSuggestedTagsMostUsed(local)), questions=list(questions))
            out['steps'] = steps
            # the other two lists are the same kind of list
            others = {}
            for name, b in [('recent', panel._recent_tags._recent_tags), ('related', panel._related_tags._related_tags)]:
                if not b._ordered_terms:
                    others[name] = dict(tags=[], tree=None)
                    continue
                first = b._ordered_terms[0]
                QTest.mouseClick(b.widget(), QC.Qt.MouseButton.LeftButton, QC.Qt.KeyboardModifier.NoModifier, QC.QPoint(10, h // 2))
                b.ShowMenuFromSignal(QC.QPoint(10, h // 2))
                settle()
                others[name] = dict(tags=[t.GetTag() for t in b._ordered_terms], selected=selected(b), tree=tree(menus.pop()) if menus else None)
            out['other_lists'] = others
        finally:
            CGC.core().PopupMenu, c.pub, ClientGUIAsync.AsyncQtJob, Q.GetYesNo = old_popup, old_pub, old_job, old_yes
            panel.hide()
            panel.deleteLater()
        return out
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()
