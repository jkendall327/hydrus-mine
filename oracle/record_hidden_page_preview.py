#!/usr/bin/env python3
"""Record actual owned Qt preview identity and intervals across hide/page boundaries."""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    from qtpy import QtWidgets as QW, QtTest
    from hydrus.core import HydrusConstants as HC, HydrusTime
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.media import ClientMediaSingle
    c = session.controller
    def drive():
        root = c.gui._notebook
        c.gui.resize(1400, 1000)
        HC.options['hpos'] = 400; HC.options['vpos'] = -240; HC.options['hide_preview'] = False
        context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        old = list(range(root.count()))
        one = root.NewPageQuery(context, page_name='A', select_page=True)
        two = root.NewPageQuery(context, page_name='B', select_page=False)
        root._ClosePages(old, 'recording', polite=False)
        def settle():
            for _ in range(3): QW.QApplication.processEvents(); QtTest.QTest.qWait(20)
        def select(page): root.setCurrentWidget(page); settle()
        select(one)
        manifest=json.loads((HERE/'fixtures'/'legacy_db'/'basic.manifest.json').read_text())
        hashes=[next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name']==name) for name in ('jpeg_00.jpg','jpeg_01.jpg')]
        media=[ClientMediaSingle.MediaSingle(c.Read('media_results',[h])[0]) for h in hashes]
        now=[1000]; intervals=[]; steps=[]
        original_time=HydrusTime.GetNowMS
        manager=c.file_viewing_stats_manager
        original_finish=manager.FinishViewing
        HydrusTime.GetNowMS=lambda:now[0]
        manager.FinishViewing=lambda result,canvas,start,elapsed:intervals.append(dict(hash=result.GetHash().hex(),canvas=canvas,start=start,elapsed=elapsed))
        def state(page):
            canvas=page._preview_canvas
            def hash_of(value):return None if value is None else value.GetHash().hex()
            window=canvas._media_container._media_window
            renderer=getattr(window,'_image_renderer',None)
            return dict(current=hash_of(canvas._current_media),hidden=hash_of(canvas._hidden_page_current_media),start=canvas._current_media_start_time_ms,visible=canvas.isVisible(),splitter_hidden=canvas._is_splitter_hidden,raster_rendered=None if window is None or not hasattr(window,'IsRendered') else window.IsRendered(),raster_ready=None if renderer is None else renderer.IsReady())
        def snap(action):settle();steps.append(dict(action=action,now=now[0],hide=HC.options['hide_preview'],shown=root.GetCurrentMediaPage().GetName(),A=state(one),B=state(two),intervals=list(intervals)))
        try:
            one._preview_canvas.SetMedia(media[0]);admission=state(one);snap('A accepts first')
            now[0]=1500;HC.options['hide_preview']=True;snap('enable hide keeps accepted A')
            now[0]=2000;select(two);snap('A to empty B while hide')
            two._preview_canvas.SetMedia(media[1]);snap('empty B rejects new media')
            now[0]=3000;select(one);snap('return A while hide retains accepted A')
            one.grab().save(str(HERE/'fixtures'/'hidden_page_preview_reference.png'))
            one._preview_canvas.ClearMedia();snap('clear rejected on returned A')
            now[0]=4000;select(two);HC.options['hide_preview']=False
            two.SetSplitterPositions(400,-240);settle();two._preview_canvas.SetSplitterHiddenStatus(False)
            two._preview_canvas.SetMedia(media[1]);snap('B accepts second after disable')
            HC.options['hide_preview']=True;now[0]=5000;select(one);snap('A and B both retain owned accepted media')
            now[0]=5500;c.gui.hide();snap('whole window hidden while globally hidden')
            now[0]=5600;c.gui.show();settle();snap('whole window shown retains accepted canvases')
            now[0]=5700;root._ClosePages([root.indexOf(two)],'recording',polite=False);snap('closed live B retains accepted canvas')
            now[0]=5800;c.gui._UnclosePage();settle();select(one);snap('reopened live B retains accepted canvas')
            now[0]=6000;one.SetSplitterPositions(400,0);one._preview_canvas.SetSplitterHiddenStatus(True);snap('splitter clear rejected while hide')
            now[0]=7000;HC.options['hide_preview']=False;snap('disable global hide while still collapsed retains accepted A')
            boundary=root.NewPageQuery(context,page_name='collapsed boundary',select_page=True);settle();select(one);snap('switch away and return while collapsed preserves A')
            one.SetSplitterPositions(400,-240);settle();one._preview_canvas.SetSplitterHiddenStatus(False);snap('reveal after disable')
            one._preview_canvas.ClearMedia();snap('accepted clear ends A interval')
            now[0]=8000;select(two);snap('normal show restores B')
            now[0]=9000;select(one);snap('normal hide finishes B and retains restoration identity')
            now[0]=10000;select(two);snap('normal show starts B successor interval')
            # A new Page built from the real saved manager has no accepted canvas.
            fresh=root.NewPage(two.GetPageManager().Duplicate(),select_page=True);settle()
            fresh_state=state(fresh)
            HC.options['hide_preview']=True
            fresh._preview_canvas.SetMedia(media[1]);settle()
            fresh_hidden=state(fresh)
            # Real forgotten-page cleanup calls ClearMedia, which global hide
            # rejects even at CleanBeforeDestroy; record that source boundary.
            now[0]=11000;HC.options['hide_preview']=False
            cleanup=root.NewPageQuery(context,page_name='cleanup',select_page=True);settle()
            cleanup.SetSplitterPositions(400,-240);settle();cleanup._preview_canvas.SetSplitterHiddenStatus(False)
            cleanup._preview_canvas.SetMedia(media[1]);settle();HC.options['hide_preview']=True
            forgotten_before=state(cleanup);cleanup.CleanBeforeDestroy();forgotten_after=state(cleanup)
            fresh.grab().save(str(HERE/'fixtures'/'hidden_page_preview_fresh_reference.png'))
            return dict(admission_before_event_settle=admission,hashes=[h.hex() for h in hashes],steps=steps,fresh_page=fresh_state,fresh_hidden=fresh_hidden,forgotten_before=forgotten_before,forgotten_after=forgotten_after,limits=['FinishViewing is captured at the real canvas boundary; manager/database cap and minimum rules are covered by preview_viewing fixtures.','Fresh reference pages receive a fresh PageKey; native session reuse of the same key is separately guarded by SearchPage identity.','Reference image decode is asynchronous; current accepted identity is sampled independently of raster readiness.'])
        finally:
            HydrusTime.GetNowMS=original_time;manager.FinishViewing=original_finish
    return c.CallBlockingToQt(c.gui,drive)

def child(out):
    import hydrus_driver,record_api
    Path(out).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp)/'out.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    dest=HERE/'fixtures'/'hidden_page_preview.json';dest.write_text(json.dumps(result,indent=2)+'\n');print(f'wrote {dest}')
if __name__=='__main__':main()
