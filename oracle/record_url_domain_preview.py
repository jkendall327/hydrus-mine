#!/usr/bin/env python3
"""Record actual domain-mask mode/test controls and selectable class preview outputs."""
import json
import os
import sys
import tempfile
HERE=os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0,HERE)


def record(session):
    c=session.controller
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui.panels import ClientGUIURLClass as C
        from hydrus.client.networking import ClientNetworkingURLClass as U
        from hydrus.core import HydrusConstants as HC
        out={'domain_steps':[],'preview_steps':[]}
        mask=U.URLDomainMask(raw_domains=['mask.example'])
        panel=C.EditURLDomainMaskWidget(c.gui,mask)
        def snap(action):
            value=panel.GetValue()
            out['domain_steps'].append({'action':action,'mode':panel._widget_mode.currentIndex(),'mode_enabled':panel._widget_mode.isEnabled(),'choices':[panel._widget_mode.itemText(i) for i in range(panel._widget_mode.count())],'raw':value.GetRawDomains(),'regex':value.GetDomainRegexes(),'match':value.match_subdomains,'keep':value.keep_matched_subdomains,'keep_enabled':panel._keep_matched_subdomains.isEnabled(),'test':panel._test_input.text(),'status':panel._test_st.text(),'normalised':panel._normalised_domain.text(),'readonly':panel._normalised_domain.isReadOnly()})
        snap('initial')
        for domain in [' mask.example ','www2.mask.example','img.mask.example','other.example','']:
            panel._test_input.setText(domain);snap('test')
        panel._match_subdomains.click();panel._test_input.setText('img.mask.example');snap('match subdomains')
        panel._keep_matched_subdomains.click();snap('keep subdomains')
        panel._match_subdomains.click();snap('disable match retains keep')
        panel._widget_mode.setCurrentIndex(1);snap('full')
        panel._raw_domain_add_remove_list.AddDatas(['second.example']);panel._UpdateAfterFullChange();snap('multiple')
        panel._domain_regex_add_remove_list.AddDatas([r'img\d+\.cdn\.example']);panel._UpdateAfterFullChange();panel._test_input.setText('img3.cdn.example');snap('regex')
        panel._domain_regex_add_remove_list.Clear();panel._raw_domain_add_remove_list.Clear();panel._raw_domain_add_remove_list.AddDatas(['mask.example']);panel._UpdateAfterFullChange();snap('single again')
        panel._widget_mode.setCurrentIndex(0);panel._raw_domain_text_input.setText('new.example');panel._test_input.setText('new.example');snap('simple edit')
        panel._raw_domain_text_input.setText('');snap('empty simple')
        cls=U.URLClass(name='preview',url_type=HC.URL_TYPE_GALLERY,url_domain_mask=U.URLDomainMask(raw_domains=['preview.example']),path_components=[(S.StringMatch(match_type=S.STRING_MATCH_FIXED,match_value='search'),None)],parameters=[U.URLClassParameterFixedName(name='page',value_string_match=S.StringMatch(match_type=S.STRING_MATCH_FLEXIBLE,match_value=S.FLEXIBLE_MATCH_NUMERIC,example_string='1'))],gallery_index_type=U.GALLERY_INDEX_TYPE_PARAMETER,gallery_index_identifier='page',gallery_index_delta=1,example_url='https://preview.example/search?page=2')
        editor=C.EditURLClassPanel(c.gui,cls)
        for name,url in [('valid',cls.GetExampleURL()),('next','https://preview.example/search?page=9'),('invalid','https://other.example/search?page=2')]:
            editor._example_url.setText(url);editor._UpdateControls()
            controls=[editor._normalised_url,editor._for_server_normalised_url,editor._api_url,editor._referral_url,editor._next_gallery_page_url]
            out['preview_steps'].append({'name':name,'class':editor._GetValue().GetSerialisableTuple(),'status':editor._example_url_classes.text(),'outputs':[ctrl.text() for ctrl in controls],'readonly':[ctrl.isReadOnly() for ctrl in controls]})
        editor.deleteLater()
        out['extra_previews']=[]
        for name,kw in [
            ('api',{'api_lookup_converter':S.StringConverter(conversions=[(S.STRING_CONVERSION_REGEX_SUB,('/search','/api'))])}),
            ('referral',{'send_referral_url':U.SEND_REFERRAL_URL_ONLY_CONVERTER,'referral_url_converter':S.StringConverter(conversions=[(S.STRING_CONVERSION_REGEX_SUB,('/search','/referral'))])}),
        ]:
            item=U.URLClass(name=name,url_type=HC.URL_TYPE_GALLERY,url_domain_mask=U.URLDomainMask(raw_domains=['preview.example']),path_components=[(S.StringMatch(match_type=S.STRING_MATCH_FIXED,match_value='search'),None)],parameters=[U.URLClassParameterFixedName(name='page',value_string_match=S.StringMatch(match_type=S.STRING_MATCH_FLEXIBLE,match_value=S.FLEXIBLE_MATCH_NUMERIC,example_string='1'))],example_url='https://preview.example/search?page=2',**kw)
            e=C.EditURLClassPanel(c.gui,item)
            controls=[e._normalised_url,e._for_server_normalised_url,e._api_url,e._referral_url,e._next_gallery_page_url]
            out['extra_previews'].append({'name':name,'class':e._GetValue().GetSerialisableTuple(),'status':e._example_url_classes.text(),'outputs':[ctrl.text() for ctrl in controls],'readonly':[ctrl.isReadOnly() for ctrl in controls]})
            e.deleteLater()
        panel.deleteLater()
        return out
    return c.CallBlockingToQt(c.gui,qt)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2];value=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(value,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path=os.path.join(tmp,'out.json');hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f:value=json.load(f)
    with open(os.path.join(HERE,'fixtures/url_domain_preview.json'),'w') as f:json.dump(value,f,indent=1);f.write('\n')
    print('recorded domain mask modes/test and readonly URL previews')


if __name__=='__main__':main()
