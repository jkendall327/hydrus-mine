#!/usr/bin/env python3
"""Record eight Qt option controls in normal and advanced modes, including clamps.

Runs the real client's connection/downloading panels on the basic fixture; each
control opens with an out-of-range saved value and receives values below/above
its bounds through its native setter.
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.client.gui.panels.options import ConnectionPanel, DownloadingPanel
        options = session.controller.new_options
        result = []
        integer_settings = ('network_timeout', 'connection_error_wait_time', 'serverside_bandwidth_wait_time', 'max_network_jobs', 'max_network_jobs_per_domain')
        for key in integer_settings:
            options.SetInteger(key, 4000000)
        for advanced in (False, True):
            options.SetBoolean('advanced_mode', advanced)
            connection = ConnectionPanel.ConnectionPanel(session.controller.gui)
            downloading = DownloadingPanel.DownloadingPanel(session.controller.gui, options)
            controls = []
            for attr in integer_settings:
                widget = getattr(connection, '_' + attr)
                minimum, maximum = widget.minimum(), widget.maximum()
                initial = widget.value()
                widget.setValue(0)
                low = widget.value()
                widget.setValue(4000000)
                controls.append({'setting': attr, 'min': minimum, 'max': maximum, 'initial_from_4000000': initial, 'below': low, 'above': widget.value()})
            for attr in ('downloader_network_error_delay', 'subscription_network_error_delay', 'subscription_other_error_delay'):
                widget = getattr(downloading, '_' + attr)
                controls.append({'setting': attr, 'min': widget._min})
            result.append({'advanced': advanced, 'controls': controls})
            connection.deleteLater()
            downloading.deleteLater()
        return result
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
    output = os.path.join(HERE, 'fixtures', 'options_ranges.json')
    with open(output, 'w') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    print('wrote ' + output)


if __name__ == '__main__':
    main()
