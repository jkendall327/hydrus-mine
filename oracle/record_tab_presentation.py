#!/usr/bin/env python3
"""Record staged GUI Pages options and actual nested notebook tab presentation.

The real GUIPagesPanel updates private options. PagesNotebook's existing update
and recursive visibility consumers apply all four alignments, gated hiding and
middle elision. QStyleOptionTab captures actual fitted paint text, independent
from the full stored tab text and page-name tooltip. Bar PNGs retain real Qt
left/right text orientation. No reference source edits.
"""
import json,sys,tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))


def record(session):
    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.gui.panels.options.GUIPagesPanel import GUIPagesPanel
    from hydrus.core import HydrusSerialisable
    c=session.controller
    original=c.new_options
    def values(options):
        return dict(alignment=options.GetInteger('notebook_tab_alignment'),tree=options.GetNoneableInteger('treeview_alignment'),hide=options.GetBoolean('treeview_hides_tabs'),elide=options.GetBoolean('elide_page_tab_names'))
    def drive():
        draft=original.Duplicate();panel=GUIPagesPanel(c.gui,draft)
        choices=[dict(label=panel._notebook_tab_alignment.itemText(i),value=panel._notebook_tab_alignment.itemData(i)) for i in range(panel._notebook_tab_alignment.count())]
        tree_choices=[dict(label=panel._treeview_alignment.itemText(i),value=panel._treeview_alignment.itemData(i)) for i in range(panel._treeview_alignment.count())]
        labels=[label.text() for label in panel.findChildren(QW.QLabel) if label.text().startswith(('Notebook tab alignment:','EXPERIMENTAL: Hide main','EXPERIMENTAL: Show tab tree','When there are too many tabs'))]
        before=values(draft)
        panel._notebook_tab_alignment.SetValue(CC.DIRECTION_DOWN)
        panel._treeview_alignment.SetValue(CC.DIRECTION_RIGHT)
        panel._treeview_hides_tabs.setChecked(True)
        panel._elide_page_tab_names.setChecked(False)
        cancelled=values(draft);panel.deleteLater()
        draft=original.Duplicate();draft.SetInteger('max_page_name_chars',256)
        draft.SetInteger('page_file_count_display',CC.PAGE_FILE_COUNT_DISPLAY_NONE)
        c.new_options=draft
        panel=GUIPagesPanel(c.gui,draft)
        root=ClientGUIPages.PagesNotebook(c.gui,'recorded root')
        names=['alpha with a very long title and a distinct end ALPHA','nested notebook','omega with a very long title and a distinct end OMEGA']
        root.NewPagesNotebook(name=names[0],give_it_a_blank_page=False)
        nested=root.NewPagesNotebook(name=names[1],give_it_a_blank_page=False)
        root.NewPagesNotebook(name=names[2],give_it_a_blank_page=False)
        nested_names=['beta with a very long title and a distinct end BETA','gamma with a very long title and a distinct end GAMMA']
        context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY)
        nested.NewPageQuery(context,page_name=nested_names[0],select_page=False)
        nested.NewPageQuery(context,page_name=nested_names[1],select_page=False)
        root.setCurrentIndex(1);nested.setCurrentIndex(1)
        root.resize(900,450);root.show();QW.QApplication.processEvents()
        def apply():
            panel.UpdateOptions()
            root._UpdateOptions();nested._UpdateOptions();root.UpdateTabVisibility()
            for notebook in (root,nested):
                for index in range(notebook.count()):notebook._RefreshPageName(index)
            QW.QApplication.processEvents()
        def snapshot(notebook):
            bar=notebook.tabBar();paint=[]
            for index in range(bar.count()):
                option=QW.QStyleOptionTab();bar.initStyleOption(option,index)
                rect=bar.tabRect(index)
                paint.append(dict(stored=bar.tabText(index),paint=option.text,tooltip=bar.tabToolTip(index),width=rect.width(),height=rect.height(),shape=option.shape.name))
            return dict(position=notebook.tabPosition().name,elide=bar.elideMode().name,hidden=bar.isHidden(),selected=bar.currentIndex(),tabs=paint)
        alignments=[]
        for choice in choices:
            panel._notebook_tab_alignment.SetValue(choice['value']);apply()
            alignments.append(dict(choice=choice,settings=values(draft),root=snapshot(root),nested=snapshot(nested)))
            root.tabBar().grab().save(str(HERE/'fixtures'/f"tab_presentation_qt_{choice['label']}.png"))
        hide=[]
        panel._notebook_tab_alignment.SetValue(CC.DIRECTION_UP)
        for tree in [None,CC.DIRECTION_LEFT,CC.DIRECTION_RIGHT]:
            panel._treeview_alignment.SetValue(tree)
            for enabled in [False,True]:
                panel._treeview_hides_tabs.setChecked(enabled);apply()
                hide.append(dict(settings=values(draft),root=snapshot(root),nested=snapshot(nested)))
        panel._treeview_alignment.SetValue(None);panel._treeview_hides_tabs.setChecked(False)
        elide=[]
        for width in [230,420,1000]:
            root.resize(width,450)
            for enabled in [False,True]:
                panel._elide_page_tab_names.setChecked(enabled);apply()
                elide.append(dict(width=width,settings=values(draft),root=snapshot(root),nested=snapshot(nested)))
        reopened_options=HydrusSerialisable.CreateFromString(draft.DumpToString())
        reopened=GUIPagesPanel(c.gui,reopened_options)
        reopened_values=dict(alignment=reopened._notebook_tab_alignment.GetValue(),tree=reopened._treeview_alignment.GetValue(),hide=reopened._treeview_hides_tabs.isChecked(),elide=reopened._elide_page_tab_names.isChecked())
        reopened.deleteLater();panel.deleteLater();root.hide();root.deleteLater()
        return dict(defaults=before,choices=choices,tree_choices=tree_choices,labels=labels,cancelled=cancelled,names=names,nested_names=nested_names,alignments=alignments,hide=hide,elide=elide,reopened=reopened_values)
    try:return c.CallBlockingToQt(c.gui,drive)
    finally:c.new_options=original


def child(out):
    import hydrus_driver,record_api
    result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    Path(out).write_text(json.dumps(result))


def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as temp:
        output=Path(temp)/'out.json'
        hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(output))
        result=json.loads(output.read_text())
    dest=HERE/'fixtures'/'tab_presentation.json'
    dest.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
    print(f'wrote {dest}')
if __name__=='__main__':main()
