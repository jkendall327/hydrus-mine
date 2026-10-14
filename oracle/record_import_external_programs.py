#!/usr/bin/env python3
"""Record the reference's external programs import options: the list editor,
its entry editor, and the programs run after a file is imported.

The real `EditExternalProgramsImportOptionsPanel` (and, in its `DialogEdit`,
the real single-entry panel) is driven with scripted answers to the dialog,
against an executable manager whose "send single file" calls are registered
here. The real `FileSeed.DoExternalProgramCalls` then runs those calls for
each import status, through the real `SendSingleFile` and a real subprocess,
which writes what it was given to a log. A call that cannot start pauses the
importers, as the reference does; that and its message are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_external_programs.py
"""
import json
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

LOGGER = '''import json, sys
with open(sys.argv[1], "a") as f:
    f.write(json.dumps(sys.argv[2:]) + "\\n")
'''


def record(session):
    c = session.controller

    def qt():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.core import HydrusData, HydrusSerialisable as S
        from hydrus.client import ClientConstants as CC
        from hydrus.client.executables import ClientExecutableManager as M, ClientExecutableCallables as C, ClientExecutablePipelines as P, ClientExecutableActualCall as A
        from hydrus.client.gui import ClientGUIDialogsMessage as Message, ClientGUITopLevelWindowsPanels as Windows
        from hydrus.client.gui.importing import ClientGUIImportOptionsPanels as Panels
        from hydrus.client.importing import ClientImportFileSeeds as Seeds
        from hydrus.client.importing.options import ExternalProgramsImportOptions as EPIO, ImportOptionsContainer as IOC

        work = tempfile.mkdtemp()
        logger = os.path.join(work, 'logger.py')
        log = os.path.join(work, 'log.txt')
        Path(logger).write_text(LOGGER)

        def process(executable, templates, parameters):
            rules = [A.LocalProcessCallInputParameterProcessingRule(parameter_type=p) for p in parameters]
            return A.ExecutableLocalProcessCall(executable, templates, rules)

        parameters = [P.PARAMETER_TYPE_FILE_PATH, P.PARAMETER_TYPE_FILE_LOCAL_PATH_URI, P.PARAMETER_TYPE_FILE_HASH, P.PARAMETER_TYPE_FILE_HASH_ID]
        calls = []
        for key, name, pipeline, call in [
            (1, 'Zulu URL', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_URL, process(sys.executable, [logger, log, '%url%'], [P.PARAMETER_TYPE_URL])),
            (2, 'beta log', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE, process(sys.executable, [logger, log, 'beta', '%path%', '%path_uri%', '%hash%'], parameters[:3])),
            (6, 'gamma file id', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE, process(sys.executable, [logger, log, 'gamma', '%file_id%'], parameters[3:])),
            (3, 'Alpha 日本 log', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE, process(sys.executable, [logger, log, 'alpha', '%path%'], parameters[:1])),
            (4, 'broken', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE, process(os.path.join(work, 'no-such-program'), ['%path%'], parameters[:1])),
        ]:
            callable_ = C.ClientExecutableCallable(name, pipeline, call)
            callable_.SetCallableKey(bytes([key]) * 32)
            calls.append(callable_)
        os_file = C.ClientExecutableCallable('Default OS File Launch', P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE, A.ExecutableLocalProcessDefaultLaunchFile())
        os_file.SetCallableKey(bytes([5]) * 32)
        calls.append(os_file)
        manager = M.ExecutableManager()
        manager.SetCallables(calls)
        old_manager = c.executable_manager
        c.executable_manager = manager
        by_key = {k: next(x for x in calls if x.GetCallableKey() == bytes([k]) * 32).GetIdAndName() for k in (1, 2, 3, 4, 5, 6)}

        out = {}
        real_dialog = Windows.DialogEdit.exec
        real_critical, real_information = Message.ShowCritical, Message.ShowInformation
        messages = []
        Message.ShowCritical = lambda owner, title, text, **kw: messages.append(['critical', title, text])
        Message.ShowInformation = lambda owner, text, **kw: messages.append(['information', text])
        scripted = []

        def exec_dialog(dlg):
            panel = dlg._panel
            step = scripted.pop(0)
            seen = dict(
                title=dlg.windowTitle(),
                programs=[[panel._id_and_name.itemText(i), panel._id_and_name.itemData(i).object_id.hex()] for i in range(panel._id_and_name.count())],
                whens=[[panel._when_to_do_it.itemText(i), list(panel._when_to_do_it.itemData(i))] for i in range(panel._when_to_do_it.count())],
                initial=dict(program=panel._id_and_name.GetValue().object_id.hex(), when=list(panel._when_to_do_it.GetValue())),
            )
            steps_seen.append(seen)

            def respond():
                if step['accept']:
                    if 'program' in step:
                        panel._id_and_name.SetValue(by_key[step['program']])
                    if 'when' in step:
                        panel._when_to_do_it.SetValue(tuple(step['when']))
                    dlg.accept()
                else:
                    dlg.reject()
            QC.QTimer.singleShot(0, respond)
            return real_dialog(dlg)

        steps_seen = []
        Windows.DialogEdit.exec = exec_dialog

        def listing(panel):
            return [panel._EntryToStringForList(e) for e in panel._entries.GetData()]

        def tuple_of(panel):
            return json.loads(json.dumps(panel.GetValue().GetSerialisableTuple()))

        try:
            parent = QW.QWidget()
            panel = Panels.EditExternalProgramsImportOptionsPanel(parent, EPIO.ExternalProgramsImportOptions())
            events = []
            # an add, cancelled; then one of each "when"
            for label, step in [
                ('add-cancel', dict(accept=False)),
                ('add-default', dict(accept=True)),
                ('add-all-new-beta', dict(accept=True, program=2, when=[True, False])),
                ('add-already-in-alpha', dict(accept=True, program=3, when=[False, True])),
                ('add-all-os', dict(accept=True, program=5, when=[True, True])),
                ('add-none-broken', dict(accept=True, program=4, when=[False, False])),
            ]:
                scripted.append(step)
                try:
                    entry = panel._AddEntry()
                    panel._entries.AddDatas([entry])
                except Exception as e:
                    entry = None
                    events.append(dict(label=label + '-raised', error=type(e).__name__))
                events.append(dict(label=label, list=listing(panel), value=tuple_of(panel), seen=steps_seen[-1]))
            out['add_events'] = events
            out['summary'] = panel.GetValue().GetSummary(2)
            # an edit
            first = panel._entries.GetData()[0]
            scripted.append(dict(accept=True, program=3, when=[True, True]))
            edited = panel._EditEntry(first)
            out['edit'] = dict(seen=steps_seen[-1], before=panel._EntryToStringForList(first), after=panel._EntryToStringForList(edited), after_value=json.loads(json.dumps(edited.GetSerialisableTuple())))
            scripted.append(dict(accept=False))
            try:
                panel._EditEntry(first)
                out['edit_cancel'] = 'returned'
            except Exception as e:
                out['edit_cancel'] = type(e).__name__
            # an entry whose program is not in the list
            ghost = EPIO.ExternalProgramsImportOptionsSingleEntry()
            ghost.SetIdAndName(S.IdAndName(bytes([9]) * 32, 'gone program'))
            messages.clear()
            scripted.append(dict(accept=False))
            try:
                panel._EditEntry(ghost)
            except Exception as e:
                pass
            out['edit_missing'] = dict(messages=list(messages), seen=steps_seen[-1], summary=ghost.GetSummary(2))
            # no registered "send single file" calls at all
            c.executable_manager = M.ExecutableManager()
            messages.clear()
            try:
                panel._AddEntry()
                out['no_calls'] = dict(raised=None, messages=list(messages))
            except Exception as e:
                out['no_calls'] = dict(raised=type(e).__name__, messages=list(messages))
            c.executable_manager = manager
            # empty options
            out['empty_summary'] = EPIO.ExternalProgramsImportOptions().GetSummary(2)
            # ShouldFire
            fire = {}
            for new, already in [(True, True), (True, False), (False, True), (False, False)]:
                entry = EPIO.ExternalProgramsImportOptionsSingleEntry()
                entry.SetDoItOnNew(new)
                entry.SetDoItOnAlreadyIn(already)
                fire['%s,%s' % (new, already)] = {str(s): entry.ShouldFire(s) for s in range(0, 10)}
            out['should_fire'] = fire
            out['default_entry'] = json.loads(json.dumps(EPIO.ExternalProgramsImportOptionsSingleEntry().GetSerialisableTuple()))
            out['sorted_programs'] = [[i.name, i.object_id.hex()] for i in manager.GetIdsAndNamesOfType(P.EXECUTABLE_PIPELINE_TYPE_OPEN_EXTERNALLY_SINGLE_FILE)]

            # the programs run after an import
            manifest = json.loads((HERE / 'fixtures/legacy_db/basic.manifest.json').read_text())
            file_hash = next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name'] == 'jpeg_00.jpg')
            media = c.Read('media_results', [file_hash])[0]
            out['file'] = dict(hash=file_hash.hex(), id=media.GetHashId())

            def run(entries, status):
                options = EPIO.ExternalProgramsImportOptions()
                built = []
                for key, new, already in entries:
                    entry = EPIO.ExternalProgramsImportOptionsSingleEntry()
                    entry.SetIdAndName(by_key[key])
                    entry.SetDoItOnNew(new)
                    entry.SetDoItOnAlreadyIn(already)
                    built.append(entry)
                options.SetEntries(built)
                container = IOC.ImportOptionsContainer()
                container.SetImportOptions(options)
                seed = Seeds.FileSeed(Seeds.FILE_SEED_TYPE_HDD, '/synthetic/file.jpg')
                seed._hashes['sha256'] = file_hash
                seed.status = status
                if os.path.exists(log):
                    os.remove(log)
                shown = []
                real_show = HydrusData.ShowText
                # (the reference also prints the whole environment, which is not kept)
                HydrusData.ShowText = lambda text: shown.append(text) if text.startswith(('Failed to run', 'Got a file not found')) else None
                real_exc = HydrusData.ShowException
                HydrusData.ShowException = lambda e, **kw: shown.append('EXCEPTION ' + type(e).__name__)
                try:
                    seed.DoExternalProgramCalls(container)
                finally:
                    HydrusData.ShowText = real_show
                    HydrusData.ShowException = real_exc
                lines = [json.loads(line) for line in Path(log).read_text().splitlines()] if os.path.exists(log) else []
                return lines, shown

            options_obj = c.new_options
            flags = ['pause_import_folders_sync', 'pause_subs_sync', 'pause_all_file_queues']
            runs = []
            for label, entries, status in [
                ('new-all', [(2, True, True)], CC.STATUS_SUCCESSFUL_AND_NEW),
                ('redundant-all', [(2, True, True)], CC.STATUS_SUCCESSFUL_BUT_REDUNDANT),
                ('new-only-on-redundant', [(2, True, False)], CC.STATUS_SUCCESSFUL_BUT_REDUNDANT),
                ('already-only-on-new', [(2, False, True)], CC.STATUS_SUCCESSFUL_AND_NEW),
                ('two-in-order', [(3, True, False), (2, True, False)], CC.STATUS_SUCCESSFUL_AND_NEW),
                ('deleted', [(2, True, True)], CC.STATUS_DELETED),
                ('none', [(2, False, False)], CC.STATUS_SUCCESSFUL_AND_NEW),
            ]:
                lines, shown = run(entries, status)
                runs.append(dict(label=label, lines=lines, shown=shown, paused=[options_obj.GetBoolean(f) for f in flags]))
            # the reference's file id parameter is a number, which its string processing refuses
            lines, shown = run([(6, True, True)], CC.STATUS_SUCCESSFUL_AND_NEW)
            runs.append(dict(label='file-id-parameter', lines=lines, shown=shown, paused=[options_obj.GetBoolean(f) for f in flags]))
            for f in flags:
                options_obj.SetBoolean(f, False)
            # a program that cannot start pauses the importers and stops the rest
            lines, shown = run([(4, True, True), (2, True, True)], CC.STATUS_SUCCESSFUL_AND_NEW)
            runs.append(dict(label='broken-first', lines=lines, shown=shown, paused=[options_obj.GetBoolean(f) for f in flags]))
            for f in flags:
                options_obj.SetBoolean(f, False)
            # a registered program gone from the manager
            c.executable_manager = M.ExecutableManager()
            lines, shown = run([(2, True, True)], CC.STATUS_SUCCESSFUL_AND_NEW)
            runs.append(dict(label='program-missing', lines=lines, shown=shown, paused=[options_obj.GetBoolean(f) for f in flags]))
            for f in flags:
                options_obj.SetBoolean(f, False)
            c.executable_manager = manager
            out['runs'] = runs
            out['work'] = work
            out['db'] = c.GetDBDir()
            out['pause_flags'] = flags
            parent.deleteLater()
        finally:
            Windows.DialogEdit.exec = real_dialog
            Message.ShowCritical, Message.ShowInformation = real_critical, real_information
            c.executable_manager = old_manager
        return out

    return c.CallBlockingToQt(c.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        output = sys.argv[2]
        import record_api
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        return
    with tempfile.TemporaryDirectory() as work:
        output = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        text = Path(output).read_text()
        result = json.loads(text)
        # (the scratch directory and the client's file path are random)
        text = text.replace(result['work'], '{work}').replace(result['db'], '{db}')
        (HERE / 'fixtures/import_external_programs.json').write_text(text + '\n')


if __name__ == '__main__':
    main()
