#!/usr/bin/env python3
"""Record real empty-OR and advanced-OR child dialogs, Apply/Cancel broadcasts.

DialogEdit.exec is intercepted after the real panels are constructed; scripted
local tag selections and Boolean input use their actual GetValue and caller
broadcast paths. The advanced panel records every supported operator, escapes,
system terms, syntax errors, preview and validation. Background fetch alone is
suppressed; nonempty searches record real DB query counts. An empty UI search
is not queried (null count); no reference source is edited.
"""
import json,os,sys,tempfile
HERE=os.path.dirname(os.path.abspath(__file__));sys.path.insert(0,HERE)
OUT=os.path.join(HERE,'fixtures','read_or_editors.json')
def record(session):
    from hydrus.client import ClientConstants as CC,ClientLocation
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels as W
    from hydrus.client.gui.search import ClientGUIACDropdown as A,ClientGUIPredicatesOR as O
    from hydrus.client.search import ClientSearchFileSearchContext as F,ClientSearchTagContext as T,ClientSearchPredicate as P
    from hydrus.core import HydrusConstants as HC
    from qtpy import QtWidgets as QW
    c=session.controller;qt=lambda f:c.CallBlockingToQt(c.gui,f)
    service=next(s.GetServiceKey() for s in c.services_manager.GetServices((HC.LOCAL_TAG,)) if s.GetName()=='my tags')
    from hydrus.client.metadata import ClientContentUpdates as U
    manifest=json.load(open(os.path.join(HERE,'fixtures','legacy_db','basic.manifest.json')))
    hashes=[bytes.fromhex(row['hash']) for row in manifest['files'][:4]]
    corpus=[('parity:or alpha',hashes[:2]),('parity:or beta',hashes[1:3]),('parity:or single',hashes[:3]),('parity:or gamma',hashes[:1])]
    updates=[U.ContentUpdate(HC.CONTENT_TYPE_MAPPINGS,HC.CONTENT_UPDATE_ADD,(tag,set(files))) for tag,files in corpus]
    c.WriteSynchronous('content_updates',U.ContentUpdatePackage.STATICCreateFromContentUpdates(service,updates))
    old_thread=c.CallToThread;c.CallToThread=lambda fn,*args,**kw:None if fn==A.ReadFetch else old_thread(fn,*args,**kw)
    def replay():
        context=F.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),tag_context=T.TagContext(service_key=service))
        ac=A.AutoCompleteDropdownTagsRead(c.gui,b'parity OR editors',context,synchronised=False)
        events=[]
        old_exec=W.DialogEdit.exec
        plans=iter([('basic',False,2),('basic',True,2),('advanced',False,0),('advanced',True,0),('basic',True,0),('basic',True,1)])
        def dialog_exec(dlg):
            kind,accepted,count=next(plans)
            panel=dlg._panel
            if kind=='basic':
                control=panel.findChild(O.ORPredicateControl)
                assert control is not None
                before=sorted(p.ToString() for p in control.GetPredicates())
                for tag in ['parity:or alpha','parity:or beta'][:count]:control._search_control.BroadcastChoices([P.Predicate(P.PREDICATE_TYPE_TAG,tag)])
                after=sorted(p.ToString() for p in control.GetPredicates())
            else:
                before=panel._result_preview.toPlainText()
                panel._input_text.setText('(parity:or alpha and parity:or beta) or parity:or gamma')
                after=sorted(p.ToString() for p in panel.GetValue())
            events.append(dict(kind=kind,accepted=accepted,title=dlg.windowTitle(),before=before,after=after,tags=['parity:or alpha','parity:or beta'][:count]))
            return QW.QDialog.DialogCode.Accepted if accepted else QW.QDialog.DialogCode.Rejected
        W.DialogEdit.exec=dialog_exec
        try:
            for action in [ac._CreateNewOR,ac._CreateNewOR,ac._AdvancedORInput,ac._AdvancedORInput,ac._CreateNewOR,ac._CreateNewOR]:
                ac._text_ctrl.setText('parent draft');action()
                events[-1].update(parent_text=ac._text_ctrl.text(),parent_predicates=sorted(p.ToString() for p in ac.GetFileSearchContext().GetPredicates()),query_count=None if not ac.GetFileSearchContext().GetPredicates() else len(c.Read('file_query_ids',ac.GetFileSearchContext())))
        finally:W.DialogEdit.exec=old_exec
        panel=A.EditAdvancedORPredicates(ac)
        cases=[]
        for value in ['', 'parity:or alpha', 'parity:or alpha and parity:or beta', '(parity:or alpha and parity:or beta) or parity:or gamma', 'not (parity:or alpha or parity:or beta)', 'parity:or alpha => parity:or beta', 'parity:or alpha xor parity:or beta', 'parity:or alpha xnor parity:or beta', 'parity:or alpha nand parity:or beta', 'parity:or alpha nor parity:or beta', 'parity:or alpha or not parity:or alpha', 'parity:or alpha and parity:or alpha', r'parity:or \(literal\) or parity:*', 'system:inbox and parity:or alpha', 'not system:inbox', 'parity:or alpha and', '(parity:or alpha', 'parity:or alpha)', 'PARITY:OR ALPHA && !PARITY:OR BETA', '-parity:or alpha || parity:or beta', 'parity:or alpha iff parity:or beta', 'parity:or alpha <=> parity:or beta', r'parity:or \and beta', 'parity:or bang!', '!!parity:or alpha', '(parity:or alpha or parity:or beta) and parity:or gamma', 'parity:or alpha and not (parity:or beta or parity:or gamma)']:
            panel._input_text.setText(value);panel._UpdateText()
            try:predicates=sorted(p.ToString() for p in panel.GetValue());error=None
            except Exception as e:predicates=[];error=str(e)
            cases.append(dict(input=value,preview=panel._result_preview.toPlainText(),valid=panel._result_preview.objectName(),predicates=predicates,error=error))
        ac.deleteLater();return dict(dialogs=events,cases=cases,corpus=[dict(tag=tag,hashes=[h.hex() for h in files]) for tag,files in corpus])
    try:return qt(replay)
    finally:c.CallToThread=old_thread
def child(out):
    import hydrus_driver,record_api
    value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
    with open(out,'w') as f:json.dump(value,f)
def main():
    if len(sys.argv)>1 and sys.argv[1]=='--child':child(sys.argv[2]);return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        path=os.path.join(d,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:value=json.load(f)
    with open(OUT,'w') as f:json.dump(value,f,indent=2);f.write('\n')
    print('wrote',OUT)
if __name__=='__main__':main()
