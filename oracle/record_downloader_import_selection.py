#!/usr/bin/env python3
"""Record the reference's downloader import choosing some objects, skipping
objects that aren't downloader ones, and setting the default downloader.

The real ReviewDownloaderImport runs unchanged; its "select objects to add"
chooser is the real multi-select dialog, answered with a Qt timer. Only the
final yes/no and the informational and warning notices are scripted or
observed. Stages, on a client with no downloaders and no default one:

- partial: "select objects from list" on, only the URL class kept;
- rest: the same package again, the downloader, parser and domain
  metadata kept (the URL class is now a duplicate);
- mixed: a package of a downloader and a non-downloader object (a string
  match), "select objects from list" off: the string match is skipped;
- misc only: a package of string matches alone.

After each: the questions, notices and warnings, the client's downloaders,
URL classes and parsers, and its default downloader.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_downloader_import_selection.py
       (writes fixtures/downloader_import_selection.json)
"""
import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    c = session.controller

    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client import ClientConstants as CC
        from hydrus.client import ClientStrings
        from hydrus.client.gui import ClientGUIDialogsQuick as Quick
        from hydrus.client.gui import ClientGUIDialogsMessage as Message
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows
        from hydrus.client.gui.panels import ClientGUISerialisableImport as Import
        from hydrus.client.networking import ClientNetworkingContexts as Contexts
        from hydrus.client.networking import ClientNetworkingDomain as D
        from hydrus.client.networking import ClientNetworkingGUG as G
        from hydrus.client.networking import ClientNetworkingURLClass as U
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable as S
        from hydrus.core.networking import HydrusNetworking as N

        engine = c.network_engine
        domain = engine.domain_manager
        domain.SetGUGs([])
        domain.SetURLClasses([])
        domain.SetParsers([])
        domain.SetDefaultGUGKeyAndName(None)

        fixed = lambda value: ClientStrings.StringMatch(match_type=ClientStrings.STRING_MATCH_FIXED, match_value=value, example_string=value)
        gug = G.GalleryURLGenerator('selection gallery', url_template='https://selection.example/search/%tags%', replacement_phrase='%tags%')
        gug.SetGUGKeyAndName((bytes.fromhex('91' * 32), gug.GetName()))
        gallery = U.URLClass('selection gallery class', url_class_key=bytes.fromhex('92' * 32), url_type=HC.URL_TYPE_GALLERY,
                             url_domain_mask=U.URLDomainMask(raw_domains=['selection.example']),
                             path_components=[(fixed('search'), None), (ClientStrings.StringMatch(), None)], parameters=[],
                             example_url='https://selection.example/search/cats')
        parser = P.PageParser(name='selection parser', parser_key=bytes.fromhex('93' * 32), example_urls=[gallery.GetExampleURL()])
        rules = N.BandwidthRules()
        rules.AddRule(HC.BANDWIDTH_TYPE_REQUESTS, 60, 3)
        metadata = D.DomainMetadataPackage(domain='selection.example', bandwidth_rules=rules)
        package = S.SerialisableList([gug, gallery, parser, metadata])
        other = G.GalleryURLGenerator('mixed gallery', url_template='https://mixed.example/search/%tags%', replacement_phrase='%tags%')
        other.SetGUGKeyAndName((bytes.fromhex('94' * 32), other.GetName()))
        misc = fixed('not a downloader object')
        mixed = S.SerialisableList([other, misc])
        misc_only = S.SerialisableList([misc, fixed('nor this')])

        answers, choices, questions, notices, warnings = [], [], [], [], []
        real_edit = Windows.DialogEdit.exec
        real_yes = Quick.GetYesNo
        real_information = Message.ShowInformation
        real_warning = Message.ShowWarning

        def edit(dlg):
            checks = dlg._panel._checkboxes
            answer = answers.pop(0)
            choices.append(dict(title=dlg.windowTitle(), labels=[checks.item(i).text() for i in range(checks.count())], answer=answer))
            def respond():
                if answer is None:
                    dlg.reject()
                else:
                    for i in range(checks.count()):
                        checks.Check(i, checks.item(i).text() in answer)
                    dlg.accept()
            QC.QTimer.singleShot(0, respond)
            return real_edit(dlg)

        def state():
            default = domain.GetDefaultGUGKeyAndName()
            return dict(gugs=sorted(g.GetName() for g in domain.GetGUGs()),
                        classes=sorted(u.GetName() for u in domain.GetURLClasses()),
                        parsers=sorted(p.GetName() for p in domain.GetParsers()),
                        default_gug=None if default is None else default[1])

        Windows.DialogEdit.exec = edit
        yes_answers = []
        Quick.GetYesNo = lambda parent, message, **kwargs: (questions.append(message), QW.QDialog.DialogCode.Accepted if yes_answers.pop(0) else QW.QDialog.DialogCode.Rejected)[1]
        Message.ShowInformation = lambda parent, message, **kwargs: notices.append(message)
        Message.ShowWarning = lambda parent, message, **kwargs: warnings.append(message)
        importer = Import.ReviewDownloaderImport(c.gui, engine)
        stages = []
        try:
            def stage(name, source, select, answer=None, yes=True):
                choices.clear(); questions.clear(); notices.clear(); warnings.clear()
                yes_answers[:] = [yes]
                if select:
                    answers.append(answer)
                importer._select_from_list.setChecked(select)
                importer._ImportPayloads([('synthetic ' + name, source.DumpToString())])
                stages.append(dict(name=name, source=json.loads(source.DumpToString()), select=select, yes=yes,
                                   choosers=list(choices), questions=list(questions), notices=list(notices),
                                   warnings=list(warnings), state=state()))

            initial = state()
            stage('partial', package, True, ['URL Class: selection gallery class'])
            stage('rest', package, True, ['GUG: selection gallery', 'Parser: selection parser', 'Domain Metadata: selection.example'])
            stage('mixed', mixed, False)
            stage('misc only', misc_only, False)
            # the package again: everything in it is already there
            stage('all duplicates', package, False)
            # a new downloader, the final question declined ("back": nothing is added)
            declined = G.GalleryURLGenerator('declined gallery', url_template='https://declined.example/search/%tags%', replacement_phrase='%tags%')
            declined.SetGUGKeyAndName((bytes.fromhex('95' * 32), declined.GetName()))
            stage('declined', S.SerialisableList([declined]), False, yes=False)
            # the chooser cancelled
            stage('chooser cancelled', S.SerialisableList([declined]), True, answer=None)
            # a new downloader beside one the client has: the duplicate is counted, not added
            fresh = G.GalleryURLGenerator('fresh gallery', url_template='https://fresh.example/search/%tags%', replacement_phrase='%tags%')
            fresh.SetGUGKeyAndName((bytes.fromhex('96' * 32), fresh.GetName()))
            stage('new and a duplicate', S.SerialisableList([fresh, other]), False)
            return dict(initial=initial, stages=stages)
        finally:
            Windows.DialogEdit.exec = real_edit
            Quick.GetYesNo = real_yes
            Message.ShowInformation = real_information
            Message.ShowWarning = real_warning
            importer.deleteLater()

    return c.CallBlockingToQt(c.gui, qt)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--child', action='store_true')
    args = parser.parse_args()
    import hydrus_driver
    if args.child:
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        (HERE / 'fixtures' / 'downloader_import_selection.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
    else:
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child')


if __name__ == '__main__':
    main()
