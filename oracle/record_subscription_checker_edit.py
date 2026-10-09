#!/usr/bin/env python3
"""Record what editing a subscription's checker options does to its query headers.

`Subscription.SetCheckerOptions` (called by the manage subscriptions panel when
its checker options are overwritten or edited) leaves a header whose history
was not loaded unsynced, with the velocity words "will recalculate when next
fully loaded"; a header whose history the dialog holds (an imported one, or
one it has read for an export, reset or retry) is recalculated by
`SyncToQueryLogContainer` at once. The real Subscription, header and
container classes run unchanged; the clock and `random.choice` are held.
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
        from hydrus.client.importing.options import CheckerImportOptions as CIO

        old_now, old_choice = T.GetNow, random.choice
        T.GetNow = lambda: NOW
        random.choice = lambda items: items[0]
        try:
            single = json.loads((HERE / 'fixtures/subscription_exchange.json').read_text())['single']

            def fresh():
                container = S.CreateFromSerialisableTuple(json.loads(json.dumps(single)))
                sub = container.subscription
                log = container.query_log_containers[0]
                return sub, log, sub.GetQueryHeaders()[0]

            def tuple_of(obj):
                return json.loads(obj.DumpToString())

            variants = dict(
                dead=CIO.CheckerOptions(10, 600, 100000, (2, 3600)),
                alive=CIO.CheckerOptions(2, 300, 90000, (1, 86400 * 180)),
                # the reasonable-defaults button "slow thread"
                slow_thread=CIO.CheckerOptions(1.0, 4 * 3600, 7 * 86400, (1, 30 * 86400)),
            )
            out = dict(now=NOW, source=single, checkers={k: tuple_of(v) for k, v in variants.items()}, cases={})
            sub, log, header = fresh()
            out['imported_checker'] = tuple_of(sub._checker_options)
            out['imported_header'] = tuple_of(header)
            for name, checker in variants.items():
                case = {}
                sub, log, header = fresh()
                sub.SetCheckerOptions(checker)
                case['unloaded'] = tuple_of(sub.GetQueryHeaders()[0])
                sub, log, header = fresh()
                sub.SetCheckerOptions(checker, {header.GetQueryLogContainerName(): log})
                case['loaded'] = tuple_of(sub.GetQueryHeaders()[0])
                out['cases'][name] = case
            # an unchanged checker leaves the header alone
            sub, log, header = fresh()
            sub.SetCheckerOptions(CIO.CheckerOptions(*sub._checker_options.ToTuple()))
            out['unchanged'] = tuple_of(sub.GetQueryHeaders()[0])
            return out
        finally:
            T.GetNow, random.choice = old_now, old_choice

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
    (HERE / 'fixtures/subscription_checker_edit.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
