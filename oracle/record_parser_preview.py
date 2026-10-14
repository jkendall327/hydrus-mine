#!/usr/bin/env python3
"""Record the parser test panel's results text for each kind of content.

The real `EditContentParserPanel` test panel is given one content parser per
case (every content type, with its options) and a document, 'test parse' is
pressed, and the results text is recorded: the header and footer with the
number of results, the lines in sorted order, how each kind words its line
(`ParsableContentDescription.ToString`), the veto message and no results.
The real `EditPageParserPanel` test panel does the same for page parsers with
two posts, none, and a veto: the posts' lines sorted within each post and the
posts separated by the reference's break.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_parser_preview.py
       (writes fixtures/parser_preview.json)
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'parser_preview.json')


def record(session):
    def qt():
        from hydrus.client.gui.parsing import ClientGUIParsing as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client import ClientStrings as S
        from hydrus.core import HydrusConstants as H
        root = session.controller.gui
        kinds = [H.CONTENT_TYPE_URLS, H.CONTENT_TYPE_MAPPINGS, H.CONTENT_TYPE_NOTES,
                 H.CONTENT_TYPE_HASH, H.CONTENT_TYPE_TIMESTAMP, H.CONTENT_TYPE_TITLE,
                 H.CONTENT_TYPE_HTTP_HEADERS, H.CONTENT_TYPE_VARIABLE, H.CONTENT_TYPE_VETO]

        def formula():
            return P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='p')], content_to_fetch=P.HTML_CONTENT_STRING)

        def content(name, content_type, info):
            return P.ContentParser(name=name, content_type=content_type, formula=formula(), additional_info=info)

        sha = '896aba496da94160475f7ac956beace2083733b5a2972ffd3053dd3d0ad1d36b'
        match = S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='nope', example_string='nope')
        cases = [
            ('desired url', content('u', H.CONTENT_TYPE_URLS, (H.URL_TYPE_DESIRED, 50)), '<p>https://example.com/a</p>'),
            ('source url', content('u', H.CONTENT_TYPE_URLS, (H.URL_TYPE_SOURCE, 60)), '<p>https://example.com/a</p>'),
            ('next page url', content('u', H.CONTENT_TYPE_URLS, (H.URL_TYPE_NEXT, 70)), '<p>https://example.com/a</p>'),
            ('sub-gallery url', content('u', H.CONTENT_TYPE_URLS, (H.URL_TYPE_SUB_GALLERY, 80)), '<p>https://example.com/a</p>'),
            ('namespaced tag', content('t', H.CONTENT_TYPE_MAPPINGS, 'series'), '<p>Foo  Bar</p>'),
            ('unnamespaced tag', content('t', H.CONTENT_TYPE_MAPPINGS, ''), '<p>Foo  Bar</p>'),
            ('tag keeping its namespace', content('t', H.CONTENT_TYPE_MAPPINGS, None), '<p>Creator:Alice</p>'),
            ('tag with a colon, unnamespaced', content('t', H.CONTENT_TYPE_MAPPINGS, ''), '<p>a:b</p>'),
            ('note', content('n', H.CONTENT_TYPE_NOTES, 'summary'), '<p>hello there</p>'),
            ('sha256 hash, hex', content('h', H.CONTENT_TYPE_HASH, ('sha256', 'hex')), '<p>' + sha + '</p>'),
            ('md5 hash, base64', content('h', H.CONTENT_TYPE_HASH, ('md5', 'base64')), '<p>aGVsbG8gd29ybGQhISE=</p>'),
            ('hash that is not one', content('h', H.CONTENT_TYPE_HASH, ('sha256', 'hex')), '<p>zz</p>'),
            ('timestamp', content('s', H.CONTENT_TYPE_TIMESTAMP, H.TIMESTAMP_TYPE_MODIFIED_DOMAIN), '<p>1700000000</p>'),
            ('timestamp that is not one', content('s', H.CONTENT_TYPE_TIMESTAMP, H.TIMESTAMP_TYPE_MODIFIED_DOMAIN), '<p>soon</p>'),
            ('title', content('w', H.CONTENT_TYPE_TITLE, 3), '<p>A thread</p>'),
            ('http header', content('x', H.CONTENT_TYPE_HTTP_HEADERS, 'X-Test'), '<p>value</p>'),
            ('temp variable', content('v', H.CONTENT_TYPE_VARIABLE, 'token'), '<p>abc</p>'),
            ('veto when found', content('stop here', H.CONTENT_TYPE_VETO, (True, match)), '<p>nope</p>'),
            ('veto not met', content('stop here', H.CONTENT_TYPE_VETO, (True, match)), '<p>fine</p>'),
            ('two results, sorted', content('t', H.CONTENT_TYPE_MAPPINGS, 'series'), '<p>b</p><p>a</p>'),
            ('no results', content('t', H.CONTENT_TYPE_MAPPINGS, 'series'), '<p></p>'),
        ]
        out = {'content': [], 'page': []}
        for name, parser, text in cases:
            panel = G.EditContentParserPanel(root, parser, P.ParsingTestData({}, [text]), kinds)
            panel._test_panel._SetExampleData(text)
            panel._test_panel.TestParse()
            out['content'].append({'name': name, 'tuple': parser.GetSerialisableTuple(), 'document': text,
                                   'results': panel._test_panel._results.toPlainText()})
            panel.deleteLater()

        def post_formula():
            return P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='div', tag_attributes={'class': 'post'})], content_to_fetch=P.HTML_CONTENT_HTML)

        def inner(name, tag_name, content_type, info):
            f = P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name=tag_name)], content_to_fetch=P.HTML_CONTENT_STRING)
            return P.ContentParser(name=name, content_type=content_type, formula=f, additional_info=info)

        post_page = P.PageParser('post', content_parsers=[inner('tag', 'b', H.CONTENT_TYPE_MAPPINGS, 'series'), inner('file', 'i', H.CONTENT_TYPE_URLS, (H.URL_TYPE_DESIRED, 50)), inner('another', 'u', H.CONTENT_TYPE_MAPPINGS, '')])
        two = P.PageParser('two posts', subsidiary_page_parsers=[P.SubsidiaryPageParser(formula=post_formula(), page_parser=post_page)])
        veto_page = P.PageParser('vetoing', content_parsers=[inner('stop', 'b', H.CONTENT_TYPE_VETO, (True, match))])
        page_cases = [
            ('two posts', two, '<div class="post"><b>one</b><i>https://example.com/1</i><u>zeta</u><u>alpha</u></div><div class="post"><b>two</b><i>https://example.com/2</i></div>'),
            ('no posts', two, '<p>nothing</p>'),
            ('one post', two, '<div class="post"><b>solo</b></div>'),
            ('a veto', veto_page, '<b>nope</b>'),
        ]
        for name, parser, text in page_cases:
            panel = G.EditPageParserPanel(root, parser, test_data=P.ParsingTestData({}, [text]))
            panel._test_panel._SetExampleData(text)
            panel._test_panel.TestParse()
            out['page'].append({'name': name, 'tuple': parser.GetSerialisableTuple(), 'document': text,
                                'results': panel._test_panel._results.toPlainText()})
            panel.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        output = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(output, 'w') as f:
            json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(OUT, 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote', OUT)


if __name__ == '__main__':
    main()
