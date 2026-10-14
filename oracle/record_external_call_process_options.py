#!/usr/bin/env python3
"""The external call process options at work, through the reference's real classes.

Runs `sh` with harmless scripts, under the options the call editor sets
(timeout, long-lived, hide terminal, output is text):

- `TestCallablePanel`'s 'test call!' (the editor's testing box): the text it
  shows, and how long the call took;
- `ExecutableLocalProcessCall.Call` (what a real launch does): the exception it
  raises, and how long it took.

The reference checks a call's timeout only every 3 seconds
(`HydrusSubprocess.SubprocessCommunicate`), so a 1 second timeout is
recorded as the 3 or so seconds it takes. A long-lived call is spawned and
returns at once; in the box it is run with a forced 15 second timeout.
"""
import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'external_call_process_options.json')

BYTES_SCRIPT = "printf 'caf\\303\\251 \\377'; printf 'err \\303\\251' >&2; exit 2"

# name, script, timeout, long_lived, hide_terminal, text
CASES = [
    ('times out', 'echo started; exec sleep 30', 1, False, True, True),
    ('inside its timeout', 'echo fine', 5, False, True, True),
    ('bad bytes, text', BYTES_SCRIPT, 15, False, True, True),
    ('bad bytes, not text', BYTES_SCRIPT, 15, False, True, False),
    ('terminal shown', 'echo fine', 5, False, False, True),
    ('long lived, fails', 'exit 3', 1, True, True, True),
    ('long lived, runs past its timeout', 'sleep 2; echo done', 1, True, True, True),
]


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client.executables import ClientExecutableActualCall as A
    from hydrus.client.executables import ClientExecutablePipelines as P
    from hydrus.client.gui.panels.options.ExternalProgramsPanel import TestCallablePanel
    c = session.controller

    def make(script, timeout, long_lived, hide_terminal, text):
        call = A.ExecutableLocalProcessCall('sh', ['-c', script, '%path%'], [A.LocalProcessCallInputParameterProcessingRule(P.PARAMETER_TYPE_FILE_PATH)])
        call.SetTimeout(timeout)
        call.SetThisIsAPotentiallyLongLivedExternalGuy(long_lived)
        call.SetHideTerminal(hide_terminal)
        call.SetText(text)
        return call

    def describe(call):
        return dict(executable=call.GetExecutablePath(), arguments=call.GetExecutableParameterTemplates(),
                    parameters=[r.parameter_type for r in call.GetInputParameterProcessingRules()],
                    long_lived=call.GetThisIsAPotentiallyLongLivedExternalGuy(), timeout=call.GetTimeout(),
                    hide_terminal=call.GetHideTerminal(), text=call.GetText())

    def work():
        out = []
        panel = TestCallablePanel(c.gui)
        for (name, script, timeout, long_lived, hide_terminal, text) in CASES:
            call = make(script, timeout, long_lived, hide_terminal, text)
            panel.SetActualCall(call)
            for p in panel._input_param_types_to_edit_panels.values():
                p._test_value.setText('/synthetic/file.png')
            started = time.time()
            panel._test_call_button.click()
            interim = panel._raw_output_text_box.toPlainText()
            deadline = time.time() + 40
            while not panel._test_call_button.isEnabled() and time.time() < deadline:
                QW.QApplication.processEvents()
                time.sleep(0.02)
            for _ in range(5):
                QW.QApplication.processEvents()
            seconds = time.time() - started
            box = dict(interim=interim, output=panel._raw_output_text_box.toPlainText(), seconds=seconds)

            # the real launch, without the box's forced timeout
            started = time.time()
            error = None
            try:
                call.Call({P.PARAMETER_TYPE_FILE_PATH: '/synthetic/file.png'})
            except Exception as e:
                error = dict(type=type(e).__name__, text=str(e))
            launch = dict(error=error, seconds=time.time() - started)
            out.append(dict(name=name, call=describe(call), box=box, launch=launch))
        panel.deleteLater()

        # a long-lived launch really is left running: it writes a file after it has returned
        with tempfile.TemporaryDirectory() as folder:
            written = os.path.join(folder, 'late.txt')
            call = A.ExecutableLocalProcessCall('sh', ['-c', 'sleep 1; echo late > "$0"', '%path%'], [A.LocalProcessCallInputParameterProcessingRule(P.PARAMETER_TYPE_FILE_PATH)])
            call.SetThisIsAPotentiallyLongLivedExternalGuy(True)
            started = time.time()
            call.Call({P.PARAMETER_TYPE_FILE_PATH: written})
            returned = time.time() - started
            existed_at_return = os.path.exists(written)
            deadline = time.time() + 10
            while not os.path.exists(written) and time.time() < deadline:
                time.sleep(0.05)
            late = dict(returned_in=returned, existed_at_return=existed_at_return, existed_later=os.path.exists(written))
        return dict(cases=out, late=late)

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
