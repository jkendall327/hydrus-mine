#!/usr/bin/env python3
"""Actual Qt external-call list/editor staging, clipboard codecs and local test call.

The only process executed is this recorder's Python interpreter writing an owned
local text fixture. OS launchers/default browser calls are inspected, never run.
The temporary basic reference client and source tree are otherwise untouched.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'external_calls.json')

def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client.executables import ClientExecutableActualCall as A, ClientExecutableCallables as C, ClientExecutableDefaults as D, ClientExecutablePipelines as P
    from hydrus.client.gui.panels.options.ExternalProgramsPanel import ExternalProgramsPanel, EditClientExecutableCallablePanel, EditProcessCallExecutableAndParametersPanel
    from hydrus.client.gui import ClientGUIDialogsQuick as Q
    from hydrus.core import HydrusSerialisable as S
    c = session.controller
    with tempfile.TemporaryDirectory() as folder:
        output_path = os.path.join(folder, 'owned-output.txt')
        code = 'import pathlib,sys; pathlib.Path(sys.argv[1]).write_text("synthetic 日本", encoding="utf-8")'
        call = C.ClientExecutableCallable('synthetic call', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE, A.ExecutableLocalProcessCall(sys.executable, ['-c', code, '%path%'], [A.LocalProcessCallInputParameterProcessingRule(0)]))
        call.SetCallableKey(bytes.fromhex('11' * 32))
        def work():
            questions = []
            old_yesno = Q.GetYesNo
            Q.GetYesNo = lambda _p, message, **kw: questions.append(dict(message=message, **kw)) or QW.QDialog.DialogCode.Rejected
            try:
                panel = ExternalProgramsPanel(c.gui, c.new_options.Duplicate())
                original = list(panel._external_calls.GetData())
                panel._AddCallableFullyFormed(call.Duplicate())
                panel._AddCallableFullyFormed(call.Duplicate())
                added = [x for x in panel._external_calls.GetData() if x.GetName().startswith('synthetic')]
                rows = [panel._ConvertCallableToDisplayTuple(x) for x in added]
                keys_fresh = [x.GetCallableKey() != call.GetCallableKey() for x in added]
                keys_unique = len({x.GetCallableKey() for x in added}) == len(added)
                # Actual detached callable editor, preserving identity on child acceptance.
                child = EditClientExecutableCallablePanel(panel, added[0])
                child._name.setText('edited 日本')
                child_value = child.GetValue()
                child_retains_key = child_value.GetCallableKey() == added[0].GetCallableKey()
                parent_before_child_accept = [x.GetName() for x in panel._external_calls.GetData()]
                panel._external_calls.ReplaceData(added[0], child_value, sort_and_scroll=True)
                saved_manager = panel.GetExecutableManager()
                saved_tuple = saved_manager.GetSerialisableTuple()
                reopened = S.CreateFromSerialisableTuple(saved_tuple)
                exported = S.SerialisableList([child_value, added[1]]).DumpToString()
                reimported = S.CreateFromString(exported)
                for x in reimported: panel._AddCallableViaImport(x)
                after_import = [x.GetName() for x in panel._external_calls.GetData()]
                weird = C.ClientExecutableCallable('weird', actual_call=A.ExecutableLocalProcessCall('x'*257))
                declined = None
                try: panel._AddCallableViaImport(weird)
                except Exception as e: declined = str(e)
                command = EditProcessCallExecutableAndParametersPanel(panel, sys.executable, ['  -c  ', ' a\t b\nc ', '  '])
                clean = command.GetValue()[1]
                command._executable_path.setText('')
                command_allowed = command.UserIsOKToOK()
                clean_cases = [[x, A.CleanExecutableParameterTemplates(x)] for x in [[], [''], ['  '], ['a\n\nb'], [' a\t b\nc '], ['%path%-%path%']]]
                preview_inputs = {0: '/synthetic folder/漢😀.png'}
                preview_call = A.ExecutableLocalProcessCall('owned-program', ['pre:%path%:%path%', '%path%'], [A.LocalProcessCallInputParameterProcessingRule(0)])
                preview = preview_call.GetCommandPreviewWithInputParams(preview_inputs)
                missing_preview = preview_call.GetCommandPreviewWithInputParams({})
                unused_call = A.ExecutableLocalProcessCall('owned-program', ['literal'], [A.LocalProcessCallInputParameterProcessingRule(0)])
                unused_preview = unused_call.GetCommandPreviewWithInputParams(preview_inputs)
                # Actual typed call test; no shell and no third-party executable.
                availability = call.GetCall().TestAvailability()
                call.GetCall().CallTest({0: output_path})
                result = dict(original_rows=[panel._ConvertCallableToDisplayTuple(x) for x in original], rows=rows, keys_fresh=keys_fresh, keys_unique=keys_unique, child_retains_key=child_retains_key, parent_before_child_accept=parent_before_child_accept, saved_manager=saved_tuple, reopened_names=[x.GetName() for x in reopened.GetCallables()], export=exported, after_import=after_import, declined=declined, command_allowed=command_allowed, questions=questions, clean=clean, clean_cases=clean_cases, preview=preview, missing_preview=missing_preview, unused_preview=unused_preview, availability=availability, process_output=open(output_path, encoding='utf-8').read(), defaults=[x.GetSerialisableTuple() for x in D.GetAllDefaults(False)], platform_defaults=[x.GetName() for x in D.GetAllDefaults(True)])
                child.resize(1050, 760); child.show()
                for _ in range(4): QW.QApplication.processEvents()
                child.grab().save(OUT.replace('.json', '.png'))
                child.hide(); child.deleteLater(); command.deleteLater(); panel.deleteLater()
                return result
            finally: Q.GetYesNo = old_yesno
        return c.CallBlockingToQt(c.gui, work)

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(OUT, 'w') as f: json.dump(result, f, indent=2); f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()
