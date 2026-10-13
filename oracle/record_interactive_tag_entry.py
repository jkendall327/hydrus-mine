#!/usr/bin/env python3
"""Record the reference's interactive tag entry shortcut command.

Fresh temporary basic client. The real ApplicationCommandWidget (for the
'media' set) is asked for its command types and its interactive service
choices; the interactive command for 'my tags' is turned into text. Twelve
recent tags are pushed a moment apart. Then, for each case, the real
GetContentApplicationCommandFromInteractiveContentCommand runs with the real
quick dialog: the dialog's exec is replaced by a script that reads the title,
message, suggestion buttons, entry text and its selection, and then presses a
suggestion button, types and presses apply, or presses cancel on the real
widgets (the dialog is made modal, as exec would). The returned command is applied with the real
ApplyContentApplicationCommandToMedia (waiting for its worker), and the files'
tags and the recent tags (read with num_recent_tags 20) are recorded. A blank
or whitespace entry is refused with a message (captured) and then cancelled.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'interactive_tag_entry.json')


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
    from hydrus.client.gui.media import ClientGUIMediaModalActions as A
    from hydrus.client.gui.widgets import ClientGUIApplicationCommand as AC
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.metadata import ClientTags
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    local = next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName() == 'my tags')
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files'][:2]]
    recent = [f'recent:{i:02}' for i in range(12)]
    for tag in recent:
        c.WriteSynchronous('push_recent_tags', local, [tag])
        time.sleep(0.02)
    c.new_options.SetNoneableInteger('num_recent_tags', 20)

    def media():
        return [ClientMediaSingle.MediaSingle(r) for r in c.Read('media_results', hashes)]

    def tags_of(m):
        return sorted(m.GetTagsManager().GetCurrent(local, ClientTags.TAG_DISPLAY_STORAGE))

    def work():
        out = {'service': 'my tags', 'pushed': recent, 'files': [h.hex() for h in hashes]}
        widget = AC.ApplicationCommandWidget(c.gui, CAC.ApplicationCommand.STATICCreateSimpleCommand(CAC.SIMPLE_ARCHIVE_FILE), 'media')
        out['command_types'] = [widget._panel_choice.itemText(i) for i in range(widget._panel_choice.count())]
        choices = widget._interactable_service_key_panel._settable_services
        out['interactive_services'] = [choices.itemText(i) for i in range(choices.count())]
        widget._panel_choice.SetValue(widget.PANEL_INTERACTABLE_SERVICE_KEY)
        choices.SetValue(local)
        command = widget.GetValue()
        out['command_text'] = command.ToString()
        out['command_round_trip'] = AC.ApplicationCommandWidget(c.gui, command, 'media')._panel_choice.currentText()
        widget.deleteLater()
        cases = [
            dict(name='button', quick=10, act=('button', 2), files=[0]),
            dict(name='typed', quick=10, act=('type', 'typed:new tag'), files=[0, 1]),
            dict(name='default', quick=3, act=('apply', None), files=[0]),
            dict(name='already present', quick=3, act=('type', 'typed:new tag'), files=[0, 1]),
            dict(name='partly present', quick=3, act=('type', 'typed:second'), files=[1]),
            dict(name='partly present apply', quick=3, act=('type', 'typed:second'), files=[0, 1]),
            dict(name='cancel', quick=3, act=('cancel', None), files=[0]),
            dict(name='blank', quick=1, act=('type', ''), files=[0]),
            dict(name='whitespace', quick=1, act=('type', '   '), files=[0]),
            dict(name='do not show', quick=None, act=('type', 'typed:hidden'), files=[0]),
            dict(name='one', quick=1, act=('button', 0), files=[1]),
            dict(name='unclean', quick=2, act=('type', ' Typed:Spaced  OUT '), files=[1]),
            dict(name='decayed by main count', quick=10, main=4, act=('cancel', None), files=[0]),
        ]
        return out, command, cases
    out, command, cases = c.CallBlockingToQt(c.gui, work)
    import threading
    from hydrus.client.gui import ClientGUIDialogsMessage as DM
    old_exec, old_thread = W.DialogEdit.exec, c.CallToThread
    old_critical, old_warning = DM.ShowCritical, DM.ShowWarning
    finished = threading.Event()
    warnings = []
    DM.ShowCritical = lambda win, title, message: warnings.append(dict(kind='critical', title=title, message=message))
    DM.ShowWarning = lambda win, message: warnings.append(dict(kind='warning', message=message))

    def call_to_thread(f, *a, **kw):
        if getattr(f, '__name__', '') != 'do_it':
            return old_thread(f, *a, **kw)
        def run():
            try:
                f(*a, **kw)
            finally:
                finished.set()
        return old_thread(run)
    c.CallToThread = call_to_thread
    steps = []
    try:
        for case in cases:
            c.new_options.SetNoneableInteger('num_recent_tags', case.get('main', 20))
            c.new_options.SetNoneableInteger('num_recent_tags_in_quick_dialog', case['quick'])
            seen = {}

            def scripted(dlg, case=case, seen=seen):
                dlg.setModal(True)
                panel = dlg._panel
                buttons = [b for b in panel.widget().findChildren(QW.QPushButton)]
                labels = [w.text() for w in panel.widget().findChildren(QW.QLabel)]
                line = panel._text
                seen.update(title=dlg.windowTitle(), message=labels, suggestions=[b.text() for b in buttons],
                            text=line.text(), selected=line.selectedText(),
                            dialog_buttons=[dlg._apply.text(), dlg._cancel.text()])
                kind, value = case['act']
                if kind == 'button':
                    buttons[value].click()
                elif kind == 'type':
                    line.setText(value)
                    dlg._apply.click()
                    if dlg.result() != QW.QDialog.DialogCode.Accepted:
                        # the apply was refused with a message; the user then cancels
                        seen['refused'] = list(warnings)
                        dlg._cancel.click()
                elif kind == 'apply':
                    dlg._apply.click()
                else:
                    dlg._cancel.click()
                return dlg.result()
            W.DialogEdit.exec = scripted
            finished.clear()
            warnings.clear()

            def ask(case=case, seen=seen):
                ms = media()
                chosen = [ms[i] for i in case['files']]
                result = A.GetContentApplicationCommandFromInteractiveContentCommand(c.gui, command, chosen[0])
                step = dict(name=case['name'], quick=case['quick'], main=case.get('main', 20), act=list(case['act']), files=case['files'], dialog=dict(seen),
                            returned_content=result.IsContentCommand())
                if result.IsContentCommand():
                    step['content_text'] = result.ToString()
                    step['content_value'] = result.GetContentValue()
                    A.ApplyContentApplicationCommandToMedia(c.gui, result, chosen)
                return step
            step = c.CallBlockingToQt(c.gui, ask)
            if step['returned_content']:
                step['applied'] = finished.wait(10)
            time.sleep(0.2)
            c.new_options.SetNoneableInteger('num_recent_tags', 20)
            step['tags_after'] = [tags_of(m) for m in media()]
            step['recent_after'] = list(c.Read('recent_tags', local))
            steps.append(step)
            time.sleep(0.02)
    finally:
        W.DialogEdit.exec, c.CallToThread = old_exec, old_thread
        DM.ShowCritical, DM.ShowWarning = old_critical, old_warning
    out['steps'] = steps
    return out


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()
