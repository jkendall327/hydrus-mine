#!/usr/bin/env python3
"""Record live Qt banner previews for ASCII/Unicode decimal sort-key boundaries.

The reference splits ASCII digit runs, then interprets every complete decimal
chunk (including non-ASCII scripts) as an integer. Empty text chunks compare
as ('',0), so a Unicode zero and empty chunk have the same first key tuple.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CASES = [
    ['2', '10', '１２', '１３', '１４', 'alpha'],
    ['999', '２', '１０', 'alpha'],
    ['999', '２', '１０'],
    ['０', '000', '2', '００１', 'alpha'],
    ['١٢', '१३', '１４', '12', 'alpha'],
    ['１２3', '1２', '１２', '2', 'alpha'],
    ['page１２', 'page10', 'page2', 'alpha', '１２'],
    ['２', '１', '１０'],
]
def record(session):
    def qt():
        from hydrus.client.gui.metadata import ClientGUITagSummaryGenerator as summary
        from hydrus.core import HydrusTags, HydrusText
        rows = []
        for subtags in CASES:
            tags = ['page:' + tag for tag in subtags]
            generator = summary.TagSummaryGenerator(namespace_info=[('page', 'p=', '..')], example_tags=tags)
            panel = summary.EditTagSummaryGeneratorPanel(session.controller.gui, generator)
            panel._example_tags.setPlainText('\n'.join(tags))
            panel._UpdateTest()
            rows.append({'subtags':subtags, 'sorted':HydrusTags.SortNumericTags(subtags), 'preview':panel._test_result.text(), 'summary':panel.GetValue().GenerateSummary(tags)})
            panel.deleteLater()
        equality = [('０', ''), ('００', '٠'), ('１２', '١٢'), ('1２', '12')]
        return {'cases':rows,'key_equalities':[{'left':a,'right':b,'equal':HydrusText.HumanTextSortKey(a)==HydrusText.HumanTextSortKey(b)} for a,b in equality]}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)
def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as handle:
            json.dump(result, handle)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as handle:
            result = json.load(handle)
    output = os.path.join(HERE, 'fixtures/tag_banner_sort_boundaries.json')
    with open(output, 'w') as handle:
        json.dump(result, handle, indent=2)
        handle.write('\n')
    print('wrote ' + output)
if __name__ == '__main__':
    main()
