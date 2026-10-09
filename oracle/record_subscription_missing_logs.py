#!/usr/bin/env python3
"""Record the reference's handling of subscription queries whose log is missing.

1. An import with missing query logs is accepted in the real
   EditSubscriptionsPanel ("import it anyway"): the panel keeps the
   subscription with no log for that query.
2. The subscription's own sync meets the missing log
   (`_SyncQueryLogContainers` -> `_DealWithMissingQueryLogContainerError`):
   the sub pauses, its header is marked missing and a message is shown.
3. The next `ClientGUI._ManageSubscriptions` asks "Missing Query Logs!"; on
   "continue" it writes empty logs and resets the headers.
The real panel, Subscription, header and ClientGUI._ManageSubscriptions run
unchanged; confirmations are scripted and the clock is held. No installed data.
"""
import json
import random
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
NOW = 1_700_000_000


def record(session):
    c = session.controller

    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.core import HydrusData, HydrusExceptions, HydrusSerialisable as S, HydrusTime as T
        from hydrus.client import ClientGlobals as CG
        from hydrus.client.gui import ClientGUIDialogsQuick as Quick, ClientGUISubscriptions as G
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as Windows
        from hydrus.client.gui import ClientGUIDialogsMessage as Message
        from hydrus.client.importing import ClientImportSubscriptionQuery as Q

        old = (T.GetNow, random.choice, HydrusData.ShowText, Quick.GetYesNo, Windows.DialogEdit.exec, c.GetClipboardText, c.Read, Message.ShowInformation, Message.ShowWarning)
        T.GetNow = lambda: NOW
        random.choice = lambda items: items[0]
        texts, questions, notices = [], [], []
        Message.ShowInformation = lambda owner, text, **kw: notices.append(('information', text))
        Message.ShowWarning = lambda owner, text, **kw: notices.append(('warning', text))
        HydrusData.ShowText = lambda text, *a, **k: texts.append(str(text))
        try:
            single = json.loads((HERE / 'fixtures/subscription_exchange.json').read_text())['single']
            missing = json.loads(json.dumps(single))
            missing[2][1] = [26, 3, []]  # no query logs in the package

            def tuple_of(obj):
                return json.loads(obj.DumpToString())

            out = dict(now=NOW, source=missing)

            # 1. the panel accepts it
            def yes_no(win, message, **kw):
                questions.append(dict(message=message, **{k: v for k, v in kw.items() if isinstance(v, str)}))
                return QW.QDialog.DialogCode.Accepted
            Quick.GetYesNo = yes_no
            c.GetClipboardText = lambda: json.dumps(missing)
            panel = G.EditSubscriptionsPanel(c.gui, [])
            try:
                panel._subscriptions_panel._ImportFromClipboard()
                subs = panel._subscriptions.GetData()
                out['panel_import_questions'] = list(questions)
                out['panel_import_notices'] = list(notices)
                out['panel_subscriptions'] = len(subs)
                (sub_after_accept,) = subs
                header = sub_after_accept.GetQueryHeaders()[0]
                out['accepted_header'] = tuple_of(header)
                out['accepted_loaded_container_names'] = sorted(panel._names_to_edited_query_log_containers)
                out['accepted_header_name_loaded'] = header.GetQueryLogContainerName() in panel._names_to_edited_query_log_containers
                (value_subs, value_containers, value_deletees) = panel.GetValue()
                out['value_containers'] = len(value_containers)
                sub_dump = json.loads(value_subs[0].DumpToString())
            finally:
                panel.deleteLater()

            # 2. the subscription's own sync meets the missing log
            sub = S.CreateFromSerialisableTuple(sub_dump)
            texts.clear()
            def read(what, *args, **kwargs):
                raise HydrusExceptions.DBException(HydrusExceptions.DataMissing('gone'), 'missing', 'tb')
            c.Read = read
            out['before_sync_paused'] = sub._paused
            sub._SyncQueryLogContainers()
            out['sync_texts'] = list(texts)
            out['after_sync_paused'] = sub._paused
            out['after_sync_header'] = tuple_of(sub.GetQueryHeaders()[0])
            c.Read = old[6]

            # 3. the next manage subscriptions dialog: the question is in
            # ClientGUI._ManageSubscriptions (a nested function, so its text is
            # taken from the source); "continue" writes an empty container and
            # resets the header, which is the real Reset
            sub = S.CreateFromSerialisableTuple(sub_dump)
            header = sub.GetQueryHeaders()[0]
            header.SetLastCheckTime(NOW - 1000)
            header.SetNextCheckTime(NOW + 1000)
            header.SetPaused(True)
            header.SetCheckerStatus(1)
            out['before_reset_header'] = tuple_of(header)
            container = Q.SubscriptionQueryLogContainer(header.GetQueryLogContainerName())
            header.Reset(container)
            out['reset_header'] = tuple_of(header)
            out['reset_container'] = tuple_of(container)
            return out
        finally:
            (T.GetNow, random.choice, HydrusData.ShowText, Quick.GetYesNo, Windows.DialogEdit.exec, c.GetClipboardText, c.Read, Message.ShowInformation, Message.ShowWarning) = old

    return c.CallBlockingToQt(c.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    import tempfile
    with tempfile.TemporaryDirectory() as work:
        output = Path(work) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/subscription_missing_logs.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
