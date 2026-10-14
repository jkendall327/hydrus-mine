#!/usr/bin/env python3
"""Record closing pages that have importer queues: a notebook of
a, imp1 (3 queued URLs), b, imp2 (2 queued URLs), c, closed from the tab
menu's "close" > other pages / pages to the left / pages to the right, and
a single page from its "close page". A page holding an importer with queued
items objects ("This is a urls import page holding N import objects."), so
the bulk close asks the longer question with a statement of the objections
(`ShowReasonsAndPagesConfirmationDialog`, a yes/no/"no, but show me the
pages" dialog whose yes starts disabled) and a single page asks "Close
<name>?" with the objection. Each is answered yes, no, and (the bulk ones)
"no, but show me the pages", which lists a button per objecting page. The
real question panel is built to record its buttons.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_tab_close_objections.py
       (writes fixtures/tab_close_objections.json)
"""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsQuick, ClientGUIReviewWindowsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.pages import ClientGUIPageManager, ClientGUIPages
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsButtonQuestions as Q
    controller, gui = session.controller, session.controller.gui
    context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
    asked, lists, shown = [], [], []
    answer = ['yes']
    old = (ClientGUIDialogsQuick.GetYesNo, ClientGUIDialogsQuick.GetYesNoNo, ClientGUIReviewWindowsQuick.OpenListButtons)
    old_show = gui.ShowPage

    def panel_of(message, no_tuples, disable_yes_initially):
        dlg = ClientGUITopLevelWindowsPanels.DialogCustomButtonQuestion(gui, 'Are you sure?')
        panel = Q.QuestionYesNoNoPanel(dlg, message, no_tuples=no_tuples, disable_yes_initially=disable_yes_initially)
        dlg.SetPanel(panel)
        dlg.show()
        QW.QApplication.processEvents()
        out = dict(buttons=[dict(text=b.text(), enabled=b.isEnabled()) for b in panel.findChildren(QW.QPushButton)],
            label=[x.text() for x in panel.findChildren(QW.QLabel) if x.text()])
        dlg.hide()
        dlg.deleteLater()
        return out

    def yesno(win, message, **kwargs):
        asked.append(dict(dialog='yes/no', message=message, title=kwargs.get('title', 'Are you sure?')))
        return QW.QDialog.DialogCode.Accepted if answer[0] == 'yes' else QW.QDialog.DialogCode.Rejected

    def yesnono(win, message, **kwargs):
        asked.append(dict(dialog='yes/no/no', message=message, title=kwargs.get('title', 'Are you sure?'),
            no_tuples=kwargs.get('no_tuples'), disable_yes_initially=kwargs.get('disable_yes_initially'),
            auto_yes_time=kwargs.get('auto_yes_time'),
            panel=panel_of(message, kwargs.get('no_tuples'), kwargs.get('disable_yes_initially'))))
        if answer[0] == 'yes':
            return QW.QDialog.DialogCode.Accepted, None
        return QW.QDialog.DialogCode.Rejected, 'show' if answer[0] == 'show' else 'no'

    def list_buttons(win, title, choice_tuples, message=''):
        lists.append(dict(title=title, message=message, choices=[(label, tip) for label, call, tip in choice_tuples]))
        for label, call, tip in choice_tuples:
            try:
                call()
            except Exception as e:
                shown.append(str(e))
    ClientGUIDialogsQuick.GetYesNo = yesno
    ClientGUIDialogsQuick.GetYesNoNo = yesnono
    ClientGUIReviewWindowsQuick.OpenListButtons = list_buttons
    gui.ShowPage = lambda page_key: shown.append(page_key.hex())

    def build():
        top = ClientGUIPages.PagesNotebook(gui, 'close objections')
        top.NewPageQuery(context, page_name='a', forced_insertion_index=0)
        for at, name, count in ((1, 'imp1', 3), (3, 'imp2', 2)):
            manager = ClientGUIPageManager.CreatePageManagerImportURLs(page_name=name)
            page = top.NewPage(manager, forced_insertion_index=at)
            urls = page.GetPageManager().GetVariable('urls_import')
            if not urls.IsPaused():
                urls.PausePlay()
            urls.PendURLs(['https://files.example/%s-%d.jpg' % (name, i) for i in range(count)])
        top.NewPageQuery(context, page_name='b', forced_insertion_index=2)
        top.NewPageQuery(context, page_name='c', forced_insertion_index=4)
        top.setCurrentIndex(0)
        return top
    cases = []

    def drive():
        for label, method, index in (('other pages', '_CloseOtherPages', 0), ('pages to the left', '_CloseLeftPages', 2),
                ('pages to the right', '_CloseRightPages', 2), ('pages to the right (nothing objects)', '_CloseRightPages', 3),
                ('close page', '_ClosePage', 1), ('close page (nothing objects)', '_ClosePage', 0)):
            for choice in ('yes', 'no', 'show') if method != '_ClosePage' else ('yes', 'no'):
                top = build()
                answer[0] = choice
                asked.clear()
                lists.clear()
                shown.clear()
                before = [top.widget(i).GetName() for i in range(top.count())]
                getattr(top, method)(index)
                QW.QApplication.processEvents()
                cases.append(dict(menu=label, index=index, answer=choice, before=before, asked=list(asked), lists=list(lists),
                    shown=list(shown), after=[top.widget(i).GetName() for i in range(top.count())]))
        return cases
    try:
        return controller.CallBlockingToQt(gui, drive)
    finally:
        ClientGUIDialogsQuick.GetYesNo, ClientGUIDialogsQuick.GetYesNoNo, ClientGUIReviewWindowsQuick.OpenListButtons = old
        gui.ShowPage = old_show


def main():
    import hydrus_driver, record_api
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as work:
        out = Path(work) / 'close.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures' / 'tab_close_objections.json').write_text(json.dumps({'cases': result}, indent=2) + '\n')


if __name__ == '__main__':
    main()
