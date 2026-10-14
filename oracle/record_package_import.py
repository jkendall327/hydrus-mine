#!/usr/bin/env python3
"""Record the reference's downloader package import, as ReviewDownloaderImport runs it.

The real import panel receives, as pasted JSON, the package of
`downloader_interchange.json` (a gallery generator, a nested generator over
it, URL classes, a page parser) and then the recorded classes and parsers of
`parser_auto_links.json`. Only the final confirmation and the notices are
scripted (their full text is recorded). After each import the real domain
manager is read: generators with their keys (whether they kept the package's
key) and nested members, URL classes, parsers and the parser links. The same
package is imported a second time to record what the reference treats as
already present.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_package_import.py
       (writes fixtures/package_import.json)
"""
import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))


def record(session):
    c = session.controller

    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUIDialogsQuick as Quick
        from hydrus.client.gui import ClientGUIDialogsMessage as Message
        from hydrus.client.gui.panels import ClientGUISerialisableImport as Import
        from hydrus.client.networking import ClientNetworkingGUG as G
        from hydrus.core import HydrusSerialisable as S

        engine = c.network_engine
        domain = engine.domain_manager
        questions, notices = [], []
        real_yes, real_information = Quick.GetYesNo, Message.ShowInformation

        def confirm(parent, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted

        Quick.GetYesNo = confirm
        Message.ShowInformation = lambda parent, message, **kwargs: notices.append(message)

        def state():
            classes = {u.GetClassKey(): u.GetName() for u in domain.GetURLClasses()}
            parsers = {p.GetParserKey(): p.GetName() for p in domain.GetParsers()}
            gugs = []
            for gug in sorted(domain.GetGUGs(), key=lambda g: g.GetName()):
                entry = {'name': gug.GetName(), 'key': gug.GetGUGKey().hex()}
                if isinstance(gug, G.NestedGalleryURLGenerator):
                    entry['members'] = [[key.hex(), name] for (key, name) in gug.GetGUGKeysAndNames()]
                gugs.append(entry)
            links = domain.GetURLClassKeysToParserKeys()
            return {
                'gugs': gugs,
                'classes': sorted(classes.values()),
                'parsers': sorted(parsers.values()),
                'links': sorted([classes[k], parsers[v]] for (k, v) in links.items() if k in classes and v in parsers),
            }

        def package_a():
            fixture = json.loads((HERE / 'fixtures' / 'downloader_interchange.json').read_text())
            return S.CreateFromString(json.dumps(fixture['reference']))

        def package_b():
            fixture = json.loads((HERE / 'fixtures' / 'parser_auto_links.json').read_text())
            objects = [S.CreateFromSerialisableTuple(x) for x in fixture['classes'] + fixture['parsers']]
            return S.SerialisableList(objects)

        importer = Import.ReviewDownloaderImport(c.gui, engine)
        importer._select_from_list.setChecked(False)
        steps = []
        try:
            for label, make in [('package', package_a), ('package again', package_a), ('linkable package', package_b)]:
                questions.clear()
                notices.clear()
                importer._ImportPayloads([(label, make().DumpToString())])
                steps.append({'step': label, 'questions': list(questions), 'notices': list(notices), 'state': state()})
            return {'steps': steps}
        finally:
            Quick.GetYesNo, Message.ShowInformation = real_yes, real_information
            importer.deleteLater()

    return c.CallBlockingToQt(c.gui, qt)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--child', action='store_true')
    parser.add_argument('--out', type=Path, default=HERE / 'fixtures')
    args = parser.parse_args()
    import hydrus_driver
    if args.child:
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        (args.out / 'package_import.json').write_text(json.dumps(result, indent=1, ensure_ascii=False) + '\n')
    else:
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', '--out', str(args.out))


if __name__ == '__main__':
    main()
