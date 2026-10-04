#!/usr/bin/env python3
"""Record real typed router/subsidiary PNG export parameters and carrier roundtrips."""
import json
import os
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)

def record(session):
    c=session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUISerialisable as G
        from hydrus.client import ClientSerialisable as PNG
        from hydrus.core import HydrusSerialisable as HS, HydrusCompression as C
        from PIL import Image
        with open(os.path.join(HERE,'fixtures/router_exchange.json')) as f: routers=json.load(f)
        with open(os.path.join(HERE,'fixtures/subsidiary_exchange.json')) as f: subsidiaries=json.load(f)
        inputs=[('router_single',routers['exports'][1]),('router_queue',routers['queues'][1]['text']),
                ('subsidiary_single',subsidiaries['single']),('subsidiary_queue',subsidiaries['bundle'])]
        previous=c.new_options.GetNoneableString('last_png_export_dir')
        old_later=c.CallLaterQtSafe
        c.CallLaterQtSafe=lambda *args,**kwargs: None
        result=[]
        try:
            for name,tuple in inputs:
                c.new_options.SetNoneableString('last_png_export_dir',None)
                obj=HS.CreateFromSerialisableTuple(tuple)
                panel=G.PNGExportPanel(c.gui,obj)
                row={'case':name,'payload':json.loads(obj.DumpToString()),'default_title':panel._title.text(),
                    'summary':panel._payload_description.text(),'default_description':panel._text.text(),
                    'default_width':panel._width.value(),'default_path':panel._filepicker.GetPath()}
                panel._title.setText('recorded queue 日本')
                panel._text.setText('synthetic typed export')
                panel._width.setValue(300)
                base=os.path.join(HERE,'fixtures/parser_png_'+name)
                panel._filepicker.SetPath(base)
                panel._Update()
                row['can_export']=panel._export.isEnabled()
                panel.Export()
                row['label_after_export']=panel._export.text()
                row['loaded']=json.loads(C.DecompressBytesToString(PNG.LoadFromPNG(base+'.png')))
                with Image.open(base+'.png') as image:row['width']=image.width
                row['saved_directory']=c.new_options.GetNoneableString('last_png_export_dir')==os.path.dirname(base)
                if name=='router_queue':
                    panel.resize(720,380);panel.show();QW.QApplication.processEvents()
                    panel.grab().save(os.path.join(HERE,'fixtures/parser_png_export_editor.png'))
                panel.deleteLater();result.append(row)
        finally:
            c.new_options.SetNoneableString('last_png_export_dir',previous)
            c.CallLaterQtSafe=old_later
        return result
    return c.CallBlockingToQt(c.gui,work)

def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output=os.path.join(directory,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures/parser_png_export.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('wrote parser_png_export.json')
if __name__=='__main__':main()
