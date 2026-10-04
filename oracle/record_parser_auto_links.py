#!/usr/bin/env python3
"""Record actual auto-fill gaps button and API/redirect pair review with synthetic objects."""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def qt():
        from qtpy import QtWidgets as QW
        from hydrus.client import ClientStrings as S
        from hydrus.client.gui import ClientGUIDownloaders as G
        from hydrus.client.networking import ClientNetworkingURLClass as U, ClientNetworkingDomain as D
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as HC
        def url_class(name, path, **kw):
            return U.URLClass(name=name,url_type=HC.URL_TYPE_POST,url_domain_mask=U.URLDomainMask(raw_domains=['links.example']),path_components=[(S.StringMatch(match_type=S.STRING_MATCH_FIXED,match_value=path),None)],parameters=[],example_url=f'https://links.example/{path}',**kw)
        direct=url_class('direct','direct'); target=url_class('API target','api')
        source=url_class('redirect source','redirect',api_lookup_converter=S.StringConverter(conversions=[(S.STRING_CONVERSION_REGEX_SUB,('/redirect','/api'))]))
        classes=[direct,source,target]
        parsers=[P.PageParser(name='direct parser',example_urls=['https://links.example/direct']),P.PageParser(name='API parser',example_urls=['https://links.example/api'])]
        keys={o.GetClassKey():o.GetName() for o in classes};parser_keys={p.GetParserKey():p.GetName() for p in parsers}
        out={'classes':[o.GetSerialisableTuple() for o in classes],'parsers':[p.GetSerialisableTuple() for p in parsers], 'cases':[]}
        broken=url_class('broken converter','broken',api_lookup_converter=S.StringConverter(conversions=[(S.STRING_CONVERSION_REGEX_SUB,('[','api'))]))
        out['invalid_classes']=[broken.GetSerialisableTuple(),target.GetSerialisableTuple()]
        out['invalid_api_pairs']=[[a.GetName(),b.GetName()] for a,b in U.ConvertURLClassesIntoAPIPairs([broken,target])]
        from qtpy import QtCore as QC
        for name, existing in [('empty',{}),('linked',{direct.GetClassKey():parsers[0].GetParserKey()}),('installed',{direct.GetClassKey():parsers[0].GetParserKey(),target.GetClassKey():parsers[1].GetParserKey()})]:
            candidates=D.NetworkDomainManager.STATICLinkURLClassesAndParsers(classes,parsers,existing)
            panel=G.EditURLClassLinksPanel(c.gui,c.network_engine,classes,parsers,existing)
            QW.QApplication.processEvents()
            result={'name':name,'existing':sorted([keys[k],parser_keys[v]] for k,v in existing.items()),'candidates':sorted([keys[k],parser_keys[v]] for k,v in candidates.items()),'tabs':[panel._notebook.tabText(i) for i in range(panel._notebook.count())],'api_pairs':[list(panel._ConvertAPIPairDataToDisplayTuple(row)) for row in panel._api_pairs_list_ctrl.GetData()], 'steps':[]}
            model=panel._api_pairs_list_ctrl.model()
            result['api_columns']=[model.headerData(i,QC.Qt.Orientation.Horizontal) for i in range(model.columnCount())]
            button=next(b for b in panel.findChildren(QW.QPushButton) if b.text()=='try to fill in gaps based on example urls')
            def state(action):return {'action':action,'enabled':button.isEnabled(),'gaps_exist':panel._GapsExist(),'rows':[list(panel._ConvertParserDataToDisplayTuple(row)) for row in panel._parser_list_ctrl.GetData()],'value':sorted([keys[k],parser_keys[v]] for k,v in panel.GetValue().items())}
            result['steps'].append(state('initial'));button.click();result['steps'].append(state('auto fill'))
            panel._notebook.setCurrentIndex(1);result['selected_tab']=panel._notebook.tabText(panel._notebook.currentIndex())
            panel.deleteLater();out['cases'].append(result)
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
    with open(os.path.join(HERE,'fixtures','parser_auto_links.json'),'w') as f:json.dump(value,f,indent=1);f.write('\n')
    print('recorded actual auto-fill button and API pair tab')


if __name__=='__main__':main()
