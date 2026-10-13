#!/usr/bin/env python3
"""Record the tag suggestions Options rows reaching the suggestion panels.

Fresh temporary basic client; 'my tags' gets a small corpus over six files and
twelve recent tags pushed a moment apart. For each scenario the real Options
page (TagSuggestionsPanel) has its related durations, concurrence threshold and
two recent counts set through its own controls, and its UpdateOptions writes
them. Then the real SuggestedTagsPanel (columns layout) is built over the
first two files: its recent list is recorded, and its related panel's quick,
medium and thorough buttons are pressed (worker and publisher forwarded
synchronously), recording what each asks the database for (the time it gives,
from the duration rows) and the tags it lists (filtered by the concurrence
threshold). The interactive tag entry's quick dialog is opened for the same
service (its exec scripted to read the suggestion buttons and cancel), to show
the second recent count's effect.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'suggestion_options_consumers.json')

CORPUS = [('source:seed', [0, 1, 2, 3]), ('context:seed', [2, 3]), ('alpha:first', [0, 1, 2]),
          ('beta:second', [3, 4, 5]), ('alpha:third', [2, 3, 4]), ('gamma:other', [0, 1, 4, 5]),
          ('rare:one', [0, 5]), ('wide:spread', [0, 1, 2, 3, 4, 5])]
SCENARIOS = [
    dict(name='defaults', durations=[250, 2000, 6000], concurrence=6, recent=20, quick=10),
    dict(name='edited', durations=[50, 700, 60000], concurrence=40, recent=3, quick=2),
    dict(name='recent off', durations=[50, 50, 50], concurrence=99, recent=None, quick=None),
    dict(name='low threshold', durations=[300, 3000, 9000], concurrence=1, recent=5, quick=1),
]


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUITagSuggestions as S
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
    from hydrus.client.gui.media import ClientGUIMediaModalActions as A
    from hydrus.client.gui.panels.options import TagSuggestionsPanel as P
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:6]]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(local, [
        U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[i] for i in files})) for tag, files in CORPUS]))
    recent = [f'recent:{i:02}' for i in range(12)]
    for tag in recent:
        c.WriteSynchronous('push_recent_tags', local, [tag])
        time.sleep(0.02)
    c.new_options.SetNoneableString('suggested_tags_layout', 'columns')
    c.new_options.SetBoolean('show_related_tags', True)
    c.new_options.SetBoolean('show_file_lookup_script_tags', False)
    c.new_options.SetRelatedTagsTagSliceWeights([['', 100], [':', 100]], [['', 100], [':', 100]])
    medias = [ClientMediaSingle.MediaSingle(r) for r in c.Read('media_results', hashes[:2])]

    def work():
        old_thread, old_after, old_read, old_exec = c.CallToThread, c.CallAfterQtSafe, c.Read, W.DialogEdit.exec
        c.CallToThread = lambda f, *a, **kw: f(*a, **kw) if getattr(f, '__name__', '') == 'do_it' else old_thread(f, *a, **kw)
        c.CallAfterQtSafe = lambda w, f, *a, **kw: f(*a, **kw) if getattr(f, '__name__', '') == 'qt_code' else old_after(w, f, *a, **kw)
        reads = []

        def read(name, *a, **kw):
            if name == 'related_tags':
                reads.append(dict(search_tags=sorted(a[2]), max_time_to_take=kw.get('max_time_to_take')))
            return old_read(name, *a, **kw)
        c.Read = read
        quick_seen = {}

        def scripted(dlg):
            dlg.setModal(True)
            quick_seen['suggestions'] = [b.text() for b in dlg._panel.widget().findChildren(QW.QPushButton)]
            quick_seen['text'] = dlg._panel._text.text()
            dlg._cancel.click()
            return dlg.result()
        W.DialogEdit.exec = scripted
        command = CAC.ApplicationCommand(CAC.APPLICATION_COMMAND_TYPE_INTERACTIVE_CONTENT, (local, HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_SET))
        out = []
        try:
            for scenario in SCENARIOS:
                page = P.TagSuggestionsPanel(c.gui, c.new_options)
                for i, ms in enumerate(scenario['durations']):
                    getattr(page, f'_related_tags_search_{i + 1}_duration_ms').setValue(ms)
                page._related_tags_concurrence_threshold_percent.setValue(scenario['concurrence'])
                page._num_recent_tags.SetValue(scenario['recent'])
                page._num_recent_tags_in_quick_dialog.SetValue(scenario['quick'])
                page.UpdateOptions()
                stored = dict(durations=[c.new_options.GetInteger(f'related_tags_search_{i}_duration_ms') for i in (1, 2, 3)],
                              concurrence=c.new_options.GetInteger('related_tags_concurrence_threshold_percent'),
                              recent=c.new_options.GetNoneableInteger('num_recent_tags'),
                              quick=c.new_options.GetNoneableInteger('num_recent_tags_in_quick_dialog'))
                page.deleteLater()
                panel = S.SuggestedTagsPanel(c.gui, local, CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, False, lambda tags, **kw: None)
                panel.SetMedia(medias)
                panel.resize(900, 500)
                panel.show()
                for _ in range(4):
                    QW.QApplication.processEvents()
                row = dict(name=scenario['name'], set=scenario, stored=stored,
                           recent_shown=panel._recent_tags is not None,
                           recent=[t.GetTag() for t in panel._recent_tags._recent_tags._ordered_terms] if panel._recent_tags is not None else None,
                           related={})
                for level, press in [('quick', 'RefreshQuick'), ('medium', 'RefreshMedium'), ('thorough', 'RefreshThorough')]:
                    reads.clear()
                    getattr(panel._related_tags, press)()
                    for _ in range(4):
                        QW.QApplication.processEvents()
                    row['related'][level] = dict(read=reads[-1] if reads else None,
                                                 listed=[t.GetTag() for t in panel._related_tags._related_tags._ordered_terms])
                quick_seen.clear()
                A.GetContentApplicationCommandFromInteractiveContentCommand(c.gui, command, medias[0])
                row['quick_dialog'] = dict(quick_seen)
                out.append(row)
                panel.hide()
                panel.deleteLater()
        finally:
            c.CallToThread, c.CallAfterQtSafe, c.Read, W.DialogEdit.exec = old_thread, old_after, old_read, old_exec
        return dict(service='my tags', files=[h.hex() for h in hashes], selected=[h.hex() for h in hashes[:2]], corpus=CORPUS,
                    pushed=recent, scenarios=out)
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
