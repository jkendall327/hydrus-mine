#!/usr/bin/env python3
"""Record real Qt parsing raw previews, clipboard controls and fetched MIME data.

Documents and image bytes are generated locally; NetworkJob completion is scripted,
so no request leaves the synthetic reference URL. The test panel, detection,
clipboard and context callbacks are the real reference consumers.
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
        from PIL import Image
        import io
        from hydrus.client.gui.parsing import ClientGUIParsingTest as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client.gui import ClientGUIDialogsQuick as D, ClientGUIDialogsMessage as M
        from hydrus.client.networking import ClientNetworkingJobs as J
        from hydrus.core import HydrusExceptions
        image = io.BytesIO()
        Image.new('RGB', (2, 2), (18, 52, 86)).save(image, format='PNG')
        image_bytes = image.getvalue()
        panel = G.TestPanel(c.gui, lambda: P.ParseFormulaStatic(static_text='parsed'),
            P.ParsingTestData({'url':'https://raw-preview.example/original','post_index':'7','custom':'kept'}, ('original',)))
        QW.QApplication.processEvents()
        original = (c.pub, c.GetClipboardText, M.ShowCritical, D.EnterText, J.NetworkJob,
            c.network_engine.AddJob, c.CallToThread, c.CallAfterQtSafe)
        copied, messages, workers, jobs = [], [], [], []
        c.pub = lambda *args, **kwargs: copied.append(list(args)) if args and args[0] == 'clipboard' else None
        M.ShowCritical = lambda owner, title, text: messages.append([title, text])
        c.CallToThread = lambda fn, *args: workers.append(lambda: fn(*args))
        c.CallAfterQtSafe = lambda owner, fn, *args: fn(*args)
        c.network_engine.AddJob = lambda job: jobs.append(job)
        D.EnterText = lambda *args, **kwargs: ' https://raw-preview.example/fetched '
        cancelled_next = [False]
        def network_job(*args, **kwargs):
            job = original[4](*args, **kwargs)
            job.WaitUntilDone = (lambda: (_ for _ in ()).throw(HydrusExceptions.CancelledException())) if cancelled_next[0] else (lambda: None)
            job.GetContentText = lambda: 'binary image text'
            job.GetContentBytes = lambda: image_bytes
            return job
        J.NetworkJob = network_job
        out = {'states':[], 'copied':copied, 'messages':messages, 'png_hex':image_bytes.hex(), 'notifications':[]}
        panel._paste_button.ShowMicroNotification = lambda message: out['notifications'].append(message)
        def snapshot(name, **input):
            text = panel._example_data_raw_preview.toPlainText()
            out['states'].append(dict(case=name,input=input,description=panel._example_data_raw_description.text(),
                preview=text if len(text)<1000 else None,preview_length=len(text),preview_start=text[:100],
                parse_enabled=panel._test_parse.isEnabled(),raw_length=len(panel._example_data_raw),
                context=dict(panel.GetExampleParsingContext())))
        try:
            for name, text in [('empty',''),('json','{"z":"日本","html":"<html>embedded</html>","values":[1,true,null]}'),
                    ('html','<!DOCTYPE html><html><body>日本</body></html>'),('fragment','<p>fragment</p>'),
                    ('plain','plain 日本 text'),('json_scalar','"日本"'),('non_finite','[NaN, Infinity, -Infinity]')]:
                panel._SetExampleData(text)
                snapshot(name, text=text)
            panel._SetExampleData('日本'*250001)
            snapshot('clipped', repeat='日本', count=250001)
            panel._SetExampleData('binary image text', example_bytes=image_bytes)
            snapshot('media', text='binary image text', mime='png')
            c.GetClipboardText = lambda: '{"clipboard":"日本"}'
            panel._Paste(); snapshot('paste', text='{"clipboard":"日本"}')
            panel._Copy()
            c.GetClipboardText = lambda: (_ for _ in ()).throw(HydrusExceptions.DataMissing('synthetic clipboard error'))
            panel._Paste(); snapshot('paste_error')
            panel._FetchFromURL(); workers.pop(0)(); snapshot('fetch_media', text='binary image text', mime='png')
            out['fetch_url'] = jobs[-1].GetURL()
            cancelled_next[0] = True
            panel._FetchFromURL()
            workers.pop(0)(); snapshot('fetch_cancelled', text='fetch cancelled')
            panel.resize(900, 650); panel.show(); QW.QApplication.processEvents()
            panel.grab().save(os.path.join(HERE, 'fixtures/parser_raw_preview.png'))
        finally:
            (c.pub, c.GetClipboardText, M.ShowCritical, D.EnterText, J.NetworkJob,
             c.network_engine.AddJob, c.CallToThread, c.CallAfterQtSafe) = original
            panel.deleteLater()
        return out
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f: json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output=os.path.join(directory,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f: result=json.load(f)
    with open(os.path.join(HERE,'fixtures/parser_raw_preview.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('wrote parser_raw_preview.json')

if __name__=='__main__':main()
