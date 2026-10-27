#!/usr/bin/env python3
"""Drive the real frame's page tabs and pages tree with Qt mouse events.

Records what each gesture on the notebook's tab bar does (double-clicks on a
tab and on empty tab space, left and middle; right-clicks on a tab, on empty
space and on a nested notebook's bar) and what the pages tree does with a
right-click and a drag and drop of a row. Events go through QTest to the
widgets of the real main window, so the notebook's own event filter and its
tab bar's handlers decide. Only the dialogs are scripted: the rename text
entry answers from a list, the page chooser records which notebook asked and
where, and the tab menu is captured instead of shown. Source Hydrus stays
unchanged.
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtCore as QC, QtGui as QG, QtWidgets as QW, QtTest
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUICore, ClientGUIDialogsQuick
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.core import HydrusExceptions
    from record_main_menu import tree as menu_tree
    c = session.controller
    gui = c.gui
    core = ClientGUICore.core()
    old_popup, old_text = core.PopupMenu, ClientGUIDialogsQuick.EnterText
    old_choose = ClientGUIPages.PagesNotebook._ChooseNewPage
    menus, texts, chosen, answers = [], [], [], []

    def text(window, message, default='', **kwargs):
        value = answers.pop(0)
        texts.append({'message': message, 'default': default, 'answer': value})
        if value is None:
            raise HydrusExceptions.CancelledException()
        return value

    def choose(notebook, insertion_index=None):
        chosen.append({'notebook': notebook.GetName(), 'insertion_index': insertion_index})

    live = []

    def popup(window, menu):
        live[:] = [menu]
        menus.append({'window': type(window).__name__, 'notebook': window.GetName() if hasattr(window, 'GetName') else None, 'entries': menu_tree(menu)})

    ClientGUIDialogsQuick.EnterText = text
    core.PopupMenu = popup
    ClientGUIPages.PagesNotebook._ChooseNewPage = choose

    def drive():
        c.new_options.SetInteger('page_file_count_display', CC.PAGE_FILE_COUNT_DISPLAY_NONE)
        c.new_options.SetNoneableInteger('treeview_alignment', CC.DIRECTION_LEFT)
        c.new_options.SetBoolean('treeview_collapse_all_children_upon_parent_closed', False)
        gui._RebuildMainFrameLayout()
        root = gui._notebook
        old = list(range(root.count()))
        context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        root.NewPageQuery(context, page_name='alpha', select_page=False)
        nested = root.NewPagesNotebook(name='nested', give_it_a_blank_page=False)
        nested.NewPageQuery(context, page_name='inner one', select_page=False)
        nested.NewPageQuery(context, page_name='inner two', select_page=False)
        root.NewPageQuery(context, page_name='omega', select_page=False)
        root._ClosePages(old, 'recording', polite=False)
        root.setCurrentIndex(1)
        nested.setCurrentIndex(0)

        def settle():
            for _ in range(5):
                QW.QApplication.processEvents()

        def tree_pages(notebook):
            out = []
            for page in notebook.GetPages():
                row = {'name': page.GetName()}
                if isinstance(page, ClientGUIPages.PagesNotebook):
                    row['children'] = tree_pages(page)
                out.append(row)
            return out

        def state():
            settle()
            return {'pages': tree_pages(root), 'shown': gui.GetCurrentPage().GetName()}

        steps = []

        def step(action, do, answer=None):
            menus.clear(); texts.clear(); chosen.clear()
            answers[:] = [] if answer is None else answer
            do()
            settle()
            steps.append({'action': action, 'menus': list(menus), 'texts': list(texts),
                          'chosen': list(chosen), **state()})

        settle()
        bar = root.tabBar()
        inner_bar = nested.tabBar()
        left, middle, right = QC.Qt.MouseButton.LeftButton, QC.Qt.MouseButton.MiddleButton, QC.Qt.MouseButton.RightButton

        def tab(bar, name):
            notebook = bar.parentWidget()
            index = next(i for i, p in enumerate(notebook.GetPages()) if p.GetName() == name)
            return bar.tabRect(index).center()

        def space(bar):
            # empty space after the last tab, in the bar itself when it is
            # wide enough, else in the notebook beside it
            last = bar.tabRect(bar.count() - 1)
            point = QC.QPoint(last.right() + 40, last.center().y())
            if bar.rect().contains(point):
                return bar, point
            notebook = bar.parentWidget()
            return notebook, notebook.mapFromGlobal(bar.mapToGlobal(point))

        geometry = {'bar': [bar.width(), bar.height()], 'tabs': [[bar.tabRect(i).x(), bar.tabRect(i).y(), bar.tabRect(i).width(), bar.tabRect(i).height()] for i in range(bar.count())],
                    'space_widget': type(space(bar)[0]).__name__}

        def click(widget, button, point):
            QtTest.QTest.mouseClick(widget, button, pos=point)

        def dclick(widget, button, point):
            # what a real double-click delivers: press, release, press,
            # double-click, release (QTest.mouseDClick sends only the last
            # three)
            QtTest.QTest.mousePress(widget, button, pos=point)
            QtTest.QTest.mouseRelease(widget, button, pos=point)
            QtTest.QTest.mouseDClick(widget, button, pos=point)

        step('left double-click tab alpha, rename', lambda: dclick(bar, left, tab(bar, 'alpha')), ['renamed alpha'])
        step('left double-click tab omega, cancel', lambda: dclick(bar, left, tab(bar, 'omega')), [None])
        step('left double-click tab nested (a page of pages)', lambda: dclick(bar, left, tab(bar, 'nested')), ['renamed nested'])
        step('left double-click inner tab inner two', lambda: dclick(inner_bar, left, tab(inner_bar, 'inner two')), ['renamed two'])
        step('left click tab omega', lambda: click(bar, left, tab(bar, 'omega')))
        step('left click empty space', lambda: click(*space(bar)[:1], left, space(bar)[1]))
        step('left double-click empty space', lambda: dclick(space(bar)[0], left, space(bar)[1]))
        step('middle click empty space', lambda: click(space(bar)[0], middle, space(bar)[1]))
        step('middle double-click empty space', lambda: dclick(space(bar)[0], middle, space(bar)[1]))
        step('right press tab alpha (no release)', lambda: QtTest.QTest.mousePress(bar, right, pos=tab(bar, 'renamed alpha')))
        step('right release tab alpha', lambda: QtTest.QTest.mouseRelease(bar, right, pos=tab(bar, 'renamed alpha')))
        step('right click tab nested', lambda: click(bar, right, tab(bar, 'renamed nested')))
        step('right click empty space', lambda: click(space(bar)[0], right, space(bar)[1]))
        root.setCurrentIndex(1)
        settle()
        step('right click inner tab inner one', lambda: click(inner_bar, right, tab(inner_bar, 'inner one')))
        step('right click inner empty space', lambda: click(space(inner_bar)[0], right, space(inner_bar)[1]))
        step('left double-click inner empty space', lambda: dclick(space(inner_bar)[0], left, space(inner_bar)[1]))
        step('middle double-click tab omega', lambda: dclick(bar, middle, tab(bar, 'omega')))

        # the pages tree, on fresh pages
        old = list(range(root.count()))
        root.NewPageQuery(context, page_name='alpha', select_page=False)
        nested = root.NewPagesNotebook(name='nested', give_it_a_blank_page=False)
        nested.NewPageQuery(context, page_name='inner one', select_page=False)
        nested.NewPageQuery(context, page_name='inner two', select_page=False)
        root.NewPageQuery(context, page_name='omega', select_page=False)
        root._ClosePages(old, 'recording', polite=False)
        root.setCurrentIndex(1)
        nested.setCurrentIndex(0)
        tree = gui._tabs_tree_view
        model = tree.model()
        tree.expandAll()
        settle()

        def walk(parent=QC.QModelIndex()):
            for row in range(model.rowCount(parent)):
                index = model.index(row, 0, parent)
                yield index
                yield from walk(index)

        def name(index):
            return model._IndexDataOrRoot(index).obj.GetName() if index.isValid() else None

        def find(label):
            return next(i for i in walk() if name(i) == label)

        def row_point(label):
            return tree.visualRect(find(label)).center()

        tree_steps = []

        def tree_step(action, do):
            menus.clear()
            do()
            settle()
            tree_steps.append({'action': action, 'menus': list(menus), **state(),
                               'rows': [name(i) for i in walk()], 'current': name(tree.currentIndex())})

        def right_click(point):
            # a right-click; offscreen, QTest's click brings no context menu
            # request, so the context menu event that Qt on Linux sends on
            # the press is sent by hand (see the limits)
            QtTest.QTest.mouseClick(tree.viewport(), right, pos=point)
            if not menus:
                event = QG.QContextMenuEvent(QG.QContextMenuEvent.Reason.Mouse, point, tree.viewport().mapToGlobal(point))
                QW.QApplication.sendEvent(tree.viewport(), event)

        tree_step('initial', lambda: None)
        rows = {name(i): [tree.visualRect(i).x(), tree.visualRect(i).y(), tree.visualRect(i).width(), tree.visualRect(i).height()] for i in walk()}
        tree_step('right click tree row alpha', lambda: right_click(row_point('alpha')))
        tree_step('right click tree row nested', lambda: right_click(row_point('nested')))
        tree_step('right click tree row inner one', lambda: right_click(row_point('inner one')))
        bottom = tree.viewport().rect().bottomLeft() + QC.QPoint(20, -5)
        tree_step('right click tree empty space', lambda: right_click(bottom))

        def trigger(*labels):
            menu = live[0]
            for label in labels:
                action = next(a for a in menu.actions() if a.text() == label)
                if action.menu() is not None:
                    menu = action.menu()
                else:
                    action.trigger()

        # a row in a notebook that is not on the way to the page shown
        root.setCurrentIndex(0)
        settle()
        tree_step('right click tree row inner two, alpha shown', lambda: right_click(row_point('inner two')))
        tree_step('its select > page to the right', lambda: trigger('select', 'page to the right'))
        tree_step('right click tree row inner two again', lambda: right_click(row_point('inner two')))
        tree_step('its move page > left', lambda: trigger('move page', 'left'))
        tree_step('right click tree row inner one, alpha shown', lambda: right_click(row_point('inner one')))
        tree_step('its close page', lambda: trigger('close page'))
        old = list(range(root.count()))
        root.NewPageQuery(context, page_name='alpha', select_page=False)
        nested = root.NewPagesNotebook(name='nested', give_it_a_blank_page=False)
        nested.NewPageQuery(context, page_name='inner one', select_page=False)
        nested.NewPageQuery(context, page_name='inner two', select_page=False)
        root.NewPageQuery(context, page_name='omega', select_page=False)
        root._ClosePages(old, 'recording', polite=False)
        root.setCurrentIndex(1)
        nested.setCurrentIndex(0)
        tree.expandAll()
        tree_step('fresh pages for the drags', lambda: None)

        # drag and drop. The offscreen platform's QDrag returns at once
        # without a drag loop, and Qt only lets an InternalMove view take a
        # drop whose QDrag it started, so the view is put in DragDrop mode
        # for the drop events: Qt's own drop position (above, on or below
        # the row under the pointer) and the model's dropMimeData decide.
        # The one thing DragDrop mode skips is InternalMove's refusal of a
        # drop onto the dragged row or its descendants (droppingOnItself),
        # so that case is not driven here.
        def drag(action, source, target):
            settle()
            target_point = target()
            viewport = tree.viewport()
            index = find(source)
            QtTest.QTest.mousePress(viewport, left, pos=row_point(source))
            QtTest.QTest.mouseRelease(viewport, left, pos=row_point(source))
            mime = model.mimeData([index])
            tree.setDragDropMode(QW.QAbstractItemView.DragDropMode.DragDrop)
            try:
                move = QC.Qt.DropAction.MoveAction
                enter = QG.QDragEnterEvent(target_point, move, mime, left, QC.Qt.KeyboardModifier.NoModifier)
                QW.QApplication.sendEvent(viewport, enter)
                over = QG.QDragMoveEvent(target_point, move, mime, left, QC.Qt.KeyboardModifier.NoModifier)
                QW.QApplication.sendEvent(viewport, over)
                indicator = tree.dropIndicatorPosition().name
                drop = QG.QDropEvent(QC.QPointF(target_point), move, mime, left, QC.Qt.KeyboardModifier.NoModifier)
                QW.QApplication.sendEvent(viewport, drop)
            finally:
                tree.setDragDropMode(QW.QAbstractItemView.DragDropMode.InternalMove)
            tree_step(action, lambda: None)
            tree_steps[-1].update({'indicator': indicator, 'accepted': drop.isAccepted(),
                                   'point': [target_point.x(), target_point.y()]})

        def above(label):
            rect = tree.visualRect(find(label))
            return QC.QPoint(rect.center().x(), rect.top() + 1)

        def below(label):
            rect = tree.visualRect(find(label))
            return QC.QPoint(rect.center().x(), rect.bottom() - 1)

        drag('drag omega above alpha', 'omega', lambda: above('alpha'))
        drag('drag omega onto nested', 'omega', lambda: row_point('nested'))
        drag('drag inner one below alpha', 'inner one', lambda: below('alpha'))
        drag('drag alpha onto inner two (a page)', 'alpha', lambda: row_point('inner two'))
        drag('drag inner two above inner one', 'inner two', lambda: above('inner one'))
        drag('drag omega below inner one', 'omega', lambda: below('inner one'))
        drag('drag inner one to empty space', 'inner one', lambda: bottom)
        tree_step('double-click tree empty space', lambda: QtTest.QTest.mouseDClick(tree.viewport(), left, pos=bottom))
        return {'geometry': geometry, 'tabs': steps, 'tree': tree_steps, 'tree_rows': rows,
                'limits': ['the page chooser and the text entry are scripted; the tab menu is captured rather than shown',
                           'tree drops are delivered as drag enter, move and drop events in DragDrop mode (offscreen Qt has no drag loop); a drop onto the dragged row or its descendants, which InternalMove refuses, is not driven',
                           'a QTest right-click gives the offscreen tree no customContextMenuRequested, so the tree menu is reached by a synthetic QContextMenuEvent at the click (Qt on Linux sends it on the press)']}

    try:
        return c.CallBlockingToQt(gui, drive)
    finally:
        core.PopupMenu, ClientGUIDialogsQuick.EnterText = old_popup, old_text
        ClientGUIPages.PagesNotebook._ChooseNewPage = old_choose


def child(out):
    import hydrus_driver, record_api
    Path(out).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / 'out.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    dest = HERE / 'fixtures' / 'tab_strip_interactions.json'
    dest.write_text(json.dumps(result, indent=2) + '\n')
    print(f'wrote {dest}')


if __name__ == '__main__':
    main()
