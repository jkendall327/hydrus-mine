#!/usr/bin/env python3
"""Record URL class and GUG list rows, editor examples, vetoes and cancel.

Boots the real reference basic client, creates real Qt panels and drives
their fields. Captures serialized definitions alongside their rows/previews
so Rust tests replay the exact model and user-visible strings.
Usage: QT_QPA_PLATFORM=offscreen python oracle/record_downloader_definitions.py
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    controller = session.controller
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusExceptions
    from hydrus.client import ClientStrings as S
    from hydrus.client.gui import ClientGUIDownloaders as G
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.panels import ClientGUIURLClass as C
    from hydrus.client.networking import ClientNetworkingGUG as NG
    from hydrus.client.networking import ClientNetworkingURLClass as U

    def qt():
        parent = QW.QWidget()
        p = U.URLClassParameterFixedName(name='page', value_string_match=S.StringMatch(match_type=S.STRING_MATCH_FLEXIBLE, match_value=S.FLEXIBLE_MATCH_NUMERIC, example_string='1'))
        p._default_value = '1'
        token = U.URLClassParameterFixedName(name='token', value_string_match=S.StringMatch())
        token._is_ephemeral = True
        cls = U.URLClass('example gallery', url_type=HC.URL_TYPE_GALLERY, url_class_key=b'\x11'*32, url_domain_mask=U.URLDomainMask(raw_domains=['example.com'], match_subdomains=True), path_components=[(S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='search', example_string='search'),None)], parameters=[p,token], example_url='http://www.example.com/search?page=2&token=secret&extra=gone#fragment', gallery_index_type=U.GALLERY_INDEX_TYPE_PARAMETER, gallery_index_identifier='page', gallery_index_delta=1)
        controller.network_engine.domain_manager.SetURLClasses([cls])
        classes = C.EditURLClassesPanel(parent,[cls])
        cp = C.EditURLClassPanel(parent,cls)
        class_cases=[]
        for example in [cls.GetExampleURL(), 'https://example.com/search?page=3', 'https://wrong.example/search?page=2']:
            cp._example_url.setText(example)
            cp._UpdateControls()
            value=cp._GetValue()
            try:
                cp.GetValue(); veto=None
            except HydrusExceptions.VetoException as e:
                veto=str(e)
            class_cases.append({'definition':value.GetSerialisableTuple(),'status':cp._example_url_classes.text(),'normalised':cp._normalised_url.text(),'request':cp._for_server_normalised_url.text(),'api':cp._api_url.text(),'referral':cp._referral_url.text(),'next':cp._next_gallery_page_url.text(),'veto':veto})
        checker=[]
        for url in ['', 'https://example.com/search?page=2','https://elsewhere.com/search']:
            classes._url_class_checker.setText(url)
            classes._UpdateURLClassCheckerText()
            checker.append({'url':url,'text':classes._url_class_checker_st.text()})
        g=NG.GalleryURLGenerator('example search',gug_key=b'\x22'*32,url_template='https://example.com/search?page=1&q=%tags%',replacement_phrase='%tags%',search_terms_separator='+',initial_search_text='search tags',example_search_text='blue_eyes 6+girls')
        gp=G.EditGUGPanel(parent,g)
        gug_cases=[]
        for phrase in ['%tags%', '', '%missing%']:
            gp._replacement_phrase.setText(phrase)
            gp._UpdateExampleURL()
            value=gp._GetValue()
            try:
                gp.GetValue(); veto=None
            except HydrusExceptions.VetoException as e:
                veto=str(e)
            gug_cases.append({'definition':value.GetSerialisableTuple(),'raw':gp._example_url.text(),'matched':gp._matched_url_class.text(),'normalised':gp._normalised_url.text(),'veto':veto})
        nested=NG.NestedGalleryURLGenerator('combined search',gug_key=b'\x33'*32,gug_keys_and_names=[g.GetGUGKeyAndName(),(b'0'*32,'missing search')])
        gl=G.EditGUGsPanel(parent,[g,nested])
        questions=[]
        original=ClientGUIDialogsQuick.GetYesNo
        def answer(parent,message,*args,**kwargs):
            questions.append(message)
            return QW.QDialog.DialogCode.Rejected
        ClientGUIDialogsQuick.GetYesNo=answer
        try:
            classes._changes_made=True
            cancelled=classes.UserIsOKToCancel()
            gl._gug_list_ctrl.SelectDatas([g])
            gl._DeleteGUG()
        finally:
            ClientGUIDialogsQuick.GetYesNo=original
        result={'class':cls.GetSerialisableTuple(),'class_row':classes._ConvertDataToDisplayTuple(cls),'class_cases':class_cases,'checker':checker,'gug':g.GetSerialisableTuple(),'gug_row':gl._ConvertGUGToDisplayTuple(g),'gug_cases':gug_cases,'nested':nested.GetSerialisableTuple(),'nested_row':gl._ConvertNGUGToDisplayTuple(nested),'questions':questions,'cancelled':cancelled}
        # Replay an accepted duplicate and dependent delete through the real
        # list handlers, with fixed generated keys for deterministic fixtures.
        from hydrus.core import HydrusData
        original_key = HydrusData.GenerateKey
        try:
            HydrusData.GenerateKey = lambda: b'\x44'*32
            gl._AddGUG(g.Duplicate())
        finally:
            HydrusData.GenerateKey = original_key
        result['after_duplicate'] = [gl._ConvertGUGToDisplayTuple(item) for item in gl._gug_list_ctrl.GetData()]
        delete_questions=[]
        def yes(parent,message,*args,**kwargs):
            delete_questions.append(message)
            return QW.QDialog.DialogCode.Accepted
        ClientGUIDialogsQuick.GetYesNo=yes
        try:
            gl._gug_list_ctrl.SelectDatas([g])
            gl._DeleteGUG()
        finally:
            ClientGUIDialogsQuick.GetYesNo=original
        result['delete_questions']=delete_questions
        result['after_delete']=[gl._ConvertGUGToDisplayTuple(item) for item in gl._gug_list_ctrl.GetData()]
        result['applied_gugs']=[item.GetSerialisableTuple() for item in gl.GetValue()]
        numeric = S.StringMatch(match_type=S.STRING_MATCH_FLEXIBLE,match_value=S.FLEXIBLE_MATCH_NUMERIC,example_string='1')
        path_rule = C.EditURLClassComponentPanel(parent,numeric,'wrong')
        try:
            path_rule.GetValue(); path_veto=None
        except HydrusExceptions.VetoException as e:
            path_veto=str(e)
        query_rule = C.EditURLClassParameterFixedNamePanel(parent,p,['token'])
        parameter_veto=[]
        for name in ['', 'token', 'page']:
            query_rule._name.setText(name)
            try:
                query_rule.GetValue(); parameter_veto.append(None)
            except HydrusExceptions.VetoException as e:
                parameter_veto.append(str(e))
        result['path_default_veto']=path_veto
        result['parameter_name_vetoes']=parameter_veto
        from hydrus.client.gui import ClientGUIDialogsMessage
        notices=[]
        original_information=ClientGUIDialogsMessage.ShowInformation
        ClientGUIDialogsMessage.ShowInformation=lambda parent,text,*args,**kwargs: notices.append(text)
        try:
            cp._should_be_associated_with_files.setChecked(True)
            cp.EventAssociationUpdate()
            cp._url_type.SetValue(HC.URL_TYPE_POST)
            cp._should_be_associated_with_files.setChecked(False)
            cp.EventAssociationUpdate()
        finally:
            ClientGUIDialogsMessage.ShowInformation=original_information
        result['association_notices']=notices
        api_cases=[]
        cp._example_url.setText(cls.GetExampleURL())
        for conversions in [[], [(S.STRING_CONVERSION_APPEND_TEXT,'&api=1')], [(S.STRING_CONVERSION_DECODE,S.ENCODING_TYPE_HEX_UTF8)]]:
            cp._api_lookup_converter.SetValue(S.StringConverter(conversions=conversions,example_string=cls.GetExampleURL()))
            cp._UpdateControls()
            try:
                cp.GetValue(); veto=None
            except HydrusExceptions.VetoException as e:
                veto=str(e)
            api_cases.append({'definition':cp._GetValue().GetSerialisableTuple(),'veto':veto,'status':cp._example_url_classes.text(),'request':cp._for_server_normalised_url.text()})
        result['api_cases']=api_cases
        result['new_class']=U.URLClass('new url class',url_class_key=b'\x55'*32).GetSerialisableTuple()
        result['new_gug']=NG.GalleryURLGenerator('new gallery url generator',gug_key=b'\x66'*32).GetSerialisableTuple()
        result['new_nested']=NG.NestedGalleryURLGenerator('new nested gallery url generator',gug_key=b'\x77'*32).GetSerialisableTuple()
        parent.deleteLater()
        return result
    return controller.CallBlockingToQt(controller.gui,qt)


def main():
    import hydrus_driver
    import record_api
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f: json.dump(result,f,ensure_ascii=False,indent=2)
        return
    with tempfile.TemporaryDirectory() as tmp:
        out=os.path.join(tmp,'result.json')
        hydrus_driver.run_in_subprocess(__file__,'--child',out)
        with open(out) as f: result=json.load(f)
    target=os.path.join(HERE,'fixtures','downloader_definitions.json')
    with open(target,'w') as f: json.dump(result,f,ensure_ascii=False,indent=2)
    print('wrote '+target)


if __name__=='__main__': main()
