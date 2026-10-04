#!/usr/bin/env python3
"""Record actual predicate star menus, comparability and future-panel defaults.

Drive every valid panel family through its real Qt widgets, menu actions and
ClientOptions. Save/reset are immediate, owner close is independent, resetting
leaves existing controls untouched. Rating service panels intentionally do not
use the custom initializer in the actual reference; comparability ignores their
service keys. Also record explicit supplied-predicate precedence for all panels.
"""
import json
import os
import shutil
import sys
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)
import hydrus_driver
import record_api
from record_system_predicate_editors import input_widgets, widget_facts, changed


def record(session):
    controller=session.controller
    def work():
        from qtpy import QtCore as QC, QtWidgets as QW
        from hydrus.client.gui.search import ClientGUISearch as S
        from hydrus.client.gui import ClientGUICore as CGC
        from hydrus.client import ClientLocation, ClientConstants as CC
        from hydrus.client.search import ClientSearchFileSearchContext as F, ClientSearchTagContext as T
        from hydrus.client.networking import ClientNetworkingURLClass as U
        from hydrus.core import HydrusSerialisable
        options=controller.new_options
        original=options._dictionary['custom_default_predicates']
        manager=controller.network_engine.domain_manager
        classes=manager.GetURLClasses()
        originals_popup=CGC.core().PopupMenu
        menus=[]
        CGC.core().PopupMenu=lambda owner,menu: menus.append(menu)
        added=[]
        for i in range(2):
            c=U.URLClass(name=f'predicate defaults posts {i}')
            c.SetClassKey(bytes([210+i])*32)
            added.append(c)
        manager.SetURLClasses(classes+added)
        def snapshot(panel):
            try:
                ps=panel.GetPredicates()
                return {'text':[p.ToString() for p in ps], 'serialised':[p.GetSerialisableTuple() for p in ps]}
            except Exception as error:
                return {'error':str(error)}
        def menu(wrapper):
            wrapper._DefaultsMenu()
            m=menus.pop()
            return m,[a.text() for a in m.actions() if not a.isSeparator()]
        def trigger(wrapper,label):
            m,labels=menu(wrapper)
            next(a for a in m.actions() if a.text()==label).trigger()
            m.deleteLater()
            return labels
        def edit(panel):
            name=type(panel).__name__
            widgets=input_widgets(panel)
            priority=('text' if name in ('PanelPredicateSystemHash','PanelPredicateSystemSimilarToData','PanelPredicateSystemSimilarToFiles','PanelPredicateSystemTagAdvanced') else 'date' if name.endswith('Date') else 'number')
            widgets.sort(key=lambda item: item[1]['kind']!=priority)
            if name=='PanelPredicateSystemFileViewingStatsViewtime':
                numbers=[w for w,f in widgets if f['kind']=='number']
                numbers[-1].setValue(37)
                return
            if name=='PredicateSystemRatingNumerical':
                service=controller.services_manager.GetService(panel._service_key)
                panel._choice.SetValue('=')
                panel._rating_control.SetRating(service.ConvertStarsToRating(3))
                return
            if name=='PanelPredicateSystemKnownURLsURLClass':
                panel._url_classes.setCurrentIndex(panel._url_classes.count()-1)
                return
            preferred={'PredicateSystemRatingLike':'like','PredicateSystemRatingNumerical':'stars','PredicateSystemRatingAdvanced':'radio','PanelPredicateSystemMime':'tree'}
            for w,fact in widgets:
                k=fact['kind']
                if name in preferred and k!=preferred[name]: continue
                if name in ('PanelPredicateSystemHash','PanelPredicateSystemSimilarToData','PanelPredicateSystemSimilarToFiles') and k in ('text','lines'):
                    w.setPlainText('11'*32)
                    return
                if k in ('text','lines'):
                    value=('predicate-defaults.example' if 'Domain' in name else 'https://predicate-defaults.example/post/7' if 'ExactURL' in name else r'predicate-defaults\.example' if 'Regex' in name else 'series:custom default' if 'TagAdvanced' in name else 'custom notes' if 'NoteName' in name else 'series')
                    if isinstance(w,QW.QLineEdit): w.setText(value)
                    else: w.setPlainText(value)
                    return
                if k=='date':
                    w.setSelectedDate(QC.QDate(2026,2,3))
                    return
                if k in ('number','like','stars','tree','radio'):
                    iterator=changed(w,fact)
                    try: next(iterator)
                    except StopIteration: continue
                    return
            raise AssertionError(f'No valid edit for {name}')
        context=F.FileSearchContext(location_context=ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY),tag_context=T.TagContext())
        offered=[p for p in controller.Read('file_system_predicates',context) if p.GetValue() is None and p.GetType() in S.FLESH_OUT_SYSTEM_PRED_TYPES]
        cases=[]
        try:
            for blank in offered:
                owner=S.FleshOutPredicatePanel(controller.gui,blank)
                owner.show()
                wrappers=owner.findChildren(S.FleshOutPredicatePanel._PredOKPanel)
                for wrapper in wrappers:
                    options._dictionary['custom_default_predicates']=HydrusSerialisable.SerialisableList()
                    panel=wrapper._predicate_panel
                    before=snapshot(panel)
                    edit(panel)
                    edited=snapshot(panel)
                    assert 'error' not in edited and edited['serialised'],(type(panel).__name__,edited)
                    original_fields=widget_facts(panel)
                    labels_before=trigger(wrapper,'set this as new default')
                    saved=list(options._dictionary['custom_default_predicates'])
                    # A new panel uses a comparable supplied predicate first;
                    # ratings also force the panel's own service key.
                    cls=type(panel)
                    args=[panel._service_key] if hasattr(panel,'_service_key') else []
                    fresh=cls(controller.gui,*args,blank)
                    explicit_input=HydrusSerialisable.CreateFromSerialisableTuple(json.loads(json.dumps(before['serialised'][0])))
                    explicit=cls(controller.gui,*args,explicit_input)
                    fresh.show()
                    explicit.show()
                    fresh_after_save=snapshot(fresh)
                    explicit_value=snapshot(explicit)
                    m,labels_after=menu(wrapper)
                    m.deleteLater()
                    reset_menu=trigger(wrapper,'reset to original default')
                    after_reset=snapshot(panel)
                    assert widget_facts(panel)==original_fields
                    future=cls(controller.gui,*args,blank)
                    future.show()
                    future_value=snapshot(future)
                    cases.append({'blank':blank.ToString(),'class':cls.__name__,'service_key':args[0].hex() if args else None,'before':before,'edited':edited,'saved':[p.GetSerialisableTuple() for p in saved],'fresh_after_save':fresh_after_save,'explicit_input':explicit_input.GetSerialisableTuple(),'explicit':explicit_value,'current_after_reset':after_reset,'fresh_after_reset':future_value,'menu_before':labels_before,'menu_after_save':labels_after,'reset_menu':reset_menu,'uses_after_reset':panel.UsesCustomDefault()})
                    for child in (fresh,explicit,future): child.deleteLater()
                owner.close()
                owner.deleteLater()
            # Keep independent values concurrently and record the full actual
            # IsUIEditable matrix, including rating service and subtype pairs.
            from hydrus.client.gui.search import ClientGUIPredicatesSingle as P
            from hydrus.core import HydrusSerialisable as H
            values=[H.CreateFromSerialisableTuple(json.loads(json.dumps(c['saved'][0]))) for c in cases]
            comparability=[[a.IsUIEditable(b) for b in values] for a in values]
            # Persist each family's actual recorded outputs through the real
            # options setter, including its cross-service rating replacement.
            options._dictionary['custom_default_predicates']=H.SerialisableList()
            for value in values:
                options.SetCustomDefaultSystemPredicates(comparable_predicates=[value])
            # Keep an interior numerical rating in the actual stored options,
            # proving import needs the service's five-star scale.
            numerical=next(value for value,case in zip(values,cases) if case['class']=='PredicateSystemRatingNumerical')
            options.SetCustomDefaultSystemPredicates(comparable_predicates=[numerical])
            assert len(options._dictionary['custom_default_predicates'])==38
            saved_families=options.GetSerialisableTuple()
            # Exercise saving and owner cancellation as a separate lifecycle.
            limit_blank=next(p for p in offered if p.ToString()=='system:limit')
            owner=S.FleshOutPredicatePanel(controller.gui,limit_blank)
            owner.show()
            wrapper=owner.findChildren(S.FleshOutPredicatePanel._PredOKPanel)[0]
            options._dictionary['custom_default_predicates']=H.SerialisableList()
            wrapper._predicate_panel._limit.setValue(731)
            trigger(wrapper,'set this as new default')
            owner.close()
            kept=[p.GetSerialisableTuple() for p in options._dictionary['custom_default_predicates']]
            restored=H.CreateFromSerialisableTuple(json.loads(json.dumps(options.GetSerialisableTuple())))
            durable=[p.GetSerialisableTuple() for p in restored.GetCustomDefaultSystemPredicates(predicate_type=limit_blank.GetType())]
            owner.deleteLater()
            # Star Save calls GetPredicates directly. Regex validation happens
            # only on accepting the panel, so even an invalid regex is durable.
            options._dictionary['custom_default_predicates']=H.SerialisableList()
            urls_blank=next(p for p in offered if p.ToString()=='system:urls')
            owner=S.FleshOutPredicatePanel(controller.gui,urls_blank)
            owner.show()
            wrapper=next(w for w in owner.findChildren(S.FleshOutPredicatePanel._PredOKPanel) if type(w._predicate_panel).__name__=='PanelPredicateSystemKnownURLsRegex')
            panel=wrapper._predicate_panel
            panel._regex.SetValue('[')
            trigger(wrapper,'set this as new default')
            fresh=type(panel)(controller.gui,urls_blank)
            m,regex_menu=menu(wrapper)
            m.deleteLater()
            try:
                panel.CheckValid()
                validation_error=None
            except Exception as error:
                validation_error=str(error)
            invalid_regex={'saved':[p.GetSerialisableTuple() for p in options._dictionary['custom_default_predicates']],'fresh':snapshot(fresh),'menu_after_save':regex_menu,'accept_validation_error':validation_error}
            assert validation_error is not None
            fresh.deleteLater()
            owner.close()
            owner.deleteLater()
            # A namespaced zero/any number-of-tags panel emits a namespace
            # predicate. It is saved but is not comparable to this panel's
            # normal number-of-tags default, so its own menu has no reset.
            options._dictionary['custom_default_predicates']=H.SerialisableList()
            tags_blank=next(p for p in offered if p.ToString()=='system:number of tags')
            owner=S.FleshOutPredicatePanel(controller.gui,tags_blank)
            owner.show()
            wrapper=owner.findChildren(S.FleshOutPredicatePanel._PredOKPanel)[0]
            panel=wrapper._predicate_panel
            panel._namespace.SetValue('series')
            panel._sign.SetValue('>')
            panel._num_tags.setValue(0)
            trigger(wrapper,'set this as new default')
            m,edge_menu=menu(wrapper)
            m.deleteLater()
            fresh=type(panel)(controller.gui,tags_blank)
            namespace_edge={'saved':snapshot(panel),'menu_after_save':edge_menu,'uses_custom':panel.UsesCustomDefault(),'fresh':snapshot(fresh)}
            fresh.deleteLater()
            owner.deleteLater()
            services=[]
            for service in controller.services_manager.GetServices():
                d=service.GetSerialisableDictionary()
                services.append({'name':service.GetName(),'key':service.GetServiceKey().hex(),'type':service.GetServiceType(),'num_stars':d.get('num_stars',5),'allow_zero':d.get('allow_zero',False)})
            return {'invalid_regex':invalid_regex,'options_saved_families':saved_families,'cases':cases,'comparability':comparability,'owner_cancel_saved':kept,'options_roundtrip':durable,'namespace_edge':namespace_edge,'services':services,'url_classes':[c.GetName() for c in manager.GetURLClasses() if c.ShouldAssociateWithFiles()],'today':QC.QDate.currentDate().toString('yyyy-MM-dd')}
        finally:
            options._dictionary['custom_default_predicates']=original
            manager.SetURLClasses(classes)
            CGC.core().PopupMenu=originals_popup
    return controller.CallBlockingToQt(controller.gui,work)


if __name__=='__main__':
    db=record_api.unpack_fixture('basic')
    try:
        result=hydrus_driver.run_client(db,record)
        with open(os.path.join(HERE,'fixtures/predicate_custom_defaults.json'),'w') as stream:
            json.dump(result,stream,indent=2,ensure_ascii=False)
            stream.write('\n')
    finally:
        shutil.rmtree(db)
