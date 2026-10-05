#!/usr/bin/env python3
"""Actual Qt raw idle seconds -> new controls -> unchanged UpdateOptions.

Abandoned panels preserve raw seconds; unchanged acceptance writes their displayed
whole-minute bounds. Each case constructs real MaintenanceAndProcessingPanel.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def record(session):
    def qt():
        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui.panels.options import MaintenanceAndProcessingPanel as M
        c = session.controller
        old = {key: HC.options[key] for key in ('idle_period', 'idle_mouse_period', 'idle_normal')}
        api = c.new_options.GetNoneableInteger('idle_mode_client_api_timeout')
        def saved():
            return [HC.options['idle_period'], HC.options['idle_mouse_period'], c.new_options.GetNoneableInteger('idle_mode_client_api_timeout')]
        def controls(panel):
            return [dict(number=w._number_value.value(), none=w._checkbox.isChecked(), seconds=w.GetValue()) for w in (panel._idle_period, panel._idle_mouse_period, panel._idle_mode_client_api_timeout)]
        cases = []
        try:
            for raw in (0, 59, 119, 60060, None):
                HC.options['idle_normal'] = True
                HC.options['idle_period'] = raw
                HC.options['idle_mouse_period'] = raw
                c.new_options.SetNoneableInteger('idle_mode_client_api_timeout', raw)
                cancelled = M.MaintenanceAndProcessingPanel(c.gui)
                displayed = controls(cancelled)
                cancelled.deleteLater()
                abandoned = saved()
                panel = M.MaintenanceAndProcessingPanel(c.gui)
                try:
                    panel.UpdateOptions()
                    cases.append(dict(raw=raw, controls=displayed, abandoned=abandoned, accepted=saved()))
                finally:
                    panel.deleteLater()
            return dict(cases=cases)
        finally:
            HC.options.update(old)
            c.new_options.SetNoneableInteger('idle_mode_client_api_timeout', api)
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1:
        import record_api
        output = Path(sys.argv[2])
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        output.write_text(json.dumps(result))
        return
    with tempfile.TemporaryDirectory() as folder:
        output = Path(folder) / 'result.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()), '--child', str(output))
        result = json.loads(output.read_text())
    (HERE / 'fixtures/idle_timeout_constructors.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
