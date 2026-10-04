#!/usr/bin/env python3
"""Record real Qt IncrementalTaggingPanel initial-value/clamp failure boundaries.

Compact input descriptors expand to literal storage-tag previews. Extremely long
synthetic previews are injected into real MediaResult tag managers, without
writing noncanonical tags to the fixture DB. The panel and Qt spinboxes are real.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'incremental_number_boundaries.json')
INPUTS = [
    {'name': 'zero', 'digits': '0'},
    {'name': 'below_clamp', 'digits': '9999999'},
    {'name': 'at_clamp', 'digits': '10000000'},
    {'name': 'above_clamp', 'digits': '10000001'},
    {'name': 'qt_int_max', 'digits': '2147483647'},
    {'name': 'above_qt_int', 'digits': '2147483648'},
    {'name': 'long_large_ascii', 'repeat': '9', 'count': 512, 'tail': ''},
    {'name': 'long_small_ascii', 'repeat': '0', 'count': 1024, 'tail': '7'},
    {'name': 'long_small_unicode', 'repeat': '٠', 'count': 1024, 'tail': '٧'},
    {'name': 'long_large_unicode', 'repeat': '٩', 'count': 512, 'tail': ''},
    {'name': 'beyond_python_digit_limit', 'repeat': '0', 'count': 5000, 'tail': '7'},
]

def record(session):
    from hydrus.core import HydrusConstants as HC
    from hydrus.client.media import ClientMediaSingle, ClientMediaManagers
    from hydrus.client.gui.metadata.ClientGUIIncrementalTagging import IncrementalTaggingPanel
    from hydrus.client import ClientConstants as CC
    controller = session.controller
    manifest = json.load(open(os.path.join(HERE, 'fixtures/legacy_db/basic.manifest.json')))
    hashes = [bytes.fromhex(row['hash']) for row in manifest['files'][:2]]
    service = controller.services_manager.GetServices((HC.LOCAL_TAG,))[0].GetServiceKey()

    def work():
        controller.new_options.SetString('last_incremental_tagging_namespace', 'page')
        controller.new_options.SetString('last_incremental_tagging_prefix', '')
        controller.new_options.SetString('last_incremental_tagging_suffix', '')
        results = {row.GetHash(): row for row in controller.Read('media_results', hashes)}
        cases = []
        for descriptor in INPUTS:
            text = descriptor.get('digits')
            if text is None:
                text = descriptor['repeat'] * descriptor['count'] + descriptor['tail']
            medias = [ClientMediaSingle.MediaSingle(results[hash_].Duplicate()) for hash_ in hashes]
            statuses = {status: ({'page:' + text} if status == HC.CONTENT_STATUS_CURRENT else set())
                        for status in (HC.CONTENT_STATUS_CURRENT, HC.CONTENT_STATUS_PENDING,
                                       HC.CONTENT_STATUS_DELETED, HC.CONTENT_STATUS_PETITIONED)}
            medias[0].GetMediaResult().SetTagsManager(ClientMediaManagers.TagsManager({service: statuses}, {service: statuses}))
            before = set(controller.gui.children())
            case = {'input': descriptor}
            try:
                panel = IncrementalTaggingPanel(controller.gui, service, medias)
                case['outcome'] = {'start': panel._start.value(), 'summary': panel._summary_st.text()}
                case['clamp_edits'] = []
                for start, step in [(-10000001, -10001), (-10000000, -10000), (10000000, 10000), (10000001, 10001)]:
                    panel._start.setValue(start)
                    panel._step.setValue(step)
                    case['clamp_edits'].append({'input': [start, step], 'actual': [panel._start.value(), panel._step.value()]})
            except Exception as error:
                case['outcome'] = {'error_type': type(error).__name__, 'error': str(error)}
            finally:
                for widget in set(controller.gui.children()) - before:
                    widget.deleteLater()
            cases.append(case)
        return {'python_int_digit_limit': sys.get_int_max_str_digits(), 'cases': cases}
    return controller.CallBlockingToQt(controller.gui, work)

def child(path):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(path, 'w') as stream:
        json.dump(result, stream)

def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as stream:
            result = json.load(stream)
    with open(OUT, 'w') as stream:
        json.dump(result, stream, indent=2, ensure_ascii=False)
        stream.write('\n')
    print('wrote', OUT)

if __name__ == '__main__':
    main()
