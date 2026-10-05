#!/usr/bin/env python3
"""Real active-list search menus, activation/edit deltas and populated panels.

Drive the actual query sidebar list and actual EditPredicatesPanel. Only modal
exec answers and foreground control changes are scripted; menu publication is
captured before popup transport. Simple/mixed dialog edits and parser vetoes are also recorded; OR and
inherited actions remain separately bounded. --mixed-only writes that fixture.
"""
import json, os, shutil, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from hydrus_driver import run_client
import record_api


def record(session):
    c = session.controller
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUICore as Core, ClientGUITopLevelWindowsPanels as Windows
    from hydrus.client.gui.search import ClientGUIACDropdown as AC
    from hydrus.client.search import ClientSearchPredicate as P, ClientSearchFileSearchContext as F, ClientSearchTagContext as T
    from qtpy import QtCore as QC, QtWidgets as QW
    gui = c.gui
    qt = lambda f: c.CallBlockingToQt(gui, f)
    context = F.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), tag_context=T.TagContext())
    page = qt(lambda: gui._notebook.NewPageQueryFileSearchContext(context, page_name='Active predicate editing'))
    for _ in range(500):
        sidebar = qt(page.GetSidebar)
        if sidebar is not None and hasattr(sidebar, '_tag_autocomplete'): break
        time.sleep(.01)
    else: raise AssertionError('query sidebar not created')
    ac = sidebar._tag_autocomplete
    box = ac._predicates_listbox
    tag = lambda name: P.Predicate(P.PREDICATE_TYPE_TAG, name)
    size = P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIZE, ('<',7,1024))
    alpha, beta = tag('series:active alpha'), tag('series:active beta')
    inbox = P.Predicate(P.PREDICATE_TYPE_SYSTEM_INBOX)
    archive = P.Predicate(P.PREDICATE_TYPE_SYSTEM_ARCHIVE)
    group = P.Predicate(P.PREDICATE_TYPE_OR_CONTAINER, [alpha,beta])
    events, edits, mixed, ors = [], [], [], []
    old_exec, old_popup, old_thread = Windows.DialogEdit.exec, Core.core().PopupMenu, c.CallToThread
    c.CallToThread = lambda fn,*args,**kw: None if fn == AC.ReadFetch else old_thread(fn,*args,**kw)
    plan = None
    def snapshot():
        values = box.GetPredicates()
        return {'predicates':[p.GetSerialisableTuple() for p in sorted(values,key=lambda p:p.ToString())], 'labels': sorted(p.ToString(with_count=False) for p in values), 'query_count': len(c.Read('file_query_ids', ac.GetFileSearchContext()))}
    def set_values(values, selected):
        ctx = context.Duplicate(); ctx.SetPredicates(values)
        ac.SetFileSearchContext(ctx); ac.SetSynchronised(False)
        box._selected_terms = {box._GenerateTermFromPredicate(p) for p in selected}
    def menu_tree(menu):
        return [{'label':a.text().replace('&&','&'), 'separator':a.isSeparator(), 'children': menu_tree(a.menu()) if a.menu() else []} for a in menu.actions()]
    menus = []
    Core.core().PopupMenu = lambda widget,menu: menus.append(menu_tree(menu))
    def dialog_exec(dialog):
        panels = dialog._panel._editable_pred_panels
        plan['title'] = dialog.windowTitle()
        plan['panels'] = [type(p).__name__ for p in panels]
        plan['initial'] = [p.GetSerialisableTuple() for p in dialog._panel.GetValue()]
        plan['initial_labels'] = [p.ToString(with_count=False) for p in dialog._panel.GetValue()]
        plan['invertible_before'] = [b.text() for b in dialog._panel._invertible_pred_buttons]
        for panel, text in zip([p for p in panels if type(p).__name__ == 'PanelPredicateSimpleTagTypes'], plan.get('simple', [])):
            panel._simple_tag_text.setText(text)
        for button in dialog._panel._invertible_pred_buttons:
            if plan.get('flip'): button.click()
        for panel, value in zip([p for p in panels if type(p).__name__ == 'PanelPredicateSystemSize'], plan.get('sizes', [])):
            panel._sign.SetValue('>'); panel._bytes.SetSeparatedValue(value,1024)
        if 'or_keep' in plan:
            for panel in panels:
                if type(panel).__name__ == 'ORPredicateControl':
                    predicates=panel._search_control.GetPredicates()
                    remove=[p for i,p in enumerate(predicates) if i not in plan['or_keep']]
                    panel._search_control._predicates_listbox.EnterPredicates(remove,permit_add=False)
        if plan['mutate_size']:
            panel = next(p for p in panels if type(p).__name__ == 'PanelPredicateSystemSize')
            panel._sign.SetValue('>');panel._bytes.SetSeparatedValue(11,1024)
        plan['value'] = [p.GetSerialisableTuple() for p in dialog._panel.GetValue()]
        plan['invertible_after'] = [b.text() for b in dialog._panel._invertible_pred_buttons]
        if plan.get('screenshot'):
            dialog.resize(800,400);dialog.show();QC.QCoreApplication.processEvents()
            assert dialog.grab().save(os.path.join(HERE,'fixtures/' + plan.get('png','active_predicate_editor_qt.png')))
        return QW.QDialog.DialogCode.Accepted if plan['accepted'] else QW.QDialog.DialogCode.Rejected
    Windows.DialogEdit.exec = dialog_exec
    def replay():
        nonlocal plan
        specs = [
            ('remove_tag',[alpha,beta],[alpha],'remove_predicates'),
            ('invert_tag',[alpha,beta],[alpha],'add_inverse_predicates'),
            ('invert_both',[alpha,beta],[alpha,beta],'add_inverse_predicates'),
            ('invert_inbox',[inbox,beta],[inbox],'add_inverse_predicates'),
            ('inverse_already_present',[alpha,alpha.GetInverseCopy(),beta],[alpha],'add_inverse_predicates'),
            ('replace_or',[alpha,beta,inbox],[alpha,beta],'replace_or_predicate'),
            ('dissolve_or',[group,inbox],[group],'dissolve_or_predicate'),
            ('add_namespace',[alpha,beta,inbox],[alpha,beta],'add_namespace_predicate'),
            ('exclude_namespace',[alpha,beta,inbox],[alpha,beta],'add_inverse_namespace_predicate'),
            ('activate_remove',[alpha,beta],[alpha],'activate'),
            ('ctrl_activate',[alpha,beta],[alpha],'ctrl_activate'),
            ('ctrl_inverse_already_present',[alpha,alpha.GetInverseCopy(),beta],[alpha],'ctrl_activate'),
            ('shift_invert_only',[inbox,beta],[inbox],'shift_activate'),
        ]
        for name,values,selected,command in specs:
            set_values(values,selected);before=snapshot();menus.clear();box.ShowMenuFromSignal(QC.QPoint(1,1))
            if command.endswith('activate'):
                result=box._Activate(command=='ctrl_activate',command=='shift_activate')
            else:
                result=None;box._ProcessMenuPredicateEvent(command)
            events.append({'name':name,'command':command,'selected':[p.GetSerialisableTuple() for p in selected],'before':before,'menu':menus[-1],'result':result,'after':snapshot()})
        c.new_options.SetCustomDefaultSystemPredicates(predicate_type=P.PREDICATE_TYPE_SYSTEM_SIZE,predicates=[P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIZE,('>',99,1024*1024))])
        examples = [('size',size),('limit',P.Predicate(P.PREDICATE_TYPE_SYSTEM_LIMIT,13)),('tag',alpha),('namespace',P.Predicate(P.PREDICATE_TYPE_NAMESPACE,'series')),('wildcard',P.Predicate(P.PREDICATE_TYPE_WILDCARD,'series:active*')),('or',group)]
        for name,predicate in examples:
            for accepted in [False,True]:
                set_values([predicate,inbox],[predicate]);plan={'name':name,'accepted':accepted,'mutate_size':name=='size','screenshot':name=='size' and accepted}
                plan['before']=snapshot();box._EditPredicates([predicate]);plan['after']=snapshot();edits.append(plan)
        simple_cases=[]
        from hydrus.client.gui.search import ClientGUIPredicatesSingle as Simple
        for value in ['', '-', '---', ':', ':*', 'series:', '-series:*', '--series:alpha*', 'a:b:c', '*', '  tag  ', '---blue eyes', 'series:*tail', 'a::']:
            try:
                predicate=Simple.GetPredicateFromSimpleTagText(value)
                simple_cases.append({'text':value,'predicate':predicate.GetSerialisableTuple()})
            except Exception as error:
                simple_cases.append({'text':value,'error':str(error)})
        smaller=P.Predicate(P.PREDICATE_TYPE_SYSTEM_SIZE, ('>',3,1024))
        everything=P.Predicate(P.PREDICATE_TYPE_SYSTEM_EVERYTHING)
        batches=[('simple_tag',[alpha],['-series:*'],[],False),('simple_wildcard',[P.Predicate(P.PREDICATE_TYPE_WILDCARD,'series:active*')],['series:beta'],[],False),('two_sizes',[size,smaller],[],[11,17],False),('mixed',[size,alpha,inbox,everything],['--series:edited*'],[11],True)]
        for name,selected,texts,sizes,flip in batches:
            for accepted in [False,True]:
                values=selected+[beta]
                set_values(values,selected)
                menus.clear();box.ShowMenuFromSignal(QC.QPoint(1,1))
                plan={'name':name,'selected':[p.GetSerialisableTuple() for p in selected],'accepted':accepted,'mutate_size':False,'simple':texts,'sizes':sizes,'flip':flip,'before':snapshot(),'menu':menus[-1],'screenshot':name=='mixed' and accepted,'png':'active_predicate_mixed_qt.png'}
                box._EditPredicates(selected);plan['after']=snapshot();mixed.append(plan)
        for name,selected,keep,command in [('edit_empty',[group],[],'edit'),('edit_single',[group],[0],'edit'),('edit_or',[group],[0,1],'edit'),('start_single',[alpha],[0],'start_or_predicate'),('start_or',[alpha,beta],[0,1],'start_or_predicate')]:
            for accepted in [False,True]:
                set_values(selected+[inbox],selected)
                menus.clear();box.ShowMenuFromSignal(QC.QPoint(1,1))
                plan={'name':name,'command':command,'selected':[p.GetSerialisableTuple() for p in selected],'accepted':accepted,'mutate_size':False,'or_keep':keep,'before':snapshot(),'menu':menus[-1],'screenshot':name=='edit_or' and accepted,'png':'active_predicate_or_qt.png'}
                if command=='edit': box._EditPredicates(selected)
                else: box._ProcessMenuPredicateEvent(command)
                plan['after']=snapshot();ors.append(plan)
        return {'events':events,'edits':edits,'custom_size_default':('>',99,1024*1024),'mixed':mixed,'simple_cases':simple_cases,'ors':ors}


    try: return qt(replay)
    finally:
        Windows.DialogEdit.exec=old_exec;Core.core().PopupMenu=old_popup;c.CallToThread=old_thread


if __name__ == '__main__':
    mixed_only='--mixed-only' in sys.argv
    or_only='--or-only' in sys.argv
    db = record_api.unpack_fixture('basic')
    try:
        result = run_client(db, record)
        if or_only: result={'ors':result['ors']}
        elif mixed_only: result={key:result[key] for key in ['mixed','simple_cases','custom_size_default']}
        filename='active_predicate_or.json' if or_only else 'active_predicate_mixed.json' if mixed_only else 'active_predicate_edit.json'
        with open(os.path.join(HERE,'fixtures',filename),'w') as stream:
            json.dump(result,stream,indent=2,ensure_ascii=False);stream.write('\n')
    finally: shutil.rmtree(db)
