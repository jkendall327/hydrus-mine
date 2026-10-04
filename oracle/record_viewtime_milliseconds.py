#!/usr/bin/env python3
"""Record the real viewtime editor and DB searches at millisecond boundaries.

Seed exact viewing times through content updates on basic fixture files, edit
the real Qt fields (including empty/multiple canvas selections), and record
the predicate's text, stored tuple and matching file hashes. No URLs/network
requests are used. Query results include the reference's float-to-ms truncation.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC, ClientLocation
        from hydrus.client.gui.search import ClientGUIPredicatesSingle as Panels
        from hydrus.client.metadata import ClientContentUpdates as Updates
        from hydrus.client.search import ClientSearchFileSearchContext, ClientSearchPredicate as P

        controller = session.controller
        with open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')) as stream:
            hashes = sorted(item['hash'] for item in json.load(stream)['files'])
        times = [0, 1, 344, 345, 346, 799, 999, 1000, 1001, 123455, 123456, 123457]
        canvases = [('media', CC.CANVAS_MEDIA_VIEWER), ('preview', CC.CANVAS_PREVIEW),
                    ('client api', CC.CANVAS_CLIENT_API)]
        seeds = []
        updates = []
        for index, file_hash in enumerate(hashes):
            for name, canvas in canvases:
                value = times[index] if index < len(times) and name == 'media' else (2 if name == 'preview' else 0)
                seeds.append({'hash': file_hash, 'canvas': name, 'viewtime_ms': value})
                updates.append(Updates.ContentUpdate(HC.CONTENT_TYPE_FILE_VIEWING_STATS,
                    HC.CONTENT_UPDATE_SET, (bytes.fromhex(file_hash), canvas, 1_700_000_000_000, 1, value)))
        controller.WriteSynchronous('content_updates',
            Updates.ContentUpdatePackage.STATICCreateFromContentUpdates(CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, updates))
        panel = Panels.PanelPredicateSystemFileViewingStatsViewtime(controller.gui,
            P.Predicate(P.PREDICATE_TYPE_SYSTEM_FILE_VIEWING_STATS))
        cases = []
        try:
            for locations in (['media'], [], ['media', 'preview'], ['client api']):
                for milliseconds in (1, 345, 999, 1000, 1001, 123456):
                    for operator in ('<', HC.UNICODE_APPROX_EQUAL, '=', '>'):
                        panel._viewing_locations.SetValue(locations)
                        panel._sign.SetValue(operator)
                        # Set individual user fields: SetValue itself can round
                        # fractional inputs, so drive the exact integer widgets.
                        delta = panel._time_delta
                        seconds, ms = divmod(milliseconds, 1000)
                        days, seconds = divmod(seconds, 86400)
                        hours, seconds = divmod(seconds, 3600)
                        minutes, seconds = divmod(seconds, 60)
                        for field, value in (('_days', days), ('_hours', hours),
                                             ('_minutes', minutes), ('_seconds', seconds),
                                             ('_milliseconds', ms)):
                            getattr(delta, field).setValue(value)
                        predicate = panel.GetPredicates()[0]
                        context = ClientSearchFileSearchContext.FileSearchContext(
                            location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),
                            predicates=[predicate])
                        ids = controller.Read('file_query_ids', context)
                        found = controller.Read('hash_ids_to_hashes', ids)
                        cases.append({'milliseconds': milliseconds, 'locations': locations,
                            'operator': operator, 'value_seconds': predicate.GetValue()[3],
                            'query_threshold_ms': int(predicate.GetValue()[3] * 1000),
                            'text': predicate.ToString(), 'serialised': predicate.GetSerialisableTuple(),
                            'hashes': sorted(value.hex() for value in found.values())})
        finally:
            panel.deleteLater()
        return {'seeds': seeds, 'cases': cases}
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv) > 1:
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as stream:
            json.dump(result, stream)
        return
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    output = os.path.join(HERE, 'fixtures', 'viewtime_milliseconds.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()
