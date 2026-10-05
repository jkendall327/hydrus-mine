#!/usr/bin/env python3
"""Record actual OR connector editor and its currently inert Qt renderer."""
import json
import sys
import tempfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
def record(session):
    from qtpy import QtWidgets as QW, QtTest
    from hydrus.core import HydrusSerialisable
    from hydrus.client import ClientOptions
    from hydrus.client.gui.panels.options.TagPresentationPanel import TagPresentationPanel
    from hydrus.client.gui.lists import ClientGUIListBoxes
    from hydrus.client.search import ClientSearchPredicate as P
    c=session.controller
    def drive():
        original=c.new_options.GetString('or_connector')
        cancelled=TagPresentationPanel(c.gui,c.new_options)
        cancelled._or_connector.setText('cancelled connector')
        before=c.new_options.GetString('or_connector');cancelled.deleteLater()
        after_cancel=c.new_options.GetString('or_connector')
        dialog=QW.QDialog(c.gui);dialog.setWindowTitle('OR connecting string reference')
        layout=QW.QVBoxLayout(dialog)
        panel=TagPresentationPanel(dialog,c.new_options);layout.addWidget(panel)
        box=ClientGUIListBoxes.ListBoxTagsPredicates(dialog,height_num_chars=4)
        layout.addWidget(box)
        pred=P.Predicate(P.PREDICATE_TYPE_OR_CONTAINER,[P.Predicate(P.PREDICATE_TYPE_TAG,'character:alpha'),P.Predicate(P.PREDICATE_TYPE_TAG,'series:beta',inclusive=False)])
        box.SetPredicates([pred]);dialog.resize(1000,1000);dialog.show()
        def settle():QW.QApplication.processEvents();QtTest.QTest.qWait(20)
        def labels():
            term=box._ordered_terms[0]
            return dict(user=pred.ToString(render_for_user=True),canonical=pred.ToString(render_for_user=False),construction=pred.ToString(render_for_user=True,or_under_construction=True),header=pred.GetTextsAndNamespaces(True),rows=[[dict(text=text,rgb=list(colour)) for text,colour in row] for row in box._GetRowsOfTextsAndColours(term)],predicate=pred.GetSerialisableTuple(),copy=term.GetCopyableTexts(),collapsed_copy=term.GetCopyableTexts(collapse_ors=True))
        cases=[]
        for raw in [' OR ',' / custom / ','','  raw\t ','A\nB\rC',' 🦊 ']:
            panel._or_connector.setText(raw);shown=panel._or_connector.text();staged=c.new_options.GetString('or_connector')
            panel.UpdateOptions();box.NotifyNewOptions();settle()
            saved=c.new_options.GetString('or_connector')
            restored=HydrusSerialisable.CreateFromString(c.new_options.DumpToString())
            reopened=TagPresentationPanel(c.gui,restored);reopened_text=reopened._or_connector.text();reopened.deleteLater()
            cases.append(dict(input=raw,shown=shown,before_apply=staged,saved=saved,reopened=reopened_text,labels=labels()))
        # Preserve an actual encoded ClientOptions with newline content for
        # retained-old-store and native legacy-import decoding regression.
        c.new_options.SetString('or_connector','legacy\n raw\t OR ')
        legacy=c.new_options.GetSerialisableTuple()
        dialog.grab().save(str(HERE/'fixtures'/'or_connector_reference.png'))
        c.new_options.SetString('or_connector',original);dialog.deleteLater()
        return dict(label='OR connecting string (on one line): ',factory=ClientOptions.ClientOptions().GetString('or_connector'),loaded=original,cancel=dict(before=before,after=after_cancel),cases=cases,legacy=legacy,legacy_value='legacy\n raw\t OR ',reference_behavior='GetTextsAndNamespaces custom connector loop is disabled; active OR header and ToString retain literal OR text regardless of saved or_connector.')
    return c.CallBlockingToQt(c.gui,drive)
def child(out):
    import hydrus_driver,record_api
    Path(out).write_text(json.dumps(hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)))
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp)/'out.json';hydrus_driver.run_in_subprocess(str(Path(__file__).resolve()),'--child',str(out));result=json.loads(out.read_text())
    dest=HERE/'fixtures'/'or_connector.json';dest.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n');print(f'wrote {dest}')
if __name__=='__main__':main()
