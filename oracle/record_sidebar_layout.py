#!/usr/bin/env python3
"""Record actual Qt Page splitters, frame save/restore actions and preview policy."""
import json
import sys
import tempfile
from pathlib import Path
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

def record(session):
    from qtpy import QtCore as QC, QtWidgets as QW, QtTest
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.media import ClientMediaSingle
    from hydrus.client.gui.panels.options.GUIPagesPanel import GUIPagesPanel
    c = session.controller
    def drive():
        root = c.gui._notebook
        c.gui.resize(1400, 1000)
        HC.options['hpos'] = 400
        HC.options['vpos'] = -240
        HC.options['hide_preview'] = False
        c.new_options.SetBoolean('saving_sash_positions_on_exit', True)
        context = ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        old = list(range(root.count()))
        one = root.NewPageQuery(context, page_name='one', select_page=True)
        two = root.NewPageQuery(context, page_name='two', select_page=False)
        root._ClosePages(old, 'recording', polite=False)
        def settle():
            for _ in range(3): QW.QApplication.processEvents(); QtTest.QTest.qWait(20)
        def select(page):
            root.setCurrentWidget(page); settle(); page.PageShown(); settle()
        def state(page):
            return dict(sash=list(page.GetSashPositions()), horizontal=page._sidebar_media_split.sizes(), vertical=page._management_preview_split.sizes(), sidebar_visible=page._management_preview_split.isVisible(), preview_visible=page._preview_panel.isVisible(), preview_splitter_hidden=page._preview_canvas._is_splitter_hidden)
        steps=[]
        def snap(action):
            settle(); steps.append(dict(action=action,current=root.GetCurrentMediaPage().GetName(),saved=[HC.options['hpos'],HC.options['vpos']],hide_preview=HC.options['hide_preview'],save_on_exit=c.new_options.GetBoolean('saving_sash_positions_on_exit'),one=state(one),two=state(two)))
        select(one);snap('initial')
        # QTest sends an actual handle press/move/release to the real splitter.
        def drag(split, delta):
            handle=split.handle(1);start=handle.rect().center()
            QtTest.QTest.mousePress(handle,QC.Qt.MouseButton.LeftButton,pos=start)
            QtTest.QTest.mouseMove(handle,start+delta)
            QtTest.QTest.mouseRelease(handle,QC.Qt.MouseButton.LeftButton,pos=start+delta)
            settle()
        drag(one._sidebar_media_split,QC.QPoint(70,0));snap('drag sidebar')
        drag(one._management_preview_split,QC.QPoint(0,-55));snap('drag preview')
        c.gui._ShowHideSplitters();snap('hide unsaved preview')
        c.gui._SaveSplitterPositions();snap('save hidden unsaved preview')
        HC.options['hpos']=400;HC.options['vpos']=-240
        one.SetSplitterPositions(470,-295);settle()
        select(two);two.SetSplitterPositions(460,-280);snap('resize second')
        select(one);snap('return first retains sizes')
        c.gui._SaveSplitterPositions();snap('save current now')
        three=root.NewPageQuery(context,page_name='three',select_page=True);settle();three.PageShown();settle()
        new_page=state(three)
        select(one);c.gui._ShowHideSplitters();snap('hide sidebar')
        c.gui._SaveSplitterPositions();snap('save while hidden')
        c.gui._ShowHideSplitters();snap('reveal uses saved')
        # Restore broadcasts asynchronously; record actual pages after dispatch.
        HC.options['hpos']=420;HC.options['vpos']=-260
        c.gui._RestoreSplitterPositions();settle();snap('restore all')
        panel=GUIPagesPanel(c.gui, c.new_options.Duplicate())
        panel._hide_preview.setChecked(True)
        before=HC.options['hide_preview']
        del panel # abandoned Options draft
        snap('cancel hide preference')
        manifest=json.loads((HERE/'fixtures'/'legacy_db'/'basic.manifest.json').read_text())
        hashes=[next(bytes.fromhex(f['hash']) for f in manifest['files'] if f['name']==name) for name in ('jpeg_00.jpg','jpeg_01.jpg')]
        media=[ClientMediaSingle.MediaSingle(c.Read('media_results',[h])[0]) for h in hashes]
        canvas=one._preview_canvas
        canvas.SetMedia(media[0]);settle()
        def displayed():return None if canvas._current_media is None else canvas._current_media.GetHash().hex()
        consumer=[dict(action='accepted before preference',shown=displayed())]
        panel=GUIPagesPanel(c.gui, c.new_options.Duplicate());panel._hide_preview.setChecked(True);panel.UpdateOptions();settle()
        canvas.SetMedia(media[1]);consumer.append(dict(action='new media rejected while hidden globally',shown=displayed()))
        canvas.ClearMedia();consumer.append(dict(action='clear rejected while hidden globally',shown=displayed()))
        snap('apply hide preference retains existing split')
        c.gui._RestoreSplitterPositions();settle();snap('restore obeys hide preference');consumer.append(dict(action='restore hide retains accepted media',shown=displayed()))
        hidden_new=root.NewPageQuery(context,page_name='hidden new',select_page=True);settle();hidden_new.PageShown();settle()
        hidden_new_state=state(hidden_new)
        select(two);consumer.append(dict(action='other page cannot display first page media',shown=None if two._preview_canvas._current_media is None else two._preview_canvas._current_media.GetHash().hex()))
        select(one);one.EventPreviewUnsplit(None);snap('preview double click clear');consumer.append(dict(action='collapsed hide retains accepted media',shown=displayed()))
        HC.options['hide_preview']=False;c.gui._RestoreSplitterPositions();settle();snap('restore visible preview')
        c.new_options.FlipBoolean('saving_sash_positions_on_exit');snap('disable exit save')
        c.new_options.FlipBoolean('saving_sash_positions_on_exit');snap('enable exit save')
        # Close/reopen reuses the actual live Page and preserves its splitter state.
        select(two);two.SetSplitterPositions(475,-290);settle();before_close=state(two)
        root._ClosePages([root.indexOf(two)],'recording',polite=False)
        c.gui._UnclosePage();settle();select(two);after_reopen=state(two)
        signed=[]
        for hpos,vpos in [(-900,600),(400,0),(0,-240)]:
            one.SetSplitterPositions(hpos,vpos);settle();signed.append(dict(requested=[hpos,vpos],actual=state(one)))
        one.SetSplitterPositions(420,-260);select(one)
        one.grab().save(str(HERE/'fixtures'/'sidebar_layout_reference.png'))
        return dict(signed_legacy_probes=signed,preview_consumer=consumer,defaults=dict(hpos=400,vpos=-240,hide_preview=False,save_on_exit=True),steps=steps,new_page=new_page,hidden_new=hidden_new_state,abandoned_hide_before=before,closed=before_close,reopened=after_reopen,limits=['Native minimum sizes/font/layout may differ; signed legacy positions and physical Qt results retained here.','Accepted exit calls _SaveSplitterPositions after confirmation in _Exit; the recorder does not exit the driver-owned frame.','Session serialization does not include live Page splitter geometry.'])
    return c.CallBlockingToQt(c.gui,drive)

def child(out):
    import hydrus_driver,record_api
    Path(out).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))

def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp)/'out.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    dest=HERE/'fixtures'/'sidebar_layout.json';dest.write_text(json.dumps(result,indent=2)+'\n');print(f'wrote {dest}')
if __name__=='__main__':main()
