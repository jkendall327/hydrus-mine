#!/usr/bin/env python3
"""Record the reference's auto-resolution rule and comparator exports.

On the `basic` fixture, the real `EditDuplicatesAutoResolutionRulesPanel` is
given rules and its `_ExportToClipboard` run on selections of them; the
clipboard text is what the reference exports (`DumpToString`, the same text
`to json file` writes, and the payload `to png file` encodes). The rules:

- the eight suggested rules (`GetDefaultRuleSuggestions`), each alone and
  all together (a `SerialisableList`);
- `wide`: a rule built as the reference's editors would build it, with
  system predicates parsed from text by the reference's own parser
  (`ParseSystemPredicateStringsToPredicates`) and tag, namespace, wildcard
  and OR predicates, every comparator type (OR and AND nested), paused,
  fully automatic, both deletes and custom metadata merge options;
- `searches`: the other dupe search types and pixel preferences, and a
  location context of two domains and a deleted one.

The rule editor's comparator list (`EditComparatorList`, a list box) is
exported too: one comparator, and two (a list). A rule and a comparator are
exported to png files (`PNGExportPanel`, its defaults), kept beside the
fixture, with the text each carries.

Then pasting and duplicating in the rules list, the rule editor's comparator
list and an OR's and AND's: the messages, what each list holds and selects.

Each export is written as the text the reference produced. The rules are
given ids 1, 2, ... as saved rules have their database ids (a rule's id is
replaced by `_ImportRule` on every import).

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_rules_exchange.py
       (writes fixtures/rules_exchange.json)
"""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

OUT = os.path.join(HERE, 'fixtures', 'rules_exchange.json')
# the pngs the reference exports: the rule "wide", and a comparator
PNGS = ['rules_exchange_rule.png', 'rules_exchange_comparator.png']

# system predicates, as a user types them; the reference's parser makes them
SYSTEM = [
    'system:inbox',
    'system:archive',
    'system:width > 1000',
    'system:height = 600',
    'system:height ≈ 600',
    'system:width ≠ 600',
    'system:duration < 5 seconds',
    'system:framerate = 30fps',
    'system:number of frames > 10',
    'system:number of words < 100',
    'system:number of notes = 2',
    'system:number of urls > 0',
    'system:filesize > 2MB',
    'system:filesize < 200 kilobytes',
    'system:filetype = image/jpg, image/png',
    'system:filetype is not video',
    'system:ratio = 16:9',
    'system:ratio wider than 1:1',
    'system:num pixels > 5 megapixels',
    'system:import time < 7 days',
    'system:modified date > 2020-01-01',
    'system:last viewed time < 3 days 4 hours',
    'system:archived time > 1 year',
    'system:has audio',
    'system:no audio',
    'system:no icc profile',
    'system:has exif',
    'system:has human-readable embedded metadata',
    'system:has transparency',
    'system:has forced filetype',
    'system:number of tags > 5',
    'system:number of character tags = 0',
    'system:has tags',
    'system:untagged',
    'system:limit = 100',
    'system:hash = abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789',
    'system:hash (md5) = 0123456789abcdef0123456789abcdef',
    'system:has url matching regex index\\.php',
    'system:has domain example.com',
    'system:does not have url https://example.com/a',
    'system:has note with name "comment"',
    'system:does not have note with name note',
    'system:tag as number page < 10',
    'system:views in media > 5',
    'system:viewtime in media, preview < 1 hour',
    'system:num file relationships > 1 duplicates',
    'system:similar to abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789 with distance 4',
    'system:file service currently in my files',
    'system:file service is not pending to my files',
    'system:is the best quality file of its group',
    'system:is not the best quality file of its group',
    'system:everything',
    'system:has xmp',
    'system:has_iptc',
    'system:has software/source metadata',
    'system:rating for favourites = like',
    'system:rating for favourites = dislike',
    'system:has rating stars',
    'system:rating for stars = 3/5',
    'system:rating for stars > 0/5',
    'system:similar to data b51d75120456d6e2155416f26416a96290b0a524bf1582af50b4fcf46dedcd91 b51d75120456d6e2155416f26416a96290b0a524bf1582af50b4fcf46dedcd92 distance 4',
    'system:no rating for counter',
    'system:rating for counter > 3',
    'system:all ratings rated',
    'system:any ratings not rated',
    'system:has tag: "skirt"',
    'system:has tag in "my tags", ignoring siblings/parents, status current, pending: "skirt"',
    'system:does not have tag "all known tags", status deleted: "filename:blarg"',
    'system:num file relationships < 3 alternates',
    'system:media views ~= 10',
]


