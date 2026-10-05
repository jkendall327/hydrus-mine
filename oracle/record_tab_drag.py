#!/usr/bin/env python3
"""Record staged tab DnD/wheel preferences and real Qt event consumers.

Synthetic drop events supply the source bar Qt normally attaches during native
QDrag execution; no decision handlers are replaced. This records in-client
routing, not platform drag-loop behavior. Source Hydrus stays unchanged.
"""
import json,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
KEYS=['page_drop_chase_normally','page_drop_chase_with_shift','page_drag_change_tab_normally','page_drag_change_tab_with_shift','wheel_scrolls_tab_bar','disable_page_tab_dnd']

def record(session):
    from qtpy import QtCore as QC,QtGui as QG,QtWidgets as QW,QtTest
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui.panels.options.GUIPagesPanel import GUIPagesPanel
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.core import HydrusTime
    from hydrus.core import HydrusSerialisable
    c=session.controller;original=c.new_options
    def drive():
        draft=original.Duplicate();c.new_options=draft
        panel=GUIPagesPanel(c.gui,draft)
        defaults={key:draft.GetBoolean(key) for key in KEYS}
        labels=[l.text() for l in panel.findChildren(QW.QLabel) if any(t in l.text() for t in ['chases dropped','Navigate tabs','With shift','Mouse wheel','Disable all page'])]
        controls=[getattr(panel,'_'+key) for key in KEYS]
        for control in controls:control.setChecked(not control.isChecked())
        cancelled={key:draft.GetBoolean(key) for key in KEYS};panel.deleteLater()
        panel=GUIPagesPanel(c.gui,draft)
        for control in [getattr(panel,'_'+key) for key in KEYS]:control.setChecked(not control.isChecked())
        panel.UpdateOptions();saved={key:draft.GetBoolean(key) for key in KEYS}
        reopened=GUIPagesPanel(c.gui,HydrusSerialisable.CreateFromString(draft.DumpToString()))
        reopen={key:getattr(reopened,'_'+key).isChecked() for key in KEYS};reopened.deleteLater();panel.deleteLater()
        root=c.gui._notebook;old=list(range(root.count()))
        context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        for name in ['alpha','beta','gamma','omega']:root.NewPageQuery(context,page_name=name,select_page=False)
        nested=root.NewPagesNotebook(name='nested',give_it_a_blank_page=False)
        for name in ['inner one','inner two']:nested.NewPageQuery(context,page_name=name,select_page=False)
        root._ClosePages(old,'recording',polite=False)
        c.new_options.SetInteger('page_file_count_display',CC.PAGE_FILE_COUNT_DISPLAY_NONE)
        def settle():
            for _ in range(5):QW.QApplication.processEvents()
        def snapshot():
            settle();return dict(order=[p.GetName() for p in root.GetPages()],root_selected=root.currentWidget().GetName(),shown=c.gui.GetCurrentPage().GetName(),nested=[p.GetName() for p in nested.GetPages()],nested_selected=nested.currentWidget().GetName())
        mime=QC.QMimeData();mime.setData('application/hydrus-tab',b'')
        navigation=[]
        for shift in [False,True]:
            for enabled in [False,True]:
                root.setCurrentIndex(0);draft.SetBoolean('page_drag_change_tab_with_shift' if shift else 'page_drag_change_tab_normally',enabled)
                pos=root.tabBar().tabRect(3).center();point=root.mapFromGlobal(root.tabBar().mapToGlobal(pos))
                event=QG.QDragMoveEvent(point,QC.Qt.DropAction.MoveAction,mime,QC.Qt.MouseButton.LeftButton,QC.Qt.KeyboardModifier.ShiftModifier if shift else QC.Qt.KeyboardModifier.NoModifier)
                root.dragMoveEvent(event);navigation.append(dict(shift=shift,enabled=enabled,after=snapshot()))
        wheels=[]
        bar=root.tabBar()
        for scroll in [False,True]:
            draft.SetBoolean('wheel_scrolls_tab_bar',scroll);root.setCurrentIndex(2)
            for delta in [120,-120,-120]:
                point=QC.QPointF(bar.tabRect(bar.currentIndex()).center())
                event=QG.QWheelEvent(point,QC.QPointF(bar.mapToGlobal(point.toPoint())),QC.QPoint(),QC.QPoint(0,delta),QC.Qt.MouseButton.NoButton,QC.Qt.KeyboardModifier.NoModifier,QC.Qt.ScrollPhase.NoScrollPhase,False)
                bar.wheelEvent(event);wheels.append(dict(scroll=scroll,delta=delta,after=snapshot(),accepted=event.isAccepted()))
        class Drop(QG.QDropEvent):
            def __init__(self,pos,source,shift):
                super().__init__(QC.QPointF(pos),QC.Qt.DropAction.MoveAction,mime,QC.Qt.MouseButton.LeftButton,QC.Qt.KeyboardModifier.ShiftModifier if shift else QC.Qt.KeyboardModifier.NoModifier);self.source_bar=source
            def source(self):return self.source_bar
        drops=[]
        for shift in [False,True]:
            for chase in [False,True]:
                draft.SetBoolean('page_drop_chase_with_shift' if shift else 'page_drop_chase_normally',chase)
                source_index=next(i for i,p in enumerate(root.GetPages()) if p.GetName()=='gamma')
                root.setCurrentIndex(source_index);source=root.tabBar()
                QtTest.QTest.mouseClick(source,QC.Qt.MouseButton.LeftButton,pos=source.tabRect(source_index).center())
                before=snapshot();dest_index=next(i for i,p in enumerate(root.GetPages()) if p.GetName()=='alpha')
                pos=root.mapFromGlobal(source.mapToGlobal(source.tabRect(dest_index).center()))
                event=Drop(pos,source,shift);root.dropEvent(event);drops.append(dict(shift=shift,chase=chase,source_index=source_index,target='alpha',before=before,after=snapshot(),accepted=event.isAccepted()))
        transfers=[]
        gamma=next(p for p in root.GetPages() if p.GetName()=='gamma')
        for chase in [False,True]:
            if gamma not in root.GetPages():root._MovePage(gamma,root,2,follow_dropped_page=True)
            source=root.tabBar();source_index=root.indexOf(gamma)
            QtTest.QTest.mouseClick(source,QC.Qt.MouseButton.LeftButton,pos=source.tabRect(source_index).center())
            # Real page-drag hover selects the destination notebook first.
            draft.SetBoolean('page_drag_change_tab_normally',True)
            point=root.mapFromGlobal(source.mapToGlobal(source.tabRect(root.indexOf(nested)).center()))
            event=QG.QDragMoveEvent(point,QC.Qt.DropAction.MoveAction,mime,QC.Qt.MouseButton.LeftButton,QC.Qt.KeyboardModifier.NoModifier);root.dragMoveEvent(event)
            draft.SetBoolean('page_drop_chase_normally',chase);settle();before=snapshot()
            pos=nested.mapFromGlobal(nested.tabBar().mapToGlobal(nested.tabBar().tabRect(0).center()))
            drop=Drop(pos,source,False);nested.dropEvent(drop);transfers.append(dict(chase=chase,source_index=source_index,before=before,after=snapshot(),accepted=drop.isAccepted()))
        launches=[];original_drag=QG.QDrag
        class ObservedDrag(original_drag):
            def exec_(self,*args):launches.append(list(self.mimeData().formats()));return QC.Qt.DropAction.IgnoreAction
        try:
            QG.QDrag=ObservedDrag
            for disabled in [False,True]:
                draft.SetBoolean('disable_page_tab_dnd',disabled);bar=root.tabBar()
                QtTest.QTest.mouseClick(bar,QC.Qt.MouseButton.LeftButton,pos=bar.tabRect(0).center())
                bar._last_clicked_timestamp_ms=HydrusTime.GetNowMS()-200
                point=root.mapFromGlobal(bar.mapToGlobal(bar.tabRect(0).center()))
                event=QG.QMouseEvent(QC.QEvent.Type.MouseMove,QC.QPointF(point),QC.QPointF(root.mapToGlobal(point)),QC.Qt.MouseButton.NoButton,QC.Qt.MouseButton.LeftButton,QC.Qt.KeyboardModifier.NoModifier)
                count=len(launches);root.mouseMoveEvent(event)
                launches.append(dict(disabled=disabled,launched=len(launches)>count))
        finally:QG.QDrag=original_drag
        # An unattached real notebook allows actual overflow, unlike frame layout.
        overflow_book=ClientGUIPages.PagesNotebook(c.gui,'wheel recorder')
        draft.SetInteger('max_page_name_chars',256)
        for i in range(8):overflow_book.NewPagesNotebook(name=f'long notebook title {i} with distinct ending',give_it_a_blank_page=False)
        overflow_book.resize(250,220);overflow_book.show();settle()
        bar=overflow_book.tabBar();draft.SetBoolean('wheel_scrolls_tab_bar',True);overflow_book.setCurrentIndex(0);settle()
        overflow=[]
        for delta in [-120,-120,120]:
            before=[bar.tabRect(i).x() for i in range(bar.count())];point=QC.QPointF(20,10)
            event=QG.QWheelEvent(point,QC.QPointF(bar.mapToGlobal(point.toPoint())),QC.QPoint(),QC.QPoint(0,delta),QC.Qt.MouseButton.NoButton,QC.Qt.KeyboardModifier.NoModifier,QC.Qt.ScrollPhase.NoScrollPhase,False)
            bar.wheelEvent(event);settle();overflow.append(dict(delta=delta,before=before,after=[bar.tabRect(i).x() for i in range(bar.count())],selected=bar.currentIndex()))
        clipped_point=bar.tabRect(bar.count()-1).center()
        clipped_hit=dict(point=[clipped_point.x(),clipped_point.y()],viewport=[bar.width(),bar.height()],inside=bar.rect().contains(clipped_point),tab_at=bar.tabAt(clipped_point),selected=bar.currentIndex())
        overflow_book.hide();overflow_book.deleteLater()
        return dict(keys=KEYS,labels=labels,defaults=defaults,cancelled=cancelled,saved=saved,reopened=reopen,navigation=navigation,wheels=wheels,drops=drops,transfers=transfers,launches=launches,overflow=overflow,clipped_hit=clipped_hit,limits=['synthetic QDropEvent source bar seam; native OS drag loop not recorded','QDrag exec instrumentation records offered drag MIME while preserving real source-start decisions'])
    try:return c.CallBlockingToQt(c.gui,drive)
    finally:c.new_options=original

def child(out):
    import hydrus_driver,record_api
    Path(out).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as temp:
        out=Path(temp)/'out.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    dest=HERE/'fixtures'/'tab_drag.json';dest.write_text(json.dumps(result,indent=2)+'\n');print(f'wrote {dest}')
if __name__=='__main__':main()
