#!/usr/bin/env python3
"""Record actual registered login scripts in the mixed downloader export/import.

Real multi-select dialogs are answered with Qt timers; the actual export panel,
dependency expansion, summary confirmation, PNGExportPanel.Export, PNG ingestion,
duplicate comparison and manager linking run unchanged. Only confirmations and
informational notices receive scripted answers/observations. All registered
definitions use reserved synthetic domains and dummy saved credentials. No HTTP
request or native build is needed. Outputs include the real mixed PNG and JSON.
"""
import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session, output):
    c = session.controller

    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client import ClientStrings
        from hydrus.client.gui import ClientGUIDialogsQuick as Quick
        from hydrus.client.gui import ClientGUIDialogsMessage as Message
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows
        from hydrus.client.gui.parsing import ClientGUIParsing
        from hydrus.client.gui.panels import ClientGUISerialisableImport as Import
        from hydrus.client.networking import ClientNetworkingGUG as G
        from hydrus.client.networking import ClientNetworkingLogin as L
        from hydrus.client.networking import ClientNetworkingURLClass as U
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable as S

        engine = c.network_engine
        domain = engine.domain_manager
        login = engine.login_manager
        original = S.CreateFromSerialisableTuple(json.loads((HERE / 'fixtures/login_editors.json').read_text())['script'])
        script = L.LoginScriptDomain(
            'mixed package login',
            required_cookies_info=original.GetRequiredCookiesInfo(),
            credential_definitions=original.GetCredentialDefinitions(),
            login_steps=original.GetLoginSteps(),
            example_domains_info=[('packages.example', 0, 'Package access.'), ('unconfigured.example', 3, 'Preferences.')],
        )
        script.SetLoginScriptKey(bytes.fromhex('71' * 32))
        other = script.Duplicate()
        other.SetLoginScriptKeyAndName((bytes.fromhex('72' * 32), 'unselected package login'))
        other._example_domains_info = [('unselected.example', 0, 'Unselected access.')]
        login.SetLoginScripts([script, other])
        single = G.GalleryURLGenerator('mixed package gallery', url_template='https://packages.example/search/%tags%', replacement_phrase='%tags%')
        single.SetGUGKeyAndName((bytes.fromhex('73' * 32), single.GetName()))
        nested = G.NestedGalleryURLGenerator('mixed package nested', gug_keys_and_names=[single.GetGUGKeyAndName()])
        nested.SetGUGKeyAndName((bytes.fromhex('74' * 32), nested.GetName()))
        fixed = lambda value: ClientStrings.StringMatch(match_type=ClientStrings.STRING_MATCH_FIXED, match_value=value, example_string=value)
        gallery = U.URLClass('mixed package gallery class', url_class_key=bytes.fromhex('75' * 32), url_type=HC.URL_TYPE_GALLERY,
                             url_domain_mask=U.URLDomainMask(raw_domains=['packages.example']),
                             path_components=[(fixed('search'), None), (ClientStrings.StringMatch(), None)], parameters=[],
                             example_url='https://packages.example/search/cats')
        post = U.URLClass('mixed package post class', url_class_key=bytes.fromhex('76' * 32), url_type=HC.URL_TYPE_POST,
                          url_domain_mask=U.URLDomainMask(raw_domains=['packages.example']),
                          path_components=[(fixed('post'), None), (ClientStrings.StringMatch(), None)], parameters=[],
                          example_url='https://packages.example/post/42')
        parser = P.PageParser(name='mixed package parser', parser_key=bytes.fromhex('77' * 32),
                              example_urls=[gallery.GetExampleURL(), post.GetExampleURL()])
        domain.SetGUGs([single, nested])
        domain.SetURLClasses([gallery, post])
        domain.SetParsers([parser])
        domain.TryToLinkURLClassesAndParsers()
        exporter = ClientGUIParsing.DownloaderExportPanel(c.gui, engine)
        answers, choices, questions, notices = [], [], [], []
        decision = [True]
        real_edit = Windows.DialogEdit.exec
        real_png = Windows.DialogNullipotent.exec
        real_yes = Quick.GetYesNo
        real_information = Message.ShowInformation

        def choose(dlg):
            checks = dlg._panel._checkboxes
            answer = answers.pop(0)
            choices.append(dict(title=dlg.windowTitle(), labels=[checks.item(i).text() for i in range(checks.count())], answer=answer))
            def respond():
                if answer is None:
                    dlg.reject()
                else:
                    for i in range(checks.count()):
                        checks.Check(i, checks.GetData(i).GetName() in answer)
                    dlg.accept()
            QC.QTimer.singleShot(0, respond)
            return real_edit(dlg)

        def confirm(parent, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted if decision[0] else QW.QDialog.DialogCode.Rejected

        png_details = {}
        def export_png(dlg):
            panel = dlg._panel
            png_details.update(title=panel._title.text(), description=panel._text.text(), payload_description=panel._payload_description.text())
            def respond():
                panel._filepicker.SetPath(str(output / 'mixed_login_packages.png'))
                panel.Export()
                dlg.accept()
            QC.QTimer.singleShot(0, respond)
            return real_png(dlg)

        def export_state():
            return [dict(name=obj.GetName(), kind=obj.SERIALISABLE_TYPE) for obj in exporter._listctrl.GetData()]

        def manager_state():
            return dict(scripts=[dict(name=s.GetName(), key=s.GetLoginScriptKey().hex()) for s in login.GetLoginScripts()],
                        domains={name: [list(info[0][:1])[0].hex(), info[0][1], *info[1:]] for name, info in login.GetDomainsToLoginInfo().items()},
                        gugs=sorted(g.GetName() for g in domain.GetGUGs()),
                        classes=sorted(u.GetName() for u in domain.GetURLClasses()),
                        parsers=sorted(p.GetName() for p in domain.GetParsers()))

        Windows.DialogEdit.exec = choose
        Windows.DialogNullipotent.exec = export_png
        Quick.GetYesNo = confirm
        Message.ShowInformation = lambda parent, message, **kwargs: notices.append(message)
        importer = None
        try:
            answers.append(None)
            exporter._AddLoginScript()
            cancelled = export_state()
            answers.append([script.GetName()])
            exporter._AddLoginScript()
            login_only = export_state()
            answers.append([nested.GetName()])
            exporter._AddGUG()
            mixed = export_state()
            answers.append([])
            exporter._AddLoginScript()
            bundle = S.SerialisableList(exporter._listctrl.GetData())
            package = json.loads(bundle.DumpToString())
            exporter._Export()
            export_questions = list(questions)
            questions.clear()
            old = script.Duplicate()
            old.SetLoginScriptKey(bytes.fromhex('78' * 32))
            old._example_domains_info = [('packages.example', 3, 'Old access.')]
            login.SetLoginScripts([old])
            login.SetDomainsToLoginInfo({'packages.example': (old.GetLoginScriptKeyAndName(), {'username': 'dummy-retained-user', 'password': 'dummy-retained-password'}, 3, 'Old access.', False, L.VALIDITY_INVALID, 'retained invalidity', 123456, 'retained delay')})
            domain.SetGUGs([])
            domain.SetURLClasses([])
            domain.SetParsers([])
            importer = Import.ReviewDownloaderImport(c.gui, engine)
            importer._select_from_list.setChecked(False)
            before = manager_state()
            decision[0] = False
            importer._ImportPayloads([('synthetic mixed JSON', bundle.DumpToString())])
            declined = manager_state()
            decision[0] = True
            importer._ImportPaths([str(output / 'mixed_login_packages.png')])
            accepted = manager_state()
            imported = next(s for s in login.GetLoginScripts() if s.GetLoginScriptKey() != old.GetLoginScriptKey())
            renamed = imported.Duplicate()
            renamed.SetLoginScriptKeyAndName((bytes.fromhex('79' * 32), 'ignored duplicate name'))
            duplicates_ignore_identity = login.AlreadyHaveExactlyThisLoginScript(renamed)
            importer._ImportPaths([str(output / 'mixed_login_packages.png')])
            reopened = manager_state()
            assert cancelled == []
            assert login_only == [dict(name=script.GetName(), kind=73)]
            assert before == declined
            assert accepted['scripts'] == reopened['scripts']
            assert accepted['domains'] == reopened['domains']
            assert duplicates_ignore_identity
            return dict(reference=package, chooser_states=choices, cancelled=cancelled, login_only=login_only, mixed=mixed,
                        png=png_details, export_questions=export_questions, import_questions=questions, notices=notices,
                        before=before, declined=declined, accepted=accepted, reopened=reopened,
                        duplicate_ignores_key_and_name=duplicates_ignore_identity,
                        imported_key_changed=imported.GetLoginScriptKey() != script.GetLoginScriptKey())
        finally:
            Windows.DialogEdit.exec = real_edit
            Windows.DialogNullipotent.exec = real_png
            Quick.GetYesNo = real_yes
            Message.ShowInformation = real_information
            exporter.deleteLater()
            if importer is not None:
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
        args.out.mkdir(parents=True, exist_ok=True)
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), lambda session: record(session, args.out))
        (args.out / 'mixed_login_packages.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
    else:
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', '--out', str(args.out))


if __name__ == '__main__':
    main()
