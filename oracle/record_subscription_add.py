#!/usr/bin/env python3
"""Record real Qt subscription Add dialogs: empty, one/many, cancel and edit.

Runs Add and SelectGUGKeyAndName unchanged. DialogEdit.exec is driven by a
Qt timer, inspecting the real list widget and editor before answering it.
The existing fake-site definitions make galleries functional without requests.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'subscription_add.json')


def record(session):
    controller = session.controller

    def run():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client.gui import ClientGUIDialogsMessage, ClientGUISubscriptions
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows
        from hydrus.client.networking import ClientNetworkingGUG as G
        import record_downloads

        manager = controller.network_engine.domain_manager
        url_classes, parsers, _ = record_downloads.definitions()
        manager.SetURLClasses(url_classes)
        manager.SetParsers(parsers)
        manager.TryToLinkURLClassesAndParsers()
        cases = []
        dialogs = []
        answers = []
        real_exec = Windows.DialogEdit.exec
        real_warning = ClientGUIDialogsMessage.ShowWarning

        def execute(dlg):
            panel = dlg._panel
            entry = {'title': dlg.windowTitle(), 'panel': type(panel).__name__}
            if hasattr(panel, '_list'):
                listing = panel._list
                entry['choices'] = [listing.item(i).text() for i in range(listing.count())]
                entry['selected'] = [item.text() for item in listing.selectedItems()]
            else:
                sub, _ = panel.GetValue()
                entry['name'] = sub.GetName()
                entry['gug'] = sub.GetGUGKeyAndName()[1]
            answer = answers.pop(0)
            entry['answer'] = answer
            dialogs.append(entry)
            def respond():
                if answer is None:
                    dlg.reject()
                else:
                    if hasattr(panel, '_list'):
                        panel._list.clearSelection()
                        panel._list.item(answer).setSelected(True)
                    dlg.accept()
            QC.QTimer.singleShot(0, respond)
            return real_exec(dlg)

        def warning(win, message, **kwargs):
            dialogs.append({'title': 'Warning', 'message': message})

        Windows.DialogEdit.exec = execute
        ClientGUIDialogsMessage.ShowWarning = warning
        try:
            for label, names, scripted in [
                ('no downloaders', [], []),
                ('single cancel', ['alpha'], [None]),
                ('single editor cancel', ['alpha'], [0, None]),
                ('single editor accept', ['alpha'], [0, 0]),
                ('multiple editor cancel', ['zed', 'alpha'], [0, None]),
                ('multiple choose zed', ['zed', 'alpha'], [1, 0]),
            ]:
                gugs = [G.GalleryURLGenerator(name,
                    url_template=record_downloads.BASE + '/search/%tags%/1',
                    replacement_phrase='%tags%', search_terms_separator='+',
                    initial_search_text='tag', example_search_text='blue_eyes') for name in names]
                manager.SetGUGs(gugs)
                manager.SetGUGKeysToDisplay([g.GetGUGKey() for g in gugs])
                manager.SetDefaultGUGKeyAndName(next((g.GetGUGKeyAndName() for g in gugs if g.GetName() == 'zed'), None))
                panel = ClientGUISubscriptions.EditSubscriptionsPanel(controller.gui, [])
                dialogs.clear()
                answers[:] = scripted
                panel.Add()
                assert not answers
                cases.append({'case': label, 'dialogs': list(dialogs),
                    'subscriptions': [s.GetName() for s in panel._subscriptions.GetData()]})
                panel.deleteLater()
            return cases
        finally:
            Windows.DialogEdit.exec = real_exec
            ClientGUIDialogsMessage.ShowWarning = real_warning
    return controller.CallBlockingToQt(controller.gui, run)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        path = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print(f'wrote {OUT}')


if __name__ == '__main__':
    main()
