#!/usr/bin/env python3
"""Actual Qt legacy subscription imports with historical file caches 1–7.

Uses real serialisable upgrades, legacy subscription conversion and list import/
export. Synthetic URL/path histories record ordering, deduplication, timestamps,
notes, cached counts/examples and malformed-upgrade failures. No installed data.
"""
import json
import random
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
NOW = 1_700_000_000


def record(session):
    c = session.controller

    def qt():
        from hydrus.core import HydrusSerialisable as S, HydrusTime as T
        from hydrus.client.gui import ClientGUISubscriptions as G, ClientGUIDialogsMessage as M
        from hydrus.client.importing import ClientImportSubscriptionLegacy as L, ClientImportSubscriptions as Subs
        template = json.loads((HERE / 'fixtures/subscription_legacy_exchange.json').read_text())['cases'][2]['source']
        old = (T.GetNow, c.pub, c.GetClipboardText, c.ClipboardHasImage, M.ShowInformation, M.ShowWarning, random.choice)
        copied = []
        messages = []
        T.GetNow = lambda: NOW
        random.choice = lambda items: items[0]
        c.pub = lambda *args, **kw: copied.append(args) if args and args[0] == 'clipboard' else None
        c.ClipboardHasImage = lambda: False
        M.ShowInformation = lambda owner, text: messages.append(text)
        M.ShowWarning = lambda owner, text: messages.append(text)

        def rows(version):
            def info(status, n, note):
                return dict(status=status, added_timestamp=NOW-100+n,
                            last_modified_timestamp=NOW-50+n,
                            source_timestamp=NOW-200+n, note=note)
            return [
                ['https://legacy.example/first', info(1, 0, 42 if version == 1 else 'first')],
                ['https://legacy.example/pending space#fragment', info(0, 1, 'pending')],
                ['https://legacy.example/first', info(7, 2, 'duplicate')],
                ['/synthetic/history/local.jpg', info(7, 3, 'local veto')],
                ['https://legacy.example/last', info(2, 4, 'last')],
            ]

        def current(container):
            value = json.loads(container.DumpToString())
            for header in value[2][0][3][1]:
                old_tags = S.CreateFromSerialisableTuple(header[2][12])
                header[2][12] = old_tags.GetTagImportOptions().GetSerialisableTuple()
            return value

        def upgrade(value):
            # Python upgrades mutate old metadata dictionaries in place. Keep
            # the original carrier frozen for native replay.
            return S.CreateFromSerialisableTuple(json.loads(json.dumps(value)))

        cases = []
        failures = []
        rewrites = []
        notes = []
        try:
            for version in range(1, 8):
                cache = [8, version, rows(version)]
                duplicate_cache = upgrade(cache)
                duplicate_source = json.loads(json.dumps(cache))
                if version >= 5:
                    del cache[2][2]
                upgraded_cache = upgrade(cache)
                source = json.loads(json.dumps(template))
                source[3][1][0][2][8] = cache
                subscription, logs = L.ConvertLegacySubscriptionToNew(upgrade(source))
                container = Subs.SubscriptionContainer()
                container.subscription = subscription
                container.query_log_containers = S.SerialisableList(logs)
                direct = current(container)
                panel = G.EditSubscriptionsPanel(c.gui, [])
                try:
                    c.GetClipboardText = lambda: json.dumps(source)
                    panel._subscriptions_panel._ImportFromClipboard()
                    assert len(panel._subscriptions.GetData()) == 1
                    panel._subscriptions.SelectDatas(panel._subscriptions.GetData())
                    panel._subscriptions_panel._ExportToClipboard()
                    exported = current(S.CreateFromSerialisableTuple(json.loads(copied[-1][2])))
                    cases.append(dict(version=version, source=source,
                                      duplicate_cache=duplicate_source,
                                      duplicate_upgraded=json.loads(duplicate_cache.DumpToString()),
                                      upgraded_cache=json.loads(upgraded_cache.DumpToString()),
                                      direct=direct, exported=exported))
                finally:
                    panel.deleteLater()
                if version >= 5:
                    # Repeats survive the upgrade; the cache drops them when it
                    # first indexes its seeds, so a list export has none.
                    duplicate_source_object = json.loads(json.dumps(template))
                    duplicate_source_object[3][1][0][2][8] = json.loads(json.dumps(duplicate_source))
                    panel = G.EditSubscriptionsPanel(c.gui, [])
                    try:
                        c.GetClipboardText = lambda: json.dumps(duplicate_source_object)
                        panel._subscriptions_panel._ImportFromClipboard()
                        panel._subscriptions.SelectDatas(panel._subscriptions.GetData())
                        panel._subscriptions_panel._ExportToClipboard()
                        cases[-1]['duplicate_source'] = duplicate_source_object
                        cases[-1]['duplicate_exported'] = current(S.CreateFromSerialisableTuple(json.loads(copied[-1][2])))
                    finally:
                        panel.deleteLater()
                rewrite = [8, version, [['https://legacy.example/history//media.tumblr.com/file', rows(version)[0][1]]]]
                rewrites.append(dict(source=rewrite, upgraded=json.loads(upgrade(rewrite).DumpToString())))
            for note in [True, False, None, 42, -17, 1.5, ['complex'], {'complex': True}]:
                cache = [8, 1, rows(1)[:1]]
                cache[2][0][1]['note'] = note
                notes.append(dict(source=cache, upgraded=json.loads(upgrade(cache).DumpToString()), safe=not isinstance(note, (float, list, dict))))
            for version in range(1, 8):
                cache = [8, version, rows(version)]
                del cache[2][0][1]['note']
                try:
                    upgrade(cache)
                except Exception as error:
                    failures.append(dict(version=version, source=cache, error=type(error).__name__))
                else:
                    raise AssertionError('missing note was accepted')
            return dict(now=NOW, cases=cases, failures=failures, rewrites=rewrites, notes=notes, messages=messages)
        finally:
            T.GetNow, c.pub, c.GetClipboardText, c.ClipboardHasImage, M.ShowInformation, M.ShowWarning, random.choice = old
    return c.CallBlockingToQt(c.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as work:
        output = Path(work) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/legacy_seed_caches.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
