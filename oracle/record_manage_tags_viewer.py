#!/usr/bin/env python3
"""Real ManageTagsPanel launched as the media viewer launches it.

`immediate_commit = True` and a `canvas_key`: every entry is written to the
database at once, the panel follows `canvas_new_display_media` for its own
canvas only, and showNext / showPrevious publish `canvas_show_next` /
`canvas_show_previous`. Three synthetic files; the panel's own list rows and
the database's tags are recorded after each step.
"""
import json, os, sys, tempfile, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'manage_tags_viewer.json')


def record(session):
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    from hydrus.client.gui import ClientGUIDialogsQuick
    from qtpy import QtWidgets as QW
    c = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:3]]
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    corpus = [('v:a', [0]), ('v:b', [1]), ('v:c', [2]), ('v:shared', [0, 1])]
    for tag, files in corpus:
        c.WriteSynchronous('content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates(
            local, [U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, (tag, {hashes[i] for i in files}))]))

    def results():
        by_hash = {r.GetHash(): r for r in c.Read('media_results', hashes)}
        return [by_hash[h] for h in hashes]

    def stored():
        return [sorted(r.GetTagsManager().GetCurrent(local, 0)) for r in results()]

    def single(i):
        return ClientMediaSingle.MediaSingle(results()[i])

    def work():
        c.new_options.SetKey('default_tag_service_tab', local)
        c.new_options.SetBoolean('yes_no_on_remove_on_manage_tags', True)
        asked, published = [], []
        ClientGUIDialogsQuick.GetYesNo = lambda win, message, **kw: (asked.append(message), QW.QDialog.DialogCode.Accepted)[1]
        old_pub = c.pub
        c.pub = lambda topic, *a, **kw: published.append([topic] + [x.hex() if isinstance(x, bytes) else x for x in a]) if topic in ('canvas_show_next', 'canvas_show_previous') else old_pub(topic, *a, **kw)
        key = b'canvas-one'
        p = ManageTagsPanel(c.gui, ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), CC.TAG_PRESENTATION_MEDIA_VIEWER_MANAGE_TAGS, [single(0)], immediate_commit=True, canvas_key=key)
        page = next(p._tag_services.widget(i) for i in range(p._tag_services.count()) if p._tag_services.widget(i).GetServiceKey() == local)

        def pump():
            for _ in range(30):
                QW.QApplication.processEvents()
                time.sleep(0.02)

        def rows():
            box = page._tags_box
            return [''.join(text for text, _ in box._GetRowsOfTextsAndColours(term)[0]) for term in box._ordered_terms if term.GetTag().startswith('v:')]

        def state(step, **extra):
            pump()
            result = {'step': step, 'rows': rows(), 'stored': [[t for t in tags if t.startswith('v:')] for tags in stored()],
                      'has_changes': page.HasChanges(), 'packages': len(p._GetContentUpdatePackages()), 'asked': list(asked), 'published': list(published)}
            result.update(extra)
            asked.clear(); published.clear()
            return result

        steps = [state('opened_on_first')]
        c.new_options.SetBoolean('allow_remove_on_manage_tags_input', False)
        page.AddTags({'v:new'})
        steps.append(state('typed_add_commits_at_once'))
        p.CanvasHasNewMedia(key, single(1))
        steps.append(state('follows_to_second'))
        page.AddTags({'v:shared'})
        steps.append(state('typed_existing_tag_does_nothing_by_default'))
        c.new_options.SetBoolean('allow_remove_on_manage_tags_input', True)
        page.AddTags({'v:shared'})
        steps.append(state('typed_existing_tag_removes_when_allowed'))
        page.RemoveTags(['v:b'])
        steps.append(state('remove_confirms_then_commits'))
        p.CanvasHasNewMedia(b'another-canvas', single(2))
        steps.append(state('other_canvas_is_ignored'))
        p.CanvasHasNewMedia(key, None)
        steps.append(state('no_media_is_ignored'))
        p.CanvasHasNewMedia(key, single(2))
        steps.append(state('follows_to_third'))
        p.ShowNext(); p.ShowPrevious()
        steps.append(state('show_next_and_previous_publish'))
        ok_to_cancel = p.UserIsOKToCancel()
        steps.append(state('ok_to_cancel_asks_nothing', ok_to_cancel=ok_to_cancel))
        p.deleteLater()
        c.pub = old_pub
        return {'files': [h.hex() for h in hashes], 'corpus': corpus, 'canvas_key': key.hex(), 'steps': steps}
    return c.CallBlockingToQt(c.gui, work)


def child(out):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2]); return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
