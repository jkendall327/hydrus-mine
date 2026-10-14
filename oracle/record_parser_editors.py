#!/usr/bin/env python3
"""Record real Qt content/page/list and downloader URL-class-link panels.
Includes every content type, note normalization, newline policies, actual test
results/errors, page example URLs and converter preservation, and clear/edit
link actions. Uses the running reference controller on its Qt thread.
"""
import json
import os
import sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.client.gui.parsing import ClientGUIParsing as G
        from hydrus.client.gui import ClientGUIDownloaders as D
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client import ClientStrings as S
        from hydrus.core import HydrusConstants as H
        from hydrus.client.networking import ClientNetworkingURLClass as U
        root = session.controller.gui
        kinds = [H.CONTENT_TYPE_URLS, H.CONTENT_TYPE_MAPPINGS, H.CONTENT_TYPE_NOTES,
                 H.CONTENT_TYPE_HASH, H.CONTENT_TYPE_TIMESTAMP, H.CONTENT_TYPE_TITLE,
                 H.CONTENT_TYPE_HTTP_HEADERS, H.CONTENT_TYPE_VARIABLE, H.CONTENT_TYPE_VETO]
        cases = [('urls', kinds[0], (H.URL_TYPE_DESIRED, 50), '../image.png'),
                 ('tags', kinds[1], 'artist', 'Samus'),
                 ('notes', kinds[2], '', 'first\n\nsecond'),
                 ('file hash', kinds[3], ('sha256', 'hex'), '00' * 32),
                 ('timestamp', kinds[4], H.TIMESTAMP_TYPE_MODIFIED_DOMAIN, '1700000000'),
                 ('watcher title', kinds[5], 75, 'a &amp; b'),
                 ('http headers', kinds[6], 'Authorization', 'token'),
                 ('temporary variable', kinds[7], 'token', 'abc'),
                 ('veto', kinds[8], (True, S.StringMatch()), 'blocked')]
        context = {'url': 'https://example.com/post/1'}
        out = []
        parsers = []
        for label, kind, info, text in cases:
            formula = P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='p')], content_to_fetch=P.HTML_CONTENT_STRING)
            content = P.ContentParser(name=label, content_type=kind, formula=formula, additional_info=info)
            panel = G.EditContentParserPanel(root, content, P.ParsingTestData(context, ['<p>' + text + '</p>']), kinds)
            if label == 'notes': panel._note_name.setText('')
            value = panel.GetValue()
            panel._test_panel._SetExampleData('<p>' + text + '</p>')
            panel._test_panel.TestParse()
            parsed = None
            error = None
            try: parsed = [c.parsed_text for c in value.Parse(dict(context), '<p>' + text + '</p>').parsed_contents]
            except Exception as e: error = str(e)
            out.append({'case': 'content', 'kind': label, 'text': text,
                        'tuple': value.GetSerialisableTuple(), 'results': parsed,
                        'error': error, 'preview': panel._test_panel._results.toPlainText(),
                        'collapse_newlines': panel._test_panel._collapse_newlines})
            parsers.append(value)
            panel.deleteLater()
        page = P.PageParser(name='test page', content_parsers=parsers[:3], example_urls=['https://example.com/post/1'])
        panel = G.EditPageParserPanel(root, page)
        panel._name.setText('edited page')
        out.append({'case': 'page', 'tuple': panel.GetValue().GetSerialisableTuple(), 'example_urls': panel.GetValue().GetExampleURLs(), 'content_count': len(panel.GetValue().GetContentParsers()[1])})
        panel.deleteLater()
        named = G.EditParsersPanel(root, [page])
        out.append({'case': 'list', 'row': named._ConvertParserToDisplayTuple(page)})
        named.deleteLater()
        url_class = U.URLClass(name='test post', url_type=H.URL_TYPE_POST, url_domain_mask=U.URLDomainMask(raw_domains=['example.com']), example_url='https://example.com/post/1')
        links = D.EditURLClassLinksPanel(root, session.controller.network_engine, [url_class], [page], {url_class.GetClassKey(): page.GetParserKey()})
        row = links._parser_list_ctrl.GetData()[0]
        out.append({'case': 'link', 'row': links._ConvertParserDataToDisplayTuple(row), 'linked': len(links.GetValue())})
        links._parser_list_ctrl.SelectDatas([row])
        from hydrus.client.gui import ClientGUIDialogsQuick as Q
        from qtpy import QtWidgets as QW
        Q.GetYesNo = lambda *args, **kwargs: QW.QDialog.DialogCode.Accepted
        links._ClearParser()
        out.append({'case': 'clear_link', 'linked': len(links.GetValue())})
        links.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f: json.dump(result, f, ensure_ascii=False, indent=2)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import tempfile
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'parser_editors.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'parser_editors.json'), 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=2)
        f.write('\n')
    print('recorded', len(result), 'parser editor states')

if __name__ == '__main__': main()
