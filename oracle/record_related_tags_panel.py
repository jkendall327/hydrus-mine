#!/usr/bin/env python3
"""Record the suggested tags' related panel: its buttons, status and searches.

The real `RelatedTagsPanel` over a small corpus written to the private basic
fixture. Its worker and Qt publisher (`do_it`, `qt_code`) are forwarded
synchronously; the DB's `related_tags` read is wrapped only to record the
arguments the panel passes (search tags, the time budget, the tags excluded).
Records the quick/medium/thorough buttons, the two on/off buttons' labels and
tooltips, the status line (without its timing, which varies), the listed
suggestions, a search from selected tags, a search tag with no count (skipped)
and a budget too small to finish one tag.
"""
import json, sys, tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.gui import ClientGUITagSuggestions as S
    from hydrus.client.media import ClientMediaSingle
    from hydrus.core import HydrusConstants as HC
    from qtpy import QtWidgets as QW
    c = session.controller
    service = next(s for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'second tags')
    manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:6]]
    corpus = [('source:seed', [0, 1, 2, 3]), ('context:seed', [2, 3]),
              ('alpha:first', [0, 1, 2]), ('beta:second', [3, 4, 5]),
              ('alpha:third', [2, 3, 4]), ('gamma:other', [0, 1, 4, 5])]
    c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(service.GetServiceKey(), [
        U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[i] for i in files})) for tag, files in corpus]))
    # The selected files: the first two.
    selected = hashes[:2]
    weights = ([['', 100], [':', 100]], [['', 100], [':', 100]])
    def drive():
        c.new_options.SetRelatedTagsTagSliceWeights(*weights)
        c.new_options.SetInteger('related_tags_concurrence_threshold_percent', 6)
        for i, ms in enumerate([250, 2000, 6000]):
            c.new_options.SetInteger(f'related_tags_search_{i + 1}_duration_ms', ms)
        medias = [ClientMediaSingle.MediaSingle(m) for m in c.Read('media_results', hashes=selected)]
        old_thread, old_after, old_read = c.CallToThread, c.CallAfterQtSafe, c.Read
        c.CallToThread = lambda f, *a, **kw: f(*a, **kw) if getattr(f, '__name__', '') == 'do_it' else old_thread(f, *a, **kw)
        c.CallAfterQtSafe = lambda w, f, *a, **kw: f(*a, **kw) if getattr(f, '__name__', '') == 'qt_code' else old_after(w, f, *a, **kw)
        reads = []
        def read(name, *a, **kw):
            if name == 'related_tags':
                reads.append(dict(search_tags=sorted(a[2]), max_time_to_take=kw.get('max_time_to_take'),
                                  other_tags_to_exclude=None if kw.get('other_tags_to_exclude') is None else sorted(kw['other_tags_to_exclude']),
                                  display=kw.get('tag_display_type'), file_service=a[0].hex()))
            return old_read(name, *a, **kw)
        c.Read = read
        panel = S.RelatedTagsPanel(c.gui, service.GetServiceKey(), lambda tags, **kw: None)
        steps = []
        def state(action):
            label = panel._status_label.text()
            steps.append(dict(action=action, status=label.split(' in ')[0] if ' in ' in label else label,
                              status_visible=not panel._status_label.isHidden(),
                              listed=[t.GetTag() for t in panel._related_tags._ordered_terms],
                              read=reads[-1] if reads else None,
                              local_label=panel._just_do_local_files.text(), display_label=panel._tag_display_type.text()))
            reads.clear()
        try:
            buttons = {b.text(): b.toolTip() for b in panel.findChildren(QW.QPushButton) if b.text() in ('quick', 'medium', 'thorough')}
            toggles = dict(local=panel._just_do_local_files.toolTip(), display=panel._tag_display_type.toolTip())
            state('initial')
            panel.SetMedia(medias)
            state('set media')
            panel.RefreshQuick(); state('quick')
            panel.RefreshMedium(); state('medium')
            panel.RefreshThorough(); state('thorough')
            panel.SetSelectedTags({'alpha:first'}); panel.RefreshQuick(); state('selected alpha:first')
            panel.SetSelectedTags({'alpha:first', 'missing:tag'}); panel.RefreshQuick(); state('selected with a tag of no count')
            panel.SetSelectedTags(set()); panel.RefreshQuick(); state('selection cleared')
            panel._just_do_local_files.SetOnOff(False); panel.RefreshQuick(); state('all known files')
            panel._just_do_local_files.SetOnOff(True)
            panel._tag_display_type.SetOnOff(False); panel.RefreshQuick(); state('storage tags')
            panel._tag_display_type.SetOnOff(True)
            c.new_options.SetInteger('related_tags_search_1_duration_ms', 0)
            panel.RefreshQuick(); state('no time at all')
            c.new_options.SetInteger('related_tags_search_1_duration_ms', 250)
            panel.SetMedia([])
            panel.RefreshQuick(); state('no media')
        finally:
            c.CallToThread, c.CallAfterQtSafe, c.Read = old_thread, old_after, old_read
            panel.hide(); panel.deleteLater()
        return dict(service=service.GetName(), files=[h.hex() for h in hashes], selected=[h.hex() for h in selected],
                    corpus=corpus, weights=weights, durations_ms=[250, 2000, 6000], buttons=buttons, toggles=toggles, steps=steps)
    return c.CallBlockingToQt(c.gui, drive)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        Path(sys.argv[2]).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)))
        return
    with tempfile.TemporaryDirectory() as temp:
        out = Path(temp) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(out))
        result = json.loads(out.read_text())
    (HERE / 'fixtures/related_tags_panel.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
    print('wrote related_tags_panel.json')
if __name__ == '__main__': main()
