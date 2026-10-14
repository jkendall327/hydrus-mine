#!/usr/bin/env python3
"""Record the options dialog's search and Apply/Cancel behaviour.

Fresh temporary `basic` client; the real `ManageOptionsPanel` (as file >
options makes it).

Search: for each query the options search box's completer is given the text
(`setCompletionPrefix`) and the matches its popup would list are written
down, in order; then the first match is activated as a click would and the
page it shows, whether its widget is highlighted and what the search box
holds afterwards are recorded.

Apply: edits on several pages are made through the real controls (trash size
to "no size limit", the recycle-bin check flipped, page name characters,
an unparseable slideshow duration, an http proxy, a duplicate score);
`CommitChanges` is called (what OK does), and the options, the popup text
and the tab-name option are read back. A second panel with edits that is not
committed (what Cancel does) leaves the options as they were.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'options_apply_search.json')
QUERIES = ['TRASH', 'zoom', 'audio', 'recycle', 'slideshow dur', 'http', 'no such option anywhere', 'gui pages']


def record(session):
    from qtpy import QtWidgets as QW, QtCore as QC
    from hydrus.core import HydrusConstants as HC, HydrusData
    from hydrus.client import ClientGlobals as CG
    from hydrus.client.gui.panels.options import ClientGUIManageOptionsPanel as M
    c = session.controller
    shown = []
    old_show = HydrusData.ShowText
    HydrusData.ShowText = lambda text, *a, **k: shown.append(text)

    def settle():
        for _ in range(6):
            QW.QApplication.processEvents()

    def work():
        out = {}
        panel = M.ManageOptionsPanel(c.gui)
        panel.show()
        settle()
        book = panel._listbook
        pages = [book.tabText(i) for i in range(book.count())]
        out['pages'] = pages
        out['opens_on'] = book.tabText(book.GetCurrentPageIndex())
        completer = panel._completer
        searches = []
        for query in QUERIES:
            completer.setCompletionPrefix(query)
            model = completer.completionModel()
            matches = [model.index(i, 0).data() for i in range(model.rowCount())]
            entry = dict(query=query, matches=matches)
            if matches:
                text = matches[0]
                completer.activated.emit(text)
                settle()
                widget = panel._completer_map[text][1]
                entry.update(first=text, page=book.tabText(book.GetCurrentPageIndex()),
                             highlighted=widget.objectName() == 'HydrusOptionsSearchHighlight',
                             search_text_after=panel._options_search.text())
            searches.append(entry)
        out['searches'] = searches
        panel.deleteLater()
        settle()

        def current():
            return dict(
                delete_to_recycle_bin=HC.options['delete_to_recycle_bin'],
                trash_max_size=HC.options['trash_max_size'],
                trash_max_age=HC.options['trash_max_age'],
                max_page_name_chars=c.new_options.GetInteger('max_page_name_chars'),
                slideshow=c.new_options.GetSlideshowDurations() if hasattr(c.new_options, 'GetSlideshowDurations') else None,
                http_proxy=c.new_options.GetNoneableString('http_proxy'),
                score_has_audio=c.new_options.GetInteger('duplicate_comparison_score_has_audio'),
            )

        def edit(panel):
            pg = {name: book_page(panel, name) for name in ['files and trash', 'gui pages', 'media viewer', 'connection', 'duplicates']}
            pg['files and trash']._trash_max_size.SetValue(None)
            pg['files and trash']._delete_to_recycle_bin.setChecked(not pg['files and trash']._delete_to_recycle_bin.isChecked())
            pg['gui pages']._max_page_name_chars.setValue(2)
            pg['media viewer']._slideshow_durations.setText('1.0,soon')
            pg['connection']._http_proxy.SetValue('http://127.0.0.1:8080')
            pg['duplicates']._duplicate_comparison_score_has_audio.setValue(-30)

        def book_page(panel, name):
            b = panel._listbook
            for i in range(b.count()):
                if b.tabText(i) == name:
                    return b.GetPage(i) if hasattr(b, 'GetPage') else b.widget(i)
            raise KeyError(name)

        def tab_names():
            notebook = c.gui._notebook
            return [notebook.tabText(i) for i in range(notebook.count())]

        def notify():
            c.pub('notify_new_options')
            c.pub('refresh_page_name')
            for _ in range(20):
                time.sleep(0.02)
                settle()

        out['before'] = current()
        out['tab_names_before'] = tab_names()
        cancelled = M.ManageOptionsPanel(c.gui)
        edit(cancelled)
        cancelled.deleteLater()
        settle()
        notify()
        out['after_cancel'] = current()
        out['tab_names_after_cancel'] = tab_names()
        out['popups_after_cancel'] = list(shown)
        applied = M.ManageOptionsPanel(c.gui)
        edit(applied)
        applied.CommitChanges()
        applied.deleteLater()
        settle()
        notify()
        out['after_apply'] = current()
        out['tab_names_after_apply'] = tab_names()
        out['popups_after_apply'] = list(shown)
        return out
    try:
        return c.CallBlockingToQt(c.gui, work)
    finally:
        HydrusData.ShowText = old_show


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
