#!/usr/bin/env python3
"""Record the real page parser's fetch/error ownership and show/copy handlers.

Real Qt page, control and NetworkJob objects run their owner callbacks. Network
completion is deterministic, no HTTP traffic is sent, and URLs are synthetic.
Python tracebacks are retained verbatim in Qt; native diagnostics use Rust errors.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui.parsing import ClientGUIParsing as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client.gui import ClientGUICore as CGC, ClientGUIDialogsMessage as M
        from hydrus.client.networking import ClientNetworkingJobs as J
        queued, workers, copied, messages, menus = [], [], [], [], []
        old_job, old_add, old_thread, old_after = J.NetworkJob, c.network_engine.AddJob, c.CallToThread, c.CallAfterQtSafe
        old_pub, old_show, old_popup = c.pub, M.ShowCritical, CGC.core().PopupMenu
        def job(*args, **kwargs):
            result = old_job(*args, **kwargs)
            result.WaitUntilDone = lambda: (_ for _ in ()).throw(RuntimeError('synthetic failure'))
            result.GetContentText = lambda: 'server detail'
            result.GetContentBytes = lambda: b'server detail'
            return result
        J.NetworkJob = job
        c.network_engine.AddJob = lambda job: queued.append(job)
        c.CallToThread = lambda fn, *args: workers.append(lambda: fn(*args))
        c.pub = lambda *args, **kwargs: copied.append(list(args)) if args and args[0] == 'clipboard' else None
        M.ShowCritical = lambda owner, title, text: messages.append([title, text])
        CGC.core().PopupMenu = lambda owner, menu: menus.append([a.text() for a in menu.actions()])
        panel = G.EditPageParserPanel(c.gui, P.PageParser(name="error owner"))
        QW.QApplication.processEvents()
        panel._test_panel._SetExampleData('original')
        c.CallAfterQtSafe = lambda owner, fn, *args: fn(*args)
        control = panel._test_network_job_control
        out = {'states': [], 'menus': menus, 'copied': copied, 'messages': messages}
        def snapshot(name):
            out['states'].append({'case': name, 'has_job': control.HasNetworkJob(),
                'has_error': control._error_text is not None, 'error_hidden': control._error_button.isHidden(),
                'error_tail': control._error_text.splitlines()[-1] if control._error_text else None})
        try:
            control.SetError('previous failure')
            panel._test_url.setText('')
            panel._FetchExampleData()
            snapshot('empty_url_keeps_error')
            panel._test_url.setText('https://fetch-errors.example/document')
            panel._FetchExampleData()
            snapshot('new_request_clears_error')
            workers.pop(0)()
            snapshot('completed_failure_retains_error_without_job')
            out['failure_document'] = panel._test_panel._example_data_raw
            out['context'] = panel._test_panel.GetExampleParsingContext()
            control._ShowErrorMenu()
            control.ShowError()
            control.CopyError()
            panel.resize(1600, 900); panel.show(); QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE, 'fixtures/parser_fetch_errors.png'))
            panel._FetchExampleData()
            snapshot('next_request_clears_error')
            queued[-1].WaitUntilDone = lambda: None
            workers.pop(0)()
            snapshot('success_without_error')
            out['success_document'] = panel._test_panel._example_data_raw
            assert copied[0][2] == messages[0][1]
            out['show_copy_identical'] = True
        finally:
            control.ClearNetworkJob()
            panel.deleteLater()
            J.NetworkJob, c.network_engine.AddJob, c.CallToThread, c.CallAfterQtSafe = old_job, old_add, old_thread, old_after
            c.pub, M.ShowCritical, CGC.core().PopupMenu = old_pub, old_show, old_popup
        return out
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
    with tempfile.TemporaryDirectory() as directory:
        output = os.path.join(directory, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', output)
        with open(output) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/parser_fetch_errors.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote parser_fetch_errors.json')

if __name__ == '__main__':
    main()
