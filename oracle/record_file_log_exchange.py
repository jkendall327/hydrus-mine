#!/usr/bin/env python3
"""Drive actual file-log clipboard imports and the selected-URL search menu."""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def work():
        from hydrus.client.gui.importing import ClientGUIFileSeedCache as W
        from hydrus.client.importing import ClientImportFileSeeds as S
        from hydrus.client.gui import ClientGUIDialogsMessage as M, ClientGUIDialogsQuick as Q
        old_clip, old_pub, old_critical, old_parse = c.GetClipboardText, c.pub, M.ShowCritical, Q.PresentClipboardParseError
        out = {'imports': [], 'errors': [], 'search': []}
        c.pub = lambda *a, **kw: out['search'].append({'topic':a[0], 'location':a[1].GetSerialisableTuple(), 'predicates':[p.GetSerialisableTuple() for p in kw['initial_predicates']], 'name':kw['page_name'], 'activate':kw['activate_window']}) if a and a[0] == 'new_page_query' else None
        M.ShowCritical = lambda win, title, text: out['errors'].append([title, text])
        Q.PresentClipboardParseError = lambda win, raw, expected, e: out['errors'].append([expected, type(e).__name__])
        try:
            cases = [('urls', '\ufeff https://clipboard.example/a b#frag\r\n\nhttps://clipboard.example/a%20b\nhttps://clipboard.example/日'), ('paths', '/synthetic/a.jpg\n/synthetic/a.jpg\n /synthetic/b.jpg '), ('url_first_mixed', 'https://clipboard.example/a\n/synthetic/a.jpg'), ('path_first_mixed', '/synthetic/a.jpg\nhttps://clipboard.example/a b'), ('empty', ' \n\t')]
            for name, raw in cases:
                cache = S.FileSeedCache()
                c.GetClipboardText = lambda raw=raw: raw
                W.ImportFromClipboard(c.gui, cache)
                W.ImportFromClipboard(c.gui, cache)
                seeds = cache.GetFileSeeds()
                out['imports'].append({'name': name, 'raw':raw, 'seeds':[[s.file_seed_type,s.file_seed_data,s.file_seed_data_for_comparison,s.status] for s in seeds]})
            from hydrus.core import HydrusExceptions
            def missing():
                raise HydrusExceptions.DataMissing('synthetic clipboard unavailable')
            c.GetClipboardText = missing
            W.ImportFromClipboard(c.gui, S.FileSeedCache())
            cache = S.FileSeedCache()
            W.ImportSources(cache, ['https://clipboard.example/a', 'https://clipboard.example/b'])
            panel = W.EditFileSeedCachePanel(c.gui, cache)
            panel._list_ctrl.SelectDatas(cache.GetFileSeeds(), deselect_others=True)
            menu = panel._GetListCtrlMenu()
            next(a for a in menu.actions() if a.text() == 'search for URLs').trigger()
            panel.close()
        finally:
            c.GetClipboardText, c.pub, M.ShowCritical, Q.PresentClipboardParseError = old_clip, old_pub, old_critical, old_parse
        return out
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output = sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f: json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as tmp:
        path=os.path.join(tmp,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',path)
        with open(path) as f: result=json.load(f)
    with open(os.path.join(HERE,'fixtures','file_log_exchange.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('recorded file-log imports and URL search')


if __name__=='__main__': main()
