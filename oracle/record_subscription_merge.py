#!/usr/bin/env python3
"""Record actual Qt multi-group subscription Merge and query-log ownership.

Runs EditSubscriptionsPanel.Merge unchanged on the basic client with fresh
subscriptions and independent file/gallery histories. Only user answers and
information/warning presentation are scripted. Records the list during each
question, all final query identities/state/history, original objects and
GetValue's retained/deleted log names. No network requests or database writes.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'subscription_merge.json')
NOW = 1_700_000_000


def record(session):
    controller = session.controller

    def run():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusExceptions, HydrusTime
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUIDialogsMessage, ClientGUIDialogsQuick, ClientGUISubscriptions
        from hydrus.client.importing import ClientImportFileSeeds, ClientImportGallerySeeds
        from hydrus.client.importing import ClientImportSubscriptionQuery as Query
        from hydrus.client.importing import ClientImportSubscriptions as Subs

        originals = []
        panel = None
        answers = []
        questions = []
        log_labels = {}
        source_labels = {}

        def snapshot(subscriptions):
            result = []
            for sub in sorted(subscriptions, key=lambda s: s.GetName()):
                queries = []
                for header in sub.GetQueryHeaders():
                    log = panel._names_to_edited_query_log_containers[header.GetQueryLogContainerName()]
                    queries.append({
                        'text': header.GetQueryText(),
                        'log': log_labels[header.GetQueryLogContainerName()],
                        'state': header.GetSerialisableTuple(),
                        'files': [[s.file_seed_data, s.status, s.note, s.created, s.modified]
                                  for s in log.GetFileSeedCache().GetFileSeeds()],
                        'gallery': [[s.url, s.status, s.note, s.created, s.modified]
                                    for s in log.GetGallerySeedLog().GetGallerySeeds()],
                    })
                result.append({'name': sub.GetName(), 'source': sub.GetGUGKeyAndName()[1],
                    'source_key': source_labels[sub.GetGUGKeyAndName()[0]],
                    'paused': sub.IsPaused(), 'limits': list(sub.ToTuple()[:2]), 'queries': queries})
            return result

        def current():
            return snapshot(panel._subscriptions.GetData())

        def identities(subscriptions):
            return [{'name': sub['name'], 'logs': [q['log'] for q in sub['queries']]}
                    for sub in subscriptions]

        def ask(kind, **fields):
            answer = answers.pop(0)
            questions.append({'kind': kind, **fields, 'answer': answer, 'during': identities(current())})
            return answer

        def yes_no(win, message, **kwargs):
            return QW.QDialog.DialogCode.Accepted if ask('confirm', message=message) else QW.QDialog.DialogCode.Rejected

        def primary(win, title, choices, **kwargs):
            answer = ask('primary', title=title, choices=[c[0] for c in choices])
            if answer is None:
                raise HydrusExceptions.CancelledException()
            return next(value for label, value, *rest in choices if label == answer)

        def name(win, message, default='', **kwargs):
            answer = ask('name', message=message, default=default)
            if answer is None:
                raise HydrusExceptions.CancelledException()
            return answer

        def information(win, message, **kwargs):
            questions.append({'kind': 'information', 'message': message, 'during': identities(current())})

        def warning(win, message, **kwargs):
            questions.append({'kind': 'warning', 'message': message, 'during': identities(current())})

        replaced = [(HydrusTime, 'GetNow', lambda: NOW),
                    (ClientGUIDialogsQuick, 'GetYesNo', yes_no),
                    (ClientGUIDialogsQuick, 'SelectFromList', primary),
                    (ClientGUIDialogsQuick, 'EnterText', name),
                    (ClientGUIDialogsMessage, 'ShowInformation', information),
                    (ClientGUIDialogsMessage, 'ShowWarning', warning)]
        saved = [(module, attr, getattr(module, attr)) for module, attr, _ in replaced]
        for module, attr, value in replaced:
            setattr(module, attr, value)
        cases = []
        try:
            for label, selected, scripted in [
                ('confirmation cancel', ['a1', 'a2', 'b1', 'b2'], [False]),
                ('first primary cancel', ['a1', 'a2', 'b1', 'b2'], [True, None]),
                ('later primary cancel', ['a1', 'a2', 'b1', 'b2'], [True, 'b1', 'staged', None]),
                ('later primary cancel after name cancel', ['a1', 'a2', 'b1', 'b2'], [True, 'b1', None, None]),
                ('both names cancel', ['a1', 'a2', 'b1', 'b2'], [True, 'b1', None, 'a2', None]),
                ('later absorbed name available', ['a1', 'a2', 'b1', 'b2'], [True, 'b1', 'a1', 'a2', 'merged']),
                ('duplicate new names', ['a1', 'a2', 'b1', 'b2'], [True, 'b1', 'combined', 'a1', 'COMBINED']),
                ('own old name reserved', ['b1', 'b2'], [True, 'b1', 'B1']),
                ('no compatible group', ['a1', 'b1'], [True]),
            ]:
                originals = []
                logs = {}
                log_labels.clear()
                source_labels.clear()
                for index, sub_name in enumerate(['a1', 'a2', 'b1', 'b2', 'outside']):
                    source = 'alpha' if sub_name[0] == 'a' else 'beta' if sub_name[0] == 'b' else 'outside'
                    key = bytes([index + 1]) * 32
                    source_labels[key] = sub_name
                    sub = Subs.Subscription(sub_name, gug_key_and_name=(key, source))
                    sub.SetPaused(sub_name == 'a1')
                    sub.SetFileLimits(50 + index, 10 + index)
                    headers = []
                    for text in ['shared', sub_name]:
                        header = Query.SubscriptionQueryHeader()
                        header.SetQueryText(text)
                        header.SetDisplayName(sub_name + ' display ' + text)
                        header.SetQueryLogContainerName('log-' + sub_name + '-' + text)
                        header.SetLastCheckTime(NOW - 100 - index)
                        header.SetCheckNow(index % 2 == 0)
                        header.SetPaused(index % 2 == 1)
                        log = Query.SubscriptionQueryLogContainer(header.GetQueryLogContainerName())
                        log_labels[log.GetName()] = log.GetName()
                        file_seed = ClientImportFileSeeds.FileSeed(ClientImportFileSeeds.FILE_SEED_TYPE_URL,
                            f'https://subscription.example/{sub_name}/{text}/file')
                        file_seed.status = CC.STATUS_ERROR if index % 2 else CC.STATUS_SUCCESSFUL_AND_NEW
                        file_seed.note = sub_name + ' file note'
                        log.GetFileSeedCache().AddFileSeeds([file_seed])
                        gallery = ClientImportGallerySeeds.GallerySeed(f'https://subscription.example/{sub_name}/{text}/gallery', True)
                        gallery.status = CC.STATUS_ERROR if index % 2 else CC.STATUS_SUCCESSFUL_AND_NEW
                        gallery.note = sub_name + ' gallery note'
                        log.GetGallerySeedLog().AddGallerySeeds([gallery])
                        logs[log.GetName()] = log
                        headers.append(header)
                    sub.SetQueryHeaders(headers)
                    originals.append(sub)
                panel = ClientGUISubscriptions.EditSubscriptionsPanel(controller.gui, originals)
                panel._names_to_edited_query_log_containers.update(logs)
                panel._subscriptions.SelectDatas([s for s in panel._subscriptions.GetData()
                    if s.GetName() in selected], deselect_others=True)
                before = current()
                original_before = snapshot(originals)
                questions.clear()
                answers[:] = scripted
                panel.Merge()
                assert not answers, (label, answers)
                values, retained_logs, deleted_logs = panel.GetValue()
                cases.append({'case': label, 'selected': selected, 'script': scripted,
                    'before': before, 'questions': list(questions), 'after': snapshot(values),
                    'source_objects_unchanged': snapshot(originals) == original_before,
                    'retained_logs': sorted(l.GetName() for l in retained_logs),
                    'deleted_logs': sorted(deleted_logs)})
                assert cases[-1]['source_objects_unchanged']
                assert not deleted_logs
                assert all(q.get('during') == identities(before) for q in questions)
                panel.deleteLater()
            return {'now': NOW, 'cases': cases}
        finally:
            for module, attr, value in saved:
                setattr(module, attr, value)
    return controller.CallBlockingToQt(controller.gui, run)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as output:
            json.dump(result, output)
        return
    with tempfile.TemporaryDirectory() as work:
        path = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as source:
            result = json.load(source)
    with open(OUT, 'w') as output:
        json.dump(result, output, indent=1, ensure_ascii=False)
        output.write('\n')
    print(f'wrote {OUT}')


if __name__ == '__main__':
    main()
