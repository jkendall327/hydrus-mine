#!/usr/bin/env python3
"""Record domain metadata (headers/bandwidth rules) in downloader packages.

The real DownloaderExportPanel and ReviewDownloaderImport run unchanged: the
domain prompt is the real EnterText dialog (typed into and accepted or
rejected with a Qt timer), the GUG chooser is the real multi-select dialog,
and PNGExportPanel.Export writes the real PNG. Only yes/no confirmations and
informational notices are scripted/observed. Headers and rules use reserved
synthetic domains. No HTTP request is made.
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
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsQuick as Quick
        from hydrus.client.gui import ClientGUIDialogsMessage as Message
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows
        from hydrus.client.gui.panels import ClientGUIScrolledPanelsTextEntry as TextEntry
        from hydrus.client.gui.parsing import ClientGUIParsing
        from hydrus.client.gui.panels import ClientGUISerialisableImport as Import
        from hydrus.client.networking import ClientNetworkingContexts as Contexts
        from hydrus.client.networking import ClientNetworkingDomain as D
        from hydrus.client.networking import ClientNetworkingGUG as G
        from hydrus.core import HydrusConstants as HC, HydrusSerialisable as S
        from hydrus.client import ClientStrings
        from hydrus.client.networking import ClientNetworkingURLClass as U
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core.networking import HydrusNetworking as N

        engine = c.network_engine
        domain = engine.domain_manager
        bandwidth = engine.bandwidth_manager

        def nc(name):
            return Contexts.NetworkContext(CC.NETWORK_CONTEXT_DOMAIN, name)

        def rules(*triples):
            r = N.BandwidthRules()
            for t in triples:
                r.AddRule(*t)
            return r

        def set_headers(name, rows):
            for (key, value, approved, reason) in rows:
                domain.SetCustomHeader(nc(name), key, value=value, approved=approved, reason=reason)

        def domain_state():
            headers = {}
            for (context, d) in domain.GetNetworkContextsToCustomHeaderDicts().items():
                if context.context_type == CC.NETWORK_CONTEXT_DOMAIN and context.context_data is not None:
                    headers[context.context_data] = {k: list(v) for (k, v) in sorted(d.items())}
            rule_map = {}
            for (context, r) in bandwidth._network_contexts_to_bandwidth_rules.items():
                if context.context_type == CC.NETWORK_CONTEXT_DOMAIN and context.context_data is not None:
                    rule_map[context.context_data] = sorted([list(rule) for rule in r.GetRules()], key=lambda x: (x[0], x[1] or 0, x[2]))
            return dict(headers=dict(sorted(headers.items())), rules=dict(sorted(rule_map.items())),
                        gugs=sorted(g.GetName() for g in domain.GetGUGs()),
                        classes=sorted(u.GetName() for u in domain.GetURLClasses()))

        def clear_domains():
            for (context, d) in list(domain.GetNetworkContextsToCustomHeaderDicts().items()):
                if context.context_type == CC.NETWORK_CONTEXT_DOMAIN and context.context_data is not None:
                    for key in list(d):
                        domain.DeleteCustomHeader(context, key)
            for context in list(bandwidth._network_contexts_to_bandwidth_rules):
                if context.context_type == CC.NETWORK_CONTEXT_DOMAIN and context.context_data is not None:
                    bandwidth.DeleteRules(context)

        clear_domains()
        set_headers('packages.example', [
            ('Referer', 'https://packages.example/', D.VALID_APPROVED, 'The gallery checks the referral.'),
            ('X-Pending', 'not shared', D.VALID_UNKNOWN, 'Waiting for approval.'),
            ('X-Denied', 'never shared', D.VALID_DENIED, 'Denied.'),
        ])
        set_headers('api.packages.example', [('X-Api-Version', '2', D.VALID_APPROVED, 'The API needs it.')])
        set_headers('pendingonly.example', [('X-Pending', 'not shared', D.VALID_UNKNOWN, 'Waiting.')])
        bandwidth.SetRules(nc('packages.example'), rules((HC.BANDWIDTH_TYPE_REQUESTS, 2, 1), (HC.BANDWIDTH_TYPE_DATA, 86400, 64 * 1024 * 1024)))
        bandwidth.SetRules(nc('rulesonly.example'), rules((HC.BANDWIDTH_TYPE_REQUESTS, None, 5000)))
        gug = G.GalleryURLGenerator('rules only gallery', url_template='https://www.rulesonly.example/search/%tags%', replacement_phrase='%tags%')
        gug.SetGUGKeyAndName((bytes.fromhex('81' * 32), gug.GetName()))
        fixed = lambda value: ClientStrings.StringMatch(match_type=ClientStrings.STRING_MATCH_FIXED, match_value=value, example_string=value)
        gallery = U.URLClass('rules only gallery class', url_class_key=bytes.fromhex('82' * 32), url_type=HC.URL_TYPE_GALLERY,
                             url_domain_mask=U.URLDomainMask(raw_domains=['rulesonly.example']),
                             path_components=[(fixed('search'), None), (ClientStrings.StringMatch(), None)], parameters=[],
                             example_url='https://www.rulesonly.example/search/cats')
        parser = P.PageParser(name='rules only parser', parser_key=bytes.fromhex('83' * 32), example_urls=[gallery.GetExampleURL()])
        domain.SetGUGs([gug])
        domain.SetURLClasses([gallery])
        domain.SetParsers([parser])
        domain.TryToLinkURLClassesAndParsers()
        initial = domain_state()

        exporter = ClientGUIParsing.DownloaderExportPanel(c.gui, engine)
        answers, prompts, choices, questions, notices = [], [], [], [], []
        decision = [True]
        real_edit = Windows.DialogEdit.exec
        real_png = Windows.DialogNullipotent.exec
        real_yes = Quick.GetYesNo
        real_information = Message.ShowInformation

        def edit(dlg):
            panel = dlg._panel
            answer = answers.pop(0)
            if isinstance(panel, TextEntry.EditTextPanel):
                prompts.append(dict(title=dlg.windowTitle(), message=panel.findChildren(QW.QLabel)[0].text(), answer=answer))
                def respond():
                    if answer is None:
                        dlg.reject()
                    else:
                        panel._text.setText(answer)
                        dlg.accept()
            else:
                checks = panel._checkboxes
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

        def confirm(parent, message, **kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Accepted if decision[0] else QW.QDialog.DialogCode.Rejected

        png_details = {}
        def export_png(dlg):
            panel = dlg._panel
            png_details.update(title=panel._title.text(), description=panel._text.text(), payload_description=panel._payload_description.text())
            def respond():
                panel._filepicker.SetPath(str(output / 'domain_metadata_packages.png'))
                panel.Export()
                dlg.accept()
            QC.QTimer.singleShot(0, respond)
            return real_png(dlg)

        def export_state():
            return [dict(name=exporter._ConvertContentToDisplayTuple(obj)[0], kind=obj.SERIALISABLE_TYPE, type=obj.SERIALISABLE_NAME)
                    for obj in exporter._listctrl.GetData()]

        def step(name, action):
            notices.clear()
            action()
            return dict(step=name, state=export_state(), notices=list(notices))

        Windows.DialogEdit.exec = edit
        Windows.DialogNullipotent.exec = export_png
        Quick.GetYesNo = confirm
        Message.ShowInformation = lambda parent, message, **kwargs: notices.append(message)
        importer = None
        try:
            export_steps = []
            answers.append(None)
            export_steps.append(step('cancelled', exporter._AddDomainMetadata))
            answers.append('nothing.example')
            export_steps.append(step('nothing', exporter._AddDomainMetadata))
            answers.append('pendingonly.example')
            export_steps.append(step('pending only', exporter._AddDomainMetadata))
            answers.append('www.api.packages.example')
            export_steps.append(step('subdomain', exporter._AddDomainMetadata))
            answers.append('packages.example')
            export_steps.append(step('again', exporter._AddDomainMetadata))
            answers.append(['rules only gallery'])
            export_steps.append(step('gug', exporter._AddGUG))
            bundle = S.SerialisableList(exporter._listctrl.GetData())
            package = json.loads(bundle.DumpToString())
            exporter._Export()
            export_questions = list(questions)
            questions.clear()
            summaries = {obj.GetDomain(): dict(safe=obj.GetSafeSummary(), detailed=obj.GetDetailedSafeSummary())
                         for obj in bundle if isinstance(obj, D.DomainMetadataPackage)}

            # Import into a client whose packages.example headers already match
            # and whose rules differ.
            clear_domains()
            domain.SetGUGs([])
            domain.SetURLClasses([])
            domain.SetParsers([])
            set_headers('packages.example', [
                ('Referer', 'https://packages.example/', D.VALID_UNKNOWN, 'An older reason.'),
            ])
            set_headers('api.packages.example', [('X-Other', 'kept?', D.VALID_APPROVED, 'Replaced by import.')])
            bandwidth.SetRules(nc('packages.example'), rules((HC.BANDWIDTH_TYPE_REQUESTS, 60, 10)))
            importer = Import.ReviewDownloaderImport(c.gui, engine)
            before = domain_state()
            importer._select_from_list.setChecked(True)
            answers.append(None)
            notices.clear()
            importer._ImportPayloads([('synthetic domain JSON', bundle.DumpToString())])
            chooser_cancelled = dict(state=domain_state(), notices=list(notices), questions=list(questions))
            importer._select_from_list.setChecked(False)
            decision[0] = False
            notices.clear()
            importer._ImportPayloads([('synthetic domain JSON', bundle.DumpToString())])
            declined = dict(state=domain_state(), notices=list(notices), questions=list(questions))
            questions.clear()
            decision[0] = True
            notices.clear()
            importer._ImportPaths([str(output / 'domain_metadata_packages.png')])
            accepted = dict(state=domain_state(), notices=list(notices), questions=list(questions))
            questions.clear()
            notices.clear()
            importer._ImportPaths([str(output / 'domain_metadata_packages.png')])
            again = dict(state=domain_state(), notices=list(notices), questions=list(questions))
            questions.clear()

            # Chooser labels, and the reference's early return in the
            # bandwidth manager: a headers-only package first stops later rules.
            clear_domains()
            mixed = S.SerialisableList([
                D.DomainMetadataPackage(domain='headersonly.example', headers_list=[('X-A', 'a', 'Header only.')]),
                D.DomainMetadataPackage(domain='zrules.example', bandwidth_rules=rules((HC.BANDWIDTH_TYPE_REQUESTS, 1, 1))),
                D.DomainMetadataPackage(domain='both.example', headers_list=[('X-B', 'b', 'Both.')], bandwidth_rules=rules((HC.BANDWIDTH_TYPE_DATA, None, 1024))),
            ])
            importer._select_from_list.setChecked(True)
            answers.append(['Domain Metadata: both.example', 'Domain Metadata: headersonly.example', 'Domain Metadata: zrules.example'])
            notices.clear()
            importer._ImportPayloads([('synthetic mixed JSON', mixed.DumpToString())])
            stopped = dict(source=json.loads(mixed.DumpToString()), state=domain_state(), notices=list(notices), questions=list(questions))
            questions.clear()

            # More than eight packages show only the first eight summaries.
            clear_domains()
            many = S.SerialisableList([
                D.DomainMetadataPackage(domain='site{}.example'.format(i), bandwidth_rules=rules((HC.BANDWIDTH_TYPE_REQUESTS, 60, i + 1)))
                for i in range(9)
            ])
            importer._select_from_list.setChecked(False)
            notices.clear()
            importer._ImportPayloads([('synthetic many JSON', many.DumpToString())])
            nine = dict(source=json.loads(many.DumpToString()), state=domain_state(), notices=list(notices), questions=list(questions))
            questions.clear()

            return dict(initial=initial, reference=package, export_steps=export_steps, prompts=prompts,
                        chooser_states=choices, png=png_details, export_questions=export_questions,
                        summaries=summaries, before=before, chooser_cancelled=chooser_cancelled,
                        declined=declined, accepted=accepted, again=again, stopped=stopped, nine=nine)
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
        (args.out / 'domain_metadata_packages.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
    else:
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', '--out', str(args.out))


if __name__ == '__main__':
    main()
