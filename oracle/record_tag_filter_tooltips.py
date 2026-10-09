#!/usr/bin/env python3
"""Record the tooltips of the tag filter controls and the preview's volume control.

Builds the real `TagFilterButton` with each configuration a hydrus-rs window
uses (tag migration's three, tag display's, the string match's, an import's
"get tags" and blacklist, the Client API's permitted tags) over a few filters,
and records the button's text and tooltip. Records the tag filter editor's
"show other panels" tooltip, and the preview canvas's `VolumeControl` mute
tooltips. `WrapToolTip` is recorded over a few long texts as well.
"""
import json
import os
import shutil
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api
OUT = os.path.join(HERE, 'fixtures', 'tag_filter_tooltips.json')

def record(session):
    controller = session.controller
    def work():
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui.media import ClientGUIMediaControls
        from hydrus.client.gui.metadata import ClientGUITagFilter
        from hydrus.core import HydrusConstants as HC, HydrusTags
        def make(rules):
            f = HydrusTags.TagFilter()
            for slice_, rule in rules:
                f.SetRule(slice_, HC.FILTER_BLACKLIST if rule == 'black' else HC.FILTER_WHITELIST)
            return f
        filters = {
            'all': [],
            'blacklist': [('series:', 'black'), ('', 'black')],
            'whitelist': [('', 'black'), (':', 'black'), ('character:', 'white'), ('creator:', 'white')],
            'long': [('goblin', 'black'), ('a very long tag that goes on and on', 'black'), ('series:', 'black'), ('meta:', 'black')],
            'except': [('series:', 'black'), ('series:evangelion', 'white')],
        }
        configs = [
            ('migration_taken', dict(label_prefix='tags taken: ', use_filter_language=True)),
            ('migration_left', dict(label_prefix='left: ', use_filter_language=True)),
            ('migration_right', dict(label_prefix='right: ', use_filter_language=True)),
            ('display_shown', dict(label_prefix='tags shown: ', use_filter_language=True)),
            ('string_match', dict()),
            ('import_get_tags', dict(label_prefix='adding: ', use_filter_language=True)),
            ('import_blacklist', dict(only_show_blacklist=True)),
            ('api_permitted', dict(label_prefix='permitted tags: ')),
        ]
        buttons = []
        for config, kwargs in configs:
            for name, rules in filters.items():
                b = ClientGUITagFilter.TagFilterButton(controller.gui, 'message', make(rules), **kwargs)
                buttons.append({'config': config, 'filter': name, 'rules': [list(r) for r in rules], 'text': b.text(), 'tooltip': b.toolTip()})
                b.deleteLater()
        editor = ClientGUITagFilter.EditTagFilterPanel(controller.gui, make([]), only_show_blacklist=True, namespaces=[])
        show_other = editor._show_all_panels_button.toolTip()
        editor.deleteLater()
        volume = ClientGUIMediaControls.VolumeControl(controller.gui, CC.CANVAS_PREVIEW, direction='up')
        result = {
            'buttons': buttons,
            'show_other_panels': show_other,
            'global_mute': volume._global_mute.toolTip(),
            'preview_mute': volume._popup_window._specific_mute.toolTip(),
            'wrapped': [{'text': t, 'wrapped': ClientGUIFunctions.WrapToolTip(t)} for t in [
                'short',
                'This shows the whitelist and advanced panels, in case you want to craft a clever blacklist with \'except\' rules.',
                'two\n\nparagraphs with a fairly long second line that needs to be wrapped at around eighty characters wide',
                'averyveryveryveryveryveryveryveryveryveryveryveryveryveryveryveryveryveryverylongwordthatcannotbreak and more',
            ]],
        }
        volume.deleteLater()
        return result
    result = controller.CallBlockingToQt(controller.gui, work)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
        f.write('\n')

if __name__ == '__main__':
    db_dir = record_api.unpack_fixture('basic')
    try:
        run_client(db_dir, record)
    finally:
        shutil.rmtree(db_dir)
