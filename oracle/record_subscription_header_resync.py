#!/usr/bin/env python3
"""Record how the reference treats a query header that is not synced to its log.

A header marked LOG_CONTAINER_UNSYNCED (what a fresh query, a renamed history or
an edit to the checker options leaves) holds default velocity and no example
gallery seed until the subscription's own Sync reads the log container and
`SyncToQueryLogContainer` recalculates it. The real Subscription,
SubscriptionQueryHeader and SubscriptionContainer classes run unchanged; only
the clock, `random.choice` and the database read of the log container are held.
No installed data.
"""
import json
import random
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
NOW = 1_700_000_000


def record(session):
    c = session.controller

    def qt():
        from hydrus.core import HydrusSerialisable as S, HydrusTime as T
        from hydrus.client import ClientGlobals as CG
        from hydrus.client.importing import ClientImportSubscriptionQuery as Q

        old_now, old_choice, old_read = T.GetNow, random.choice, c.Read
        T.GetNow = lambda: NOW
        random.choice = lambda items: items[0]
        try:
            single = json.loads((HERE / 'fixtures/subscription_exchange.json').read_text())['single']
            container = S.CreateFromSerialisableTuple(json.loads(json.dumps(single)))
            sub = container.subscription
            log = container.query_log_containers[0]
            header = sub.GetQueryHeaders()[0]

            def tuple_of(h):
                return json.loads(h.DumpToString())

            def reads(name_type, name):
                assert name == header.GetQueryLogContainerName()
                return log
            c.Read = lambda what, *args, **kwargs: reads(*args)

            out = dict(now=NOW, source=single)
            out['imported'] = tuple_of(header)
            out['imported_wants_resync'] = header.WantsToResyncWithLogContainer()
            header.SetQueryLogContainerStatus(Q.LOG_CONTAINER_UNSYNCED)
            out['unsynced'] = tuple_of(header)
            out['unsynced_wants_resync'] = header.WantsToResyncWithLogContainer()
            out['unsynced_next_check_status'] = header.GetNextCheckStatusString()
            out['unsynced_subscription_wants_sync'] = sub._SyncQueryLogContainersCanDoWork()
            sub._SyncQueryLogContainers()
            out['synced'] = tuple_of(header)
            out['synced_wants_resync'] = header.WantsToResyncWithLogContainer()
            out['synced_subscription_wants_sync'] = sub._SyncQueryLogContainersCanDoWork()
            # a header the way the add-query panel makes one: defaults, then the
            # file status of its history
            fresh = Q.SubscriptionQueryHeader()
            fresh.SetQueryLogContainerName(header.GetQueryLogContainerName())
            fresh.SetQueryText('fresh query')
            out['fresh'] = tuple_of(fresh)
            fresh.UpdateFileStatus(log)
            out['fresh_updated'] = tuple_of(fresh)
            out['fresh_wants_resync'] = fresh.WantsToResyncWithLogContainer()
            return out
        finally:
            T.GetNow, random.choice, c.Read = old_now, old_choice, old_read

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
    (HERE / 'fixtures/subscription_header_resync.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
