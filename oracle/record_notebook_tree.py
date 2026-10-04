#!/usr/bin/env python3
"""Drive the real frame's notebook tree with Qt mouse, keys and toolbar actions."""
import json,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))

def record(session):
    from qtpy import QtCore as QC,QtWidgets as QW,QtTest
    from hydrus.client import ClientConstants as CC,ClientLocation
    c=session.controller
    def drive():
        c.new_options.SetNoneableInteger('treeview_alignment',CC.DIRECTION_LEFT)
        c.new_options.SetBoolean('treeview_collapse_all_children_upon_parent_closed',False)
        c.new_options.SetInteger('page_file_count_display',CC.PAGE_FILE_COUNT_DISPLAY_NONE)
        c.gui._RebuildMainFrameLayout()
        root=c.gui._notebook
        old=list(range(root.count()))
        alpha=root.NewPagesNotebook(name='alpha',give_it_a_blank_page=False)
        context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        alpha.NewPageQuery(context,page_name='one',select_page=False)
        inner=alpha.NewPagesNotebook(name='inner',give_it_a_blank_page=False)
        inner.NewPageQuery(context,page_name='beta',select_page=False)
        inner.NewPageQuery(context,page_name='gamma',select_page=False)
        root.NewPageQuery(context,page_name='omega',select_page=False)
        root._ClosePages(old,'recording',polite=False)
        root.setCurrentIndex(0);alpha.setCurrentIndex(1);inner.setCurrentIndex(1)
        tree=c.gui._tabs_tree_view;model=tree.model()
        def settle():
            for _ in range(5):QW.QApplication.processEvents()
        def walk(parent=QC.QModelIndex()):
            for row in range(model.rowCount(parent)):
                index=model.index(row,0,parent)
                yield index
                yield from walk(index)
        def name(index):return model._IndexDataOrRoot(index).obj.GetName() if index.isValid() else None
        def find(label):return next(i for i in walk() if name(i)==label)
        def visible(index):
            parent=index.parent()
            while parent.isValid():
                if not tree.isExpanded(parent):return False
                parent=parent.parent()
            return True
        steps=[]
        def snap(action):
            settle();steps.append(dict(action=action,visible=[name(i) for i in walk() if visible(i)],expanded=[name(i) for i in walk() if tree.isExpanded(i)],current=name(tree.currentIndex()),shown=c.gui.GetCurrentPage().GetName()))
        settle();tree.setFocus();snap('initial')
        c.gui._tabs_tree_sidebar.expand_all.click();snap('expand all')
        QtTest.QTest.mouseClick(tree.viewport(),QC.Qt.MouseButton.LeftButton,pos=tree.visualRect(find('beta')).center());snap('click beta')
        QtTest.QTest.keyClick(tree,QC.Qt.Key.Key_Return);snap('activate beta')
        for key,label in [(QC.Qt.Key.Key_Down,'Down'),(QC.Qt.Key.Key_Return,'Return'),(QC.Qt.Key.Key_Left,'Left'),(QC.Qt.Key.Key_Left,'Left'),(QC.Qt.Key.Key_Right,'Right'),(QC.Qt.Key.Key_Right,'Right'),(QC.Qt.Key.Key_End,'End'),(QC.Qt.Key.Key_Home,'Home')]:
            QtTest.QTest.keyClick(tree,key);snap(label)
        tree.setCurrentIndex(find('inner'));snap('select inner')
        QtTest.QTest.keyClick(tree,QC.Qt.Key.Key_Left);snap('collapse inner')
        tree.setCurrentIndex(find('alpha'));QtTest.QTest.keyClick(tree,QC.Qt.Key.Key_Left);snap('collapse alpha')
        QtTest.QTest.keyClick(tree,QC.Qt.Key.Key_Right);snap('expand alpha retains inner collapse')
        tree.expand(find('inner'));snap('expand inner')
        tree.collapse(find('alpha'));snap('collapse parent retains inner expansion')
        tree.expand(find('alpha'));snap('expand parent')
        QtTest.QTest.mouseDClick(tree.viewport(),QC.Qt.MouseButton.LeftButton,pos=tree.visualRect(find('beta')).center());snap('double click beta')
        c.gui._tabs_tree_sidebar.collapse_all.click();snap('collapse all')
        QtTest.QTest.keyClick(tree,QC.Qt.Key.Key_Up);snap('Up')
        QtTest.QTest.keyClick(tree,QC.Qt.Key.Key_Down);snap('Down')
        c.gui.ShowPage(model.GetPageKeyFromIndex(find('gamma')));snap('show gamma reveals ancestors')
        c.gui._tabs_tree_sidebar.expand_all.click();snap('expand all again')
        index=find('inner');rect=tree.visualRect(index)
        QtTest.QTest.mouseClick(tree.viewport(),QC.Qt.MouseButton.LeftButton,pos=QC.QPoint(rect.left()-tree.indentation()//2,rect.center().y()));snap('click inner disclosure')
        QtTest.QTest.mouseClick(tree.viewport(),QC.Qt.MouseButton.LeftButton,pos=tree.visualRect(find('inner')).center())
        QtTest.QTest.mouseDClick(tree.viewport(),QC.Qt.MouseButton.LeftButton,pos=tree.visualRect(find('inner')).center());snap('double click inner')
        tree.grab().save(str(HERE/'fixtures'/'notebook_tree_qt.png'))
        return dict(steps=steps,limits=['default collapse-child option false','single selection; no drag/drop, context, filters, history or cog actions'])
    return c.CallBlockingToQt(c.gui,drive)

def child(out):
    import hydrus_driver,record_api
    Path(out).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp)/'out.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    dest=HERE/'fixtures'/'notebook_tree.json';dest.write_text(json.dumps(result,indent=2)+'\n');print(f'wrote {dest}')
if __name__=='__main__':main()
