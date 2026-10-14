#!/usr/bin/env python3
"""Record the reference's content parser editor edited to other settings.

Opens the real `EditContentParserPanel` on a tags parser and on a veto parser
and edits them through its widgets, as `record_parser_editors.py`'s panels
are made:

- tags: the namespace control set to "any namespace" (`None`), to the empty
  text ("unnamespaced") and to `creator`; for each, the serialised parser
  `GetValue` hands back and the tags `ParsedPost.GetTags` makes of several
  documents' texts;
- veto: the "veto if match found" box and the string match (match anything,
  or fixed `other`) edited; for each, the serialised parser and what parsing
  `blocked` and `other` does (the parsed contents, or the veto's message).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_parser_content_variants.py
       (writes fixtures/parser_content_variants.json)
"""
import json, os, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

TAG_TEXTS = ['Samus', 'series: Metroid Prime', 'Creator:Samus  Aran', '  spaced   out ']


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
        context = {'url': 'https://example.com/post/1'}

        def formula():
            return P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='p')], content_to_fetch=P.HTML_CONTENT_STRING)

        out = {'tags': [], 'veto': [], 'context': context}

        def result_of(value, text):
            try:
                post = value.Parse(dict(context), '<p>' + text + '</p>')
                return {'texts': [c.parsed_text for c in post.parsed_contents], 'tags': sorted(post.GetTags()), 'error': None}
            except Exception as e:
                return {'texts': None, 'tags': None, 'error': str(e)}

        for label, setter in (('any namespace', lambda c: c._namespace.SetValue(None)),
                              ('unnamespaced', lambda c: c._namespace.SetValue('')),
                              ('creator', lambda c: c._namespace.SetValue('creator'))):
            content = P.ContentParser(name='tags', content_type=kinds[1], formula=formula(), additional_info='artist')
            panel = G.EditContentParserPanel(root, content, P.ParsingTestData(context, ['<p>Samus</p>']), kinds)
            setter(panel)
            value = panel.GetValue()
            out['tags'].append({'edit': label, 'tuple': value.GetSerialisableTuple(), 'results': [dict(result_of(value, t), text=t) for t in TAG_TEXTS]})
            panel.deleteLater()

        fixed = S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='other')
        for polarity in (True, False):
            for matcher_name, matcher in (('match anything', S.StringMatch()), ('fixed other', fixed)):
                content = P.ContentParser(name='veto', content_type=kinds[8], formula=formula(), additional_info=(True, S.StringMatch()))
                panel = G.EditContentParserPanel(root, content, P.ParsingTestData(context, ['<p>blocked</p>']), kinds)
                panel._veto_if_matches_found.setChecked(polarity)
                panel._string_match.SetValue(matcher)
                value = panel.GetValue()
                out['veto'].append({'if_matches_found': polarity, 'matcher': matcher_name, 'tuple': value.GetSerialisableTuple(),
                                    'results': [dict(result_of(value, t), text=t) for t in ('blocked', 'other')]})
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
    with open(os.path.join(HERE, 'fixtures', 'parser_content_variants.json'), 'w') as f:
        json.dump(result, f, ensure_ascii=False, indent=1)
        f.write('\n')
    for kind in ('tags', 'veto'):
        for row in result[kind]:
            print(kind, row.get('edit') or (row['if_matches_found'], row['matcher']), [(r['text'], r['tags'], r['error']) for r in row['results']])


if __name__ == '__main__':
    main()
