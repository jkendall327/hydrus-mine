#!/usr/bin/env python3
"""Cross-roundtrip real reference downloader JSON/PNG through native encoders.

Requires --rust-executable pointing at the built exchange example. Boots the
reference client and calls its real PNG export/import functions on the Qt
thread. Records all formula kinds, page/subsidiary/content nodes, generators,
URL classes, auxiliary context, old versions and duplicate identity rules.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))


def record(session, executable, out):
    def qt():
        from hydrus.core import HydrusSerialisable as S
        from hydrus.client import ClientSerialisable as PNG
        from hydrus.client.networking import ClientNetworkingDomain as D
        # Use reference-built corpus definitions, covering every actual native
        # branch while keeping the exchange fixture small and reviewable.
        values = []
        for filename, field in [('url_classes', 'url_class'), ('gugs', 'gug'), ('formulas', 'formula'), ('page_parsers', 'parser')]:
            corpus = json.loads((HERE / 'fixtures' / (filename + '.json')).read_text())['cases']
            seen = set()
            for case in corpus:
                if field not in case:
                    field = 'page_parser' if filename == 'page_parsers' else field
                value = case[field]
                if value[0] in seen:
                    continue
                seen.add(value[0])
                values.append(S.CreateFromSerialisableTuple(value))
        # Ensure a nested generator, every content kind, subsidiary parser,
        # and all six formula kinds appear even if corpus cases are bundled.
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client.networking import ClientNetworkingGUG as G
        values += [P.ParseFormulaHTML(), P.ParseFormulaJSON(), P.ParseFormulaZipper(), P.ParseFormulaContextVariable(), P.ParseFormulaNested(), P.ParseFormulaStatic(static_text='exported 日本', num_to_do=2)]
        single = G.GalleryURLGenerator('exchange gallery', url_template='https://example.com/search?q=%tags%', replacement_phrase='%tags%')
        single.SetGUGKeyAndName((bytes.fromhex('11' * 32), 'exchange gallery'))
        values += [single, G.NestedGalleryURLGenerator('exchange nested', gug_keys_and_names=[single.GetGUGKeyAndName()])]
        content = json.loads((HERE / 'fixtures' / 'parser_editors.json').read_text())
        contents = [S.CreateFromSerialisableTuple(c['tuple']) for c in content if c['case'] == 'content']
        child = P.PageParser(name='child', content_parsers=contents[:2], example_parsing_context={'url': 'https://example.com/child', 'token': 'preserve child'})
        page = P.PageParser(name='exchange page', parser_key=bytes.fromhex('22' * 32), content_parsers=contents, subsidiary_page_parsers=[P.SubsidiaryPageParser(formula=P.ParseFormulaStatic(static_text='<p>hello</p>'), page_parser=child)], example_urls=['https://example.com/post/1'], example_parsing_context={'url': 'https://example.com/post/1', 'token': 'preserve 日本'})
        values += [page] + contents
        bundle = S.SerialisableList(values)
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            source = tmp / 'reference.json'
            source.write_text(bundle.DumpToString())
            png = tmp / 'reference.png'
            PNG.DumpToPNG(512, bundle.DumpToNetworkBytes(), 'exchange oracle', 'downloader definitions', 'real reference PNG', str(png))
            expected = json.loads(bundle.DumpToString())
            result = {'reference': expected, 'native_exports_loaded_in_reference': [], 'duplicates': {}}
            for label, input_path in [('text', source), ('png', png)]:
                native_json = tmp / (label + '.json')
                native_png = tmp / (label + '.png')
                subprocess.run([executable, str(input_path), str(native_json), str(native_png)], check=True)
                parsed_json = S.CreateFromString(native_json.read_text(), raise_error_on_future_version=True)
                parsed_png = S.CreateFromNetworkBytes(PNG.LoadFromPNG(str(native_png)))
                assert json.loads(parsed_json.DumpToString()) == expected, label + ' JSON differs'
                assert json.loads(parsed_png.DumpToString()) == expected, label + ' PNG differs'
                result['native_exports_loaded_in_reference'].append(label)
            (out / 'downloader_interchange.png').write_bytes(png.read_bytes())
            result['old_versions'] = []
            for current in expected[2]:
                value = current[1]
                kind = value[0]
                old = json.loads(json.dumps(value))
                if kind in (27, 31, 59, 60, 133):
                    name_at = {27: 3, 31: 2, 59: 2, 60: 1, 133: 2}[kind]
                    old[2].pop(name_at)
                    old[1] -= 1
                elif kind == 58:
                    old[2] = 2
                    old[3][3] = [[item[1][2][0], item[1][2][2]] for item in old[3][3][2]]
                elif kind == 50:
                    old[2] = 14
                    mask = old[3][3][2]
                    if len(mask[0]) != 1 or mask[1]:
                        continue
                    old[3][3] = mask[0][0]
                    old[3][4] = mask[2:] + old[3][4]
                elif kind == 30:
                    old[1] = 6
                else:
                    continue
                upgraded = S.CreateFromSerialisableTuple(old)
                expected_upgrade = json.loads(upgraded.DumpToString())
                source.write_text(json.dumps(old))
                subprocess.run([executable, str(source), str(native_json), str(native_png)], check=True)
                assert json.loads(S.CreateFromString(native_json.read_text()).DumpToString()) == expected_upgrade
                result['old_versions'].append({'source': old, 'upgraded': expected_upgrade})
            # Reference duplicate normalization deliberately ignores these
            # identity/editor fields. Record native model comparison targets.
            manager = D.NetworkDomainManager()
            manager.SetGUGs([single])
            renamed = single.Duplicate()
            renamed.SetGUGKeyAndName((bytes.fromhex('33' * 32), 'renamed'))
            result['duplicates']['gug_ignores_key_and_name'] = manager.AlreadyHaveExactlyThisGUG(renamed)
            manager.SetParsers([page])
            renamed = page.Duplicate()
            renamed.SetName('renamed page')
            renamed.SetParserKey(bytes.fromhex('44' * 32))
            renamed.SetExampleURLs(['https://example.com/new'])
            result['duplicates']['parser_ignores_name_key_examples'] = manager.AlreadyHaveExactlyThisParser(renamed)
            result['duplicates']['merged_examples'] = sorted(page.GetExampleURLs())
            return result
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--rust-executable', required=True)
    parser.add_argument('--child', action='store_true')
    parser.add_argument('--out', default=str(HERE / 'fixtures'))
    args = parser.parse_args()
    import hydrus_driver
    if args.child:
        import record_api
        output = hydrus_driver.run_client(record_api.unpack_fixture('basic'), lambda session: record(session, args.rust_executable, Path(args.out)))
        (Path(args.out) / 'downloader_interchange.json').write_text(json.dumps(output, ensure_ascii=False, indent=2))
    else:
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', '--rust-executable', args.rust_executable, '--out', args.out)


if __name__ == '__main__':
    main()
