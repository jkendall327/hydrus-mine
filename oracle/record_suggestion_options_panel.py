#!/usr/bin/env python3
"""Record Options > tag suggestions' layout, default page and width rows.

Fresh temporary `basic` client. The real options page (`TagSuggestionsPanel`)
is built; for each layout the default-page choice's enabled state is recorded
(it follows the layout), the choice labels and the spin box's range. Applied
values are read back from `new_options`. The real `SuggestedTagsPanel` is then
built at each recorded width, in each layout, with most used, related and
recent on, and the minimum width of each list is recorded (the width option
sets each list's minimum width).
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'suggestion_options_panel.json')
WIDTHS = [20, 240, 500]


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUITagSuggestions as S
    from hydrus.client.gui.panels.options import TagSuggestionsPanel as P
    from hydrus.client.media import ClientMediaSingle
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:2]]
    c.WriteSynchronous('push_recent_tags', local, ['parity:recent'])
    c.new_options.SetSuggestedTagsMostUsed(local, ['parity:one'])
    c.new_options.SetBoolean('show_related_tags', True)
    c.new_options.SetBoolean('show_file_lookup_script_tags', False)
    c.new_options.SetNoneableInteger('num_recent_tags', 20)
    medias = [ClientMediaSingle.MediaSingle(r) for r in c.Read('media_results', hashes)]

    def work():
        out = {}
        panel = P.TagSuggestionsPanel(c.gui, c.new_options)
        layout, default, width = panel._suggested_tags_layout, panel._default_suggested_tags_notebook_page, panel._suggested_tags_width
        out['initial'] = dict(width=width.value(), minimum=width.minimum(), maximum=width.maximum(), layout=layout.GetValue(), default=default.GetValue(), default_enabled=default.isEnabled(),
                              layout_labels=[layout.itemText(i) for i in range(layout.count())], default_labels=[default.itemText(i) for i in range(default.count())],
                              default_values=[default.itemData(i) for i in range(default.count())])
        steps = []
        for value in ['columns', 'notebook', 'columns']:
            layout.SetValue(value)
            steps.append(dict(layout=value, default_enabled=default.isEnabled()))
        out['layout_steps'] = steps
        applied = []
        for (value, page, w) in [('notebook', 'recent', 240), ('columns', 'related', 500), ('notebook', 'file_lookup_scripts', 20)]:
            layout.SetValue(value)
            default.SetValue(page)
            width.setValue(w)
            panel.UpdateOptions()
            applied.append(dict(layout=value, default=page, width=w, saved_layout=c.new_options.GetNoneableString('suggested_tags_layout'),
                                saved_default=c.new_options.GetString('default_suggested_tags_notebook_page'), saved_width=c.new_options.GetInteger('suggested_tags_width')))
        out['applied'] = applied
        panel.deleteLater()
        widths = []
        for layout_name in ['notebook', 'columns']:
            for w in WIDTHS:
                c.new_options.SetNoneableString('suggested_tags_layout', layout_name)
                c.new_options.SetString('default_suggested_tags_notebook_page', 'favourites')
                c.new_options.SetInteger('suggested_tags_width', w)
                s = S.SuggestedTagsPanel(c.gui, local, CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, False, lambda tags, **kw: None)
                s.SetMedia(medias)
                s.resize(1400, 500)
                s.show()
                for _ in range(4):
                    QW.QApplication.processEvents()
                lists = dict(favourites=s._favourite_tags._favourite_tags, related=s._related_tags._related_tags, recent=s._recent_tags._recent_tags)
                wide = {k: v.width() for k, v in lists.items()}
                # narrowed as far as the layout lets it, the lists have the width the option gave them
                s.resize(1, 500)
                for _ in range(4):
                    QW.QApplication.processEvents()
                narrow = {k: v.width() for k, v in lists.items()}
                widths.append(dict(layout=layout_name, width=w, minimum_widths={k: v.minimumWidth() for k, v in lists.items()},
                                   wide_widths=wide, narrow_widths=narrow, narrow_panel=[s.width(), s.height()]))
                s.hide()
                s.deleteLater()
        out['widths'] = widths
        return out
    return c.CallBlockingToQt(c.gui, work)


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
