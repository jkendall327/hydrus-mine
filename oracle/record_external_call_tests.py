#!/usr/bin/env python3
"""The external callable editor's 'testing' box, through the real Qt panel.

Drives `TestCallablePanel` (the box at the bottom of the callable editor):
its input rows and preview per call, then 'test availability!' and 'test
call!' for calls that work, fail to start, exit non-zero and run long, with
the text box and button states as the click lands and once the job is done.

The only processes run are `sh` with a harmless script writing an owned
temporary file, and missing executables (which never start).
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'external_call_tests.json')


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client.executables import ClientExecutableActualCall as A
    from hydrus.client.executables import ClientExecutablePipelines as P
    from hydrus.client.gui.panels.options.ExternalProgramsPanel import TestCallablePanel
    c = session.controller
    with tempfile.TemporaryDirectory() as folder:
        output_path = os.path.join(folder, 'owned output.txt')

        def work():
            panel = TestCallablePanel(c.gui)

            def text():
                return panel._raw_output_text_box.toPlainText()

            def settle(button):
                deadline = time.time() + 30
                while not button.isEnabled() and time.time() < deadline:
                    QW.QApplication.processEvents()
                    time.sleep(0.02)
                for _ in range(5):
                    QW.QApplication.processEvents()

            def rows():
                return [dict(name=p._name.text(), value=p._test_value.text())
                        for p in panel._input_param_types_to_edit_panels.values()]

            def process(executable, arguments, parameter=P.PARAMETER_TYPE_FILE_PATH, long_lived=False):
                call = A.ExecutableLocalProcessCall(executable, arguments, [A.LocalProcessCallInputParameterProcessingRule(parameter)])
                call._this_is_a_potentially_long_lived_external_guy = long_lived
                return call

            def describe(call):
                if isinstance(call, A.ExecutableLocalProcessCall):
                    rules = [r.parameter_type for r in call.GetInputParameterProcessingRules()]
                    return dict(kind='process', executable=call.GetExecutablePath(),
                                arguments=call.GetExecutableParameterTemplates(), parameters=rules,
                                long_lived=call.GetThisIsAPotentiallyLongLivedExternalGuy())
                return dict(kind=type(call).__name__)

            # what each call shows before any test: rows, preview, buttons
            setups = []
            for call in [process('xdg-open', ['%path%']),
                         process('firefox', ['%url%'], P.PARAMETER_TYPE_URL),
                         A.ExecutableLocalProcessDefaultLaunchFile(),
                         A.ExecutableLocalProcessDefaultLaunchURL()]:
                panel.SetActualCall(call)
                setups.append(dict(call=describe(call), rows=rows(), preview=panel._actual_command_preview.text(),
                                   availability_enabled=panel._test_availability_button.isEnabled(),
                                   test_call_enabled=panel._test_call_button.isEnabled(), output=text()))

            availability = []
            for call in [process('sh', ['%path%']), process('/bin/sh', ['%path%']),
                         process('no-such-program-anywhere', ['%path%']),
                         A.ExecutableLocalProcessDefaultLaunchFile()]:
                panel.SetActualCall(call)
                if not panel._test_availability_button.isEnabled():
                    availability.append(dict(call=describe(call), enabled=False))
                    continue
                panel._test_availability_button.click()
                interim = dict(output=text(), enabled=panel._test_availability_button.isEnabled())
                settle(panel._test_availability_button)
                availability.append(dict(call=describe(call), enabled=True, interim=interim, output=text(),
                                         enabled_after=panel._test_availability_button.isEnabled()))

            script = 'printf "synthetic 日本" > "$0"'
            calls = []
            for name, call, value in [
                ('works', process('sh', ['-c', script, '%path%']), output_path),
                ('missing', process('no-such-program-anywhere', ['%path%']), '/synthetic/漢.png'),
                ('exit code', process('sh', ['-c', 'exit 3', '%path%']), '/synthetic/漢.png'),
                ('long lived', process('sh', ['-c', script, '%path%'], long_lived=True), output_path),
            ]:
                if os.path.exists(output_path):
                    os.remove(output_path)
                panel.SetActualCall(call)
                for p in panel._input_param_types_to_edit_panels.values():
                    p._test_value.setText(value)
                preview = panel._actual_command_preview.text()
                panel._test_call_button.click()
                interim = dict(output=text(), enabled=panel._test_call_button.isEnabled())
                settle(panel._test_call_button)
                written = None
                if os.path.exists(output_path):
                    with open(output_path, encoding='utf-8') as f:
                        written = f.read()
                calls.append(dict(name=name, call=describe(call), input=value, preview=preview, interim=interim,
                                  output=text(), enabled_after=panel._test_call_button.isEnabled(),
                                  written=written))

            # typing a test value clears the output and redoes the preview
            panel.SetActualCall(process('sh', ['-c', script, 'pre:%path%']))
            panel._test_call_button.click()
            settle(panel._test_call_button)
            before = text()
            p = list(panel._input_param_types_to_edit_panels.values())[0]
            p._test_value.setText('/typed/漢 value.png')
            typed = dict(output_before=before, output=text(), preview=panel._actual_command_preview.text())
            panel.deleteLater()

            # a new editor starts each input at the value last typed for its kind
            remembered = []
            for call in [process('xdg-open', ['%path%']), process('firefox', ['%url%'], P.PARAMETER_TYPE_URL)]:
                again = TestCallablePanel(c.gui)
                again.SetActualCall(call)
                remembered.append(dict(call=describe(call), rows=[dict(name=p._name.text(), value=p._test_value.text())
                                       for p in again._input_param_types_to_edit_panels.values()]))
                again.deleteLater()

            # 'test call!' on the OS launchers opens the input for real: what
            # each would have run is recorded instead
            launched = []
            old_run, old_open = A.HydrusSubprocess.RunSubprocess, A.webbrowser.open
            A.HydrusSubprocess.RunSubprocess = lambda cmd, **kw: launched.append(dict(cmd=list(cmd))) or ('', '', 0)
            A.webbrowser.open = lambda url, *a, **kw: launched.append(dict(url=url)) or True
            os_calls = []
            try:
                for call in [A.ExecutableLocalProcessDefaultLaunchFile(), A.ExecutableLocalProcessDefaultLaunchURL()]:
                    again = TestCallablePanel(c.gui)
                    again.SetActualCall(call)
                    launched.clear()
                    again._test_call_button.click()
                    interim = again._raw_output_text_box.toPlainText()
                    settle(again._test_call_button)
                    os_calls.append(dict(call=describe(call), interim=interim,
                                         output=again._raw_output_text_box.toPlainText(), launched=list(launched)))
                    again.deleteLater()
            finally:
                A.HydrusSubprocess.RunSubprocess, A.webbrowser.open = old_run, old_open
            return dict(setups=setups, availability=availability, calls=calls, typed=typed,
                        remembered=remembered, os_calls=os_calls, output_path=output_path)
        return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=2, ensure_ascii=False)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