def record(session):
    c = session.controller
    gui = c.gui

    def f():
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client import ClientLocation
        from hydrus.client.duplicates import ClientDuplicates
        from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR
        from hydrus.client.duplicates import ClientDuplicatesAutoResolutionComparators as C
        from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext as P
        from hydrus.client.files.images import ClientVisualData
        from hydrus.client.gui.duplicates import ClientGUIDuplicatesAutoResolution as G
        from hydrus.client.metadata import ClientMetadataConditional
        from hydrus.client.search import ClientNumberTest
        from hydrus.client.search import ClientSearchFileSearchContext as FSC
        from hydrus.client.search import ClientSearchParseSystemPredicates
        from hydrus.client.search import ClientSearchPredicate as SP
        from hydrus.client.search import ClientSearchTagContext

        clipboard = [None]
        original_pub = c.pub

        def pub(topic, *args, **kwargs):
            if topic == 'clipboard' and args[0] == 'text':
                clipboard[0] = args[1]
                return
            return original_pub(topic, *args, **kwargs)

        c.pub = pub

        out = {'services': [], 'refused': [], 'exports': [], 'comparators': []}
        for service in c.services_manager.GetServices():
            out['services'].append({'key': service.GetServiceKey().hex(), 'name': service.GetName(), 'type': service.GetServiceType()})

        parsed = []
        for text in SYSTEM:
            try:
                (predicate,) = ClientSearchParseSystemPredicates.ParseSystemPredicateStringsToPredicates([text])
                parsed.append(predicate)
            except Exception as e:
                out['refused'].append({'text': text, 'error': repr(e)})

        tags = [
            SP.Predicate(SP.PREDICATE_TYPE_TAG, 'blue eyes'),
            SP.Predicate(SP.PREDICATE_TYPE_TAG, 'solo', inclusive=False),
            SP.Predicate(SP.PREDICATE_TYPE_TAG, 'character:samus aran'),
            SP.Predicate(SP.PREDICATE_TYPE_NAMESPACE, 'series'),
            SP.Predicate(SP.PREDICATE_TYPE_NAMESPACE, 'creator', inclusive=False),
            SP.Predicate(SP.PREDICATE_TYPE_WILDCARD, 'blue*'),
            SP.Predicate(SP.PREDICATE_TYPE_WILDCARD, 'title:*ing', inclusive=False),
            SP.Predicate(SP.PREDICATE_TYPE_OR_CONTAINER, [
                SP.Predicate(SP.PREDICATE_TYPE_TAG, 'red'),
                SP.Predicate(SP.PREDICATE_TYPE_TAG, 'green', inclusive=False),
                SP.Predicate(SP.PREDICATE_TYPE_SYSTEM_INBOX),
            ]),
            SP.Predicate(SP.PREDICATE_TYPE_TAG, 'ünicode タグ "quoted"'),
        ]

        local = ClientLocation.LocationContext.STATICCreateSimple(CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY)

        def search(predicates, location=local, tag_service=CC.COMBINED_TAG_SERVICE_KEY):
            return FSC.FileSearchContext(location_context=location, tag_context=ClientSearchTagContext.TagContext(service_key=tag_service), predicates=predicates)

        def conditional(looking_at, predicates):
            comparator = C.PairComparatorOneFileMetadataConditional()
            comparator.SetLookingAt(looking_at)
            mc = ClientMetadataConditional.MetadataConditional()
            mc.SetFileSearchContext(FSC.FileSearchContext(predicates=predicates))
            comparator.SetMetadataConditional(mc)
            return comparator

        def relative(predicate_type, operator, multiplier, delta, extra=None):
            comparator = C.PairComparatorRelativeFileInfo()
            comparator.SetSystemPredicate(SP.Predicate(predicate_type))
            test = ClientNumberTest.NumberTest(operator=operator)
            if extra is not None:
                test = ClientNumberTest.NumberTest(operator=operator, extra_value=extra)
            comparator.SetNumberTest(test)
            comparator.SetMultiplier(multiplier)
            comparator.SetDelta(delta)
            return comparator

        def hardcoded(looking_at, kind):
            comparator = C.PairComparatorOneFileHardcoded()
            comparator.SetLookingAt(looking_at)
            comparator.SetComparatorType(kind)
            return comparator

        comparators = [
            conditional(C.LOOKING_AT_B, parsed[: len(parsed) // 2]),
            conditional(C.LOOKING_AT_EITHER, tags),
            hardcoded(C.LOOKING_AT_EITHER, C.HARDCODED_COMPARATOR_TYPE_ONE_FILE_JPEG_IS_NOT_PROGRESSIVE),
            hardcoded(C.LOOKING_AT_A, C.HARDCODED_COMPARATOR_TYPE_ONE_FILE_JPEG_IS_PROGRESSIVE),
            relative(SP.PREDICATE_TYPE_SYSTEM_WIDTH, ClientNumberTest.NUMBER_TEST_OPERATOR_GREATER_THAN, 2.5, 10),
            relative(SP.PREDICATE_TYPE_SYSTEM_SIZE, ClientNumberTest.NUMBER_TEST_OPERATOR_APPROXIMATE_PERCENT, 1.0, 0, 0.1),
            relative(SP.PREDICATE_TYPE_SYSTEM_NUM_PIXELS, ClientNumberTest.NUMBER_TEST_OPERATOR_APPROXIMATE_ABSOLUTE, 0.5, -3, 20),
            relative(SP.PREDICATE_TYPE_SYSTEM_DURATION, ClientNumberTest.NUMBER_TEST_OPERATOR_LESS_THAN_OR_EQUAL_TO, 1.0, 0),
            C.PairComparatorRelativeVisualDuplicates(acceptable_confidence=ClientVisualData.VISUAL_DUPLICATES_RESULT_PROBABLY),
            C.PairComparatorOR([
                C.PairComparatorRelativeHardcoded(C.HARDCODED_COMPARATOR_TYPE_TWO_FILES_HAS_EXIF_SAME),
                C.PairComparatorAND([
                    C.PairComparatorRelativeVisualDuplicates(acceptable_confidence=ClientVisualData.VISUAL_DUPLICATES_RESULT_VERY_PROBABLY),
                    C.PairComparatorRelativeHardcoded(C.HARDCODED_COMPARATOR_TYPE_TWO_FILES_HAS_ICC_PROFILE_SAME),
                ]),
            ]),
            C.PairComparatorRelativeHardcoded(C.HARDCODED_COMPARATOR_TYPE_TWO_FILES_FILETYPE_DIFFERS),
        ]

        def rule(name, search_1, search_2, dupe_type, pixels, distance, comparators, action, paused, mode, max_pending, delete, merge):
            r = AR.DuplicatesAutoResolutionRule(name)
            p = P.PotentialDuplicatesSearchContext()
            p.SetFileSearchContext1(search_1)
            p.SetFileSearchContext2(search_2)
            p.SetDupeSearchType(dupe_type)
            p.SetPixelDupesPreference(pixels)
            p.SetMaxHammingDistance(distance)
            r.SetPotentialDuplicatesSearchContext(p)
            selector = C.PairSelector()
            selector.SetComparators(comparators)
            r.SetPairSelector(selector)
            r.SetAction(action)
            r.SetPaused(paused)
            r.SetOperationMode(mode)
            r.SetMaxPendingPairs(max_pending)
            r.SetDeleteInfo(*delete)
            r.SetDuplicateContentMergeOptions(merge)
            return r

        merge = c.new_options.GetDuplicateContentMergeOptions(HC.DUPLICATE_BETTER).Duplicate()
        merge.SetSyncArchiveAction(ClientDuplicates.SYNC_ARCHIVE_DO_BOTH_REGARDLESS)
        merge.SetSyncURLsAction(HC.CONTENT_MERGE_ACTION_TWO_WAY_MERGE)

        my_files = c.services_manager.GetServiceKeys((HC.LOCAL_FILE_DOMAIN,))
        wide = rule('wide', search(parsed[len(parsed) // 2:] + tags), search(tags[:3]), ClientDuplicates.DUPE_SEARCH_BOTH_FILES_MATCH_DIFFERENT_SEARCHES,
                    ClientDuplicates.SIMILAR_FILES_PIXEL_DUPES_EXCLUDED, 8, comparators, HC.DUPLICATE_ALTERNATE, True,
                    AR.DUPLICATES_AUTO_RESOLUTION_RULE_OPERATION_MODE_FULLY_AUTOMATIC, None, (True, True), merge)
        locations = ClientLocation.LocationContext(current_service_keys=[CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY], deleted_service_keys=[CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY])
        searches = [
            rule('one search', search([tags[0]], location=ClientLocation.LocationContext.STATICCreateSimple(list(my_files)[0]), tag_service=CC.DEFAULT_LOCAL_TAG_SERVICE_KEY), search([]),
                 ClientDuplicates.DUPE_SEARCH_ONE_FILE_MATCHES_ONE_SEARCH, ClientDuplicates.SIMILAR_FILES_PIXEL_DUPES_ALLOWED, 4, [comparators[2]], HC.DUPLICATE_SAME_QUALITY,
                 False, AR.DUPLICATES_AUTO_RESOLUTION_RULE_OPERATION_MODE_WORK_BUT_NO_ACTION, 77, (False, True), None),
            rule('both one search', search([], location=locations), search([]), ClientDuplicates.DUPE_SEARCH_BOTH_FILES_MATCH_ONE_SEARCH,
                 ClientDuplicates.SIMILAR_FILES_PIXEL_DUPES_REQUIRED, 0, [], HC.DUPLICATE_FALSE_POSITIVE,
                 False, AR.DUPLICATES_AUTO_RESOLUTION_RULE_OPERATION_MODE_WORK_BUT_NO_ACTION, 500, (False, False), None),
            rule('worse', search([]), search([]), ClientDuplicates.DUPE_SEARCH_ONE_FILE_MATCHES_ONE_SEARCH,
                 ClientDuplicates.SIMILAR_FILES_PIXEL_DUPES_ALLOWED, 2, [comparators[4]], HC.DUPLICATE_WORSE,
                 False, AR.DUPLICATES_AUTO_RESOLUTION_RULE_OPERATION_MODE_WORK_BUT_NO_ACTION, 1, (True, False), None),
        ]

        suggested = AR.GetDefaultRuleSuggestions()
        rules = suggested + [wide] + searches
        # as saved rules have: their database ids (some suggestions are
        # duplicates of others, sharing an id the list would merge)
        for (i, r) in enumerate(rules):
            r.SetId(i + 1)
        panel = G.EditDuplicatesAutoResolutionRulesPanel(gui, rules)
        listctrl = panel._duplicates_auto_resolution_rules_panel
        view = listctrl._listctrl

        def select(names):
            view.SelectDatas([r for r in view.GetData() if r.GetName() in names], deselect_others=True)

        def export(names):
            select(names)
            clipboard[0] = None
            listctrl._ExportToClipboard()
            selected = [r.GetName() for r in view.GetData(only_selected=True)]
            out['exports'].append({'names': names, 'selected_in_order': selected, 'text': clipboard[0]})

        for r in rules:
            export([r.GetName()])
        export([r.GetName() for r in suggested])
        export([r.GetName() for r in [wide] + searches])

        # the rule editor's comparator list
        editor = G.EditDuplicatesAutoResolutionRulePanel(gui, wide)
        box = editor._pair_selector._comparators
        listbox = box._listbox

        def select_rows(rows):
            for i in range(listbox.count()):
                listbox.item(i).setSelected(i in rows)

        for rows in [[0], [4], [9], [2, 5]]:
            select_rows(rows)
            clipboard[0] = None
            box._ExportToClipboard()
            out['comparators'].append({'rows': rows, 'text': clipboard[0]})
        out['comparator_rows'] = [listbox.item(i).text() for i in range(listbox.count())]

        # to png: the rules list's (a list control's) and the comparator
        # list's (a list box's) PNGExportPanel, exported to a file
        import shutil, tempfile
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientSerialisable
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels
        pngs = []

        class Dialog(QW.QDialog):
            def __init__(self, parent, title, *args, **kwargs):
                super().__init__(parent)
                self.title = title

            def __enter__(self):
                return self

            def __exit__(self, *args):
                return False

            def SetPanel(self, panel):
                self.panel = panel

            def exec(self):
                p = self.panel
                target = os.path.join(HERE, 'fixtures', PNGS[len(pngs)])
                with tempfile.TemporaryDirectory() as folder:
                    # (the panel's title starts as the payload's description)
                    title = p._title.text()
                    p._filepicker.SetPath(os.path.join(folder, 'export'))
                    p._Update()
                    p._export.click()
                    written = os.path.join(folder, 'export.png')
                    shutil.copy(written, target)
                    payload = ClientSerialisable.LoadFromPNG(written)
                    pngs.append({'dialog': self.title, 'file': PNGS[len(pngs)], 'title': title, 'payload': payload.hex()})
                return 0

        original_dialog = ClientGUITopLevelWindowsPanels.DialogNullipotent
        ClientGUITopLevelWindowsPanels.DialogNullipotent = Dialog
        panel = G.EditDuplicatesAutoResolutionRulesPanel(gui, rules)
        listctrl = panel._duplicates_auto_resolution_rules_panel
        listctrl._listctrl.SelectDatas([wide], deselect_others=True)
        listctrl._ExportToPNG()
        select_rows([2])
        box._ExportToPNG()
        ClientGUITopLevelWindowsPanels.DialogNullipotent = original_dialog
        from hydrus.core import HydrusCompression
        for png in pngs:
            png['text'] = HydrusCompression.DecompressBytesToString(bytes.fromhex(png.pop('payload')))
        out['pngs'] = pngs

        # importing and duplicating: messages, what the list holds and selects
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.core import HydrusExceptions, HydrusSerialisable
        messages = []
        ClientGUIDialogsMessage.ShowInformation = lambda parent, text, **kwargs: messages.append({'kind': 'information', 'text': text})
        ClientGUIDialogsMessage.ShowWarning = lambda parent, text, **kwargs: messages.append({'kind': 'warning', 'text': text})
        ClientGUIDialogsMessage.ShowCritical = lambda parent, title, text, **kwargs: messages.append({'kind': 'critical', 'title': title, 'text': text})

        def get_clipboard():
            if clipboard[0] is None:
                raise HydrusExceptions.DataMissing('No text on the clipboard!')
            return clipboard[0]

        c.GetClipboardText = get_clipboard
        # (the rules list looks for a png on the clipboard first)
        c.ClipboardHasImage = lambda: False

        def text_of(names):
            return [e for e in out['exports'] if e['names'] == names][0]['text']

        comparator_text = out['comparators'][0]['text']
        mixed = HydrusSerialisable.SerialisableList([HydrusSerialisable.CreateFromString(text_of(['worse'])),
                                                     HydrusSerialisable.CreateFromString(comparator_text)]).DumpToString()

        start = [suggested[0].Duplicate(), searches[2].Duplicate()]
        for (i, r) in enumerate(start):
            r.SetId(100 + i)
        panel = G.EditDuplicatesAutoResolutionRulesPanel(gui, start)
        listctrl = panel._duplicates_auto_resolution_rules_panel
        view = listctrl._listctrl

        def rules_state():
            return {'names': [r.GetName() for r in view.GetData()],
                    'selected': [r.GetName() for r in view.GetData(only_selected=True)]}

        out['rule_imports'] = {'start': rules_state(), 'steps': []}
        steps = [
            ('paste the suggested rules', 'paste', text_of([r.GetName() for r in suggested])),
            ('paste a comparator', 'paste', comparator_text),
            ('paste a rule and a comparator', 'paste', mixed),
            ('paste text that is not an object', 'paste', 'not a rule'),
            # (Qt gives '' for an empty clipboard, never the None that would be
            # GetClipboardText's DataMissing)
            ('paste an empty clipboard', 'paste', ''),
            ('duplicate the selected', 'duplicate', None),
        ]
        for (name, action, text) in steps:
            del messages[:]
            if action == 'paste':
                view.SelectDatas([], deselect_others=True)
                clipboard[0] = text
                listctrl._ImportFromClipboard()
            else:
                view.SelectDatas([r for r in view.GetData() if r.GetName() in ('worse', 'pixel-perfect pairs')], deselect_others=True)
                listctrl._Duplicate()
            out['rule_imports']['steps'].append({'name': name, 'action': action, 'text': text, 'messages': list(messages), **rules_state()})

        # the rule editor's comparator list, and an OR's
        def list_state(box):
            lb = box._listbox
            return {'rows': [lb.item(i).text() for i in range(lb.count())],
                    'selected': [i for i in range(lb.count()) if lb.item(i).isSelected()]}

        def run(box, steps):
            lb = box._listbox
            record = {'start': list_state(box), 'steps': []}
            for (name, action, rows, text) in steps:
                del messages[:]
                for i in range(lb.count()):
                    lb.item(i).setSelected(i in rows)
                clipboard[0] = text
                if action == 'paste':
                    box._ImportFromClipboard()
                elif action == 'duplicate':
                    box._Duplicate()
                else:
                    clipboard[0] = None
                    box._ExportToClipboard()
                # 'select': the rows selected first; 'rows' and 'selected': the list after
                record['steps'].append({'name': name, 'action': action, 'select': rows, 'text': clipboard[0] if action == 'export' else text,
                                        'messages': list(messages), **list_state(box)})
            return record

        two = [e for e in out['comparators'] if e['rows'] == [2, 5]][0]['text']
        common = [
            ('duplicate two', 'duplicate', [2, 5], None),
            ('paste two', 'paste', [], two),
            ('paste a rule', 'paste', [], text_of(['worse'])),
            ('paste a rule and a comparator', 'paste', [], mixed),
            ('paste text that is not an object', 'paste', [], 'not a comparator'),
            ('paste an empty clipboard', 'paste', [], ''),
        ]
        editor = G.EditDuplicatesAutoResolutionRulePanel(gui, wide.Duplicate())
        out['comparator_imports'] = run(editor._pair_selector._comparators, common)
        or_panel = G.EditPairComparatorOR(gui, comparators[9].Duplicate())
        out['or_list'] = run(or_panel._comparators, [
            ('export the first', 'export', [0], None),
            ('export both', 'export', [0, 1], None),
        ] + common[1:])
        and_panel = G.EditPairComparatorAND(gui, [sub for sub in comparators[9]._sub_comparators if isinstance(sub, C.PairComparatorAND)][0].Duplicate())
        out['and_list'] = run(and_panel._comparators, [
            ('export the second', 'export', [1], None),
            ('duplicate the first', 'duplicate', [0], None),
        ])
        return out

    return c.CallBlockingToQt(gui, f)


def main():
    import hydrus_driver, record_api, tempfile
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as fh:
            json.dump(result, fh, ensure_ascii=False)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as fh:
            result = json.load(fh)
    with open(OUT, 'w') as fh:
        json.dump(result, fh, indent=1, ensure_ascii=False)
        fh.write('\n')
    print(len(result['exports']), len(result['comparators']), 'refused:', result['refused'])


if __name__ == '__main__':
    main()
