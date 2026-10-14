#!/usr/bin/env python3
"""Record what the parser test panel's example parsing context does.

The real `EditContentParserPanel`'s test panel is given an example parsing
context (its "edit example parsing context" dictionary) and a document, and
"test parse" is pressed: relative urls resolved against the context's `url`,
a context variable formula reading `token` (present, absent), and one
reading `post_index` (which the reference's test panel removes from the
context before it parses). Recorded for each: the context, the document, the
serialised parser and the results text the panel shows.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_parser_context.py
       (writes fixtures/parser_context.json)
"""
import json, os, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    def qt():
        from hydrus.client.gui.parsing import ClientGUIParsing as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.core import HydrusConstants as H
        root = session.controller.gui
        kinds = [H.CONTENT_TYPE_URLS, H.CONTENT_TYPE_MAPPINGS, H.CONTENT_TYPE_NOTES,
                 H.CONTENT_TYPE_HASH, H.CONTENT_TYPE_TIMESTAMP, H.CONTENT_TYPE_TITLE,
                 H.CONTENT_TYPE_HTTP_HEADERS, H.CONTENT_TYPE_VARIABLE, H.CONTENT_TYPE_VETO]

        def html():
            return P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='p')], content_to_fetch=P.HTML_CONTENT_STRING)

        urls = P.ContentParser(name='urls', content_type=kinds[0], formula=P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='p')], content_to_fetch=P.HTML_CONTENT_STRING), additional_info=(H.URL_TYPE_DESIRED, 50))
        token = P.ContentParser(name='token', content_type=kinds[1], formula=P.ParseFormulaContextVariable(variable_name='token'), additional_info='')
        post_index = P.ContentParser(name='index', content_type=kinds[1], formula=P.ParseFormulaContextVariable(variable_name='post_index'), additional_info='')
        cases = [
            ('relative url, post url as the url', urls, {'url': 'https://example.com/post/1'}, '<p>../image.png</p>'),
            ('relative url, another url', urls, {'url': 'https://other.example/a/b/c/d'}, '<p>../image.png</p>'),
            ('relative url, no url', urls, {}, '<p>../image.png</p>'),
            ('variable present', token, {'url': 'https://example.com/post/1', 'token': 'abc'}, '<p>ignored</p>'),
            ('variable absent', token, {'url': 'https://example.com/post/1'}, '<p>ignored</p>'),
            ('post index named', post_index, {'url': 'https://example.com/post/1', 'post_index': '3'}, '<p>ignored</p>'),
        ]
        out = []
        for name, content, context, text in cases:
            panel = G.EditContentParserPanel(root, content, P.ParsingTestData({}, [text]), kinds)
            panel._test_panel._example_parsing_context.SetValue(dict(context))
            panel._test_panel._SetExampleData(text)
            panel._test_panel.TestParse()
            out.append({'name': name, 'tuple': content.GetSerialisableTuple(), 'context': context, 'document': text,
                        'results': panel._test_panel._results.toPlainText()})
            panel.deleteLater()
        return out
    return session.controller.CallBlockingToQt(session.controller.gui, qt)


def child(out):
    import hydrus_driver, record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f:
        json.dump(result, f, ensure_ascii=False)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        child(sys.argv[2])
        return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, 'r.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', path)
        with open(path) as f:
            result = json.load(f)
    with open(os.path.join(HERE, 'fixtures', 'parser_context.json'), 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=1)
        f.write('\n')
    for r in result:
        print(r['name'], '->', repr(r['results'][:300]))


if __name__ == '__main__':
    main()
