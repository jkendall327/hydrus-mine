#!/usr/bin/env python3
"""Record the parser test panels' results text for the cases
`record_parser_preview.py` leaves out (issue #221).

The real `EditContentParserPanel`, `EditPageParserPanel` and
`EditSubsidiaryPageParserPanel` test panels are given a parser and a document,
'test parse' is pressed (`TestPanel.TestParse`), and the results text is
recorded:

- content parsers: timestamps in a zone that is not UTC (Asia/Kolkata, the machine's local
  time, with the zone's current offset recorded), timestamps that are not
  numbers or are out of range, hashes that decode one way or another or not at
  all (Python's `bytes.fromhex` and `base64.b64decode` errors), tags that clean
  to nothing, url priorities sorted, a parse error (the panel's "Exception:"
  text) and no document ("Nothing to parse!");
- page parsers: top-level content alone, top-level content merged into each
  subsidiary post, two subsidiaries in name order, a subsidiary inside a
  subsidiary, a veto inside one post, posts sorted by source time, an error
  inside a page, and a thousand posts (the count's thousands separator);
- the subsidiary page parser editor's own panel, which parses each text and
  joins the results with a blank line, and shows nothing for an empty text.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_parser_preview_format.py
       (writes fixtures/parser_preview_format.json)
"""
import json
import os
import sys
import tempfile
import time

# a zone that is not UTC and has no summer time, so the test panel's local
# time shows and its offset is the same whenever the recording is replayed
os.environ['TZ'] = 'Asia/Kolkata'
time.tzset()
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'parser_preview_format.json')


def results_text(panel):
    """The panel's text, with a Python traceback cut to its first line (its
    file paths and line numbers are the reference's own)."""
    text = panel._test_panel._results.toPlainText()
    marker = '\nTraceback (most recent call last):\n'
    if text.startswith('Exception:') and marker in text:
        text = text[:text.index(marker) + len(marker) - 1]
    return text


def record(session):
    def qt():
        import datetime
        from hydrus.client.gui.parsing import ClientGUIParsing as G
        from hydrus.client.parsing import ClientParsing as P
        from hydrus.client import ClientStrings as S
        from hydrus.core import HydrusConstants as H
        root = session.controller.gui
        kinds = [H.CONTENT_TYPE_URLS, H.CONTENT_TYPE_MAPPINGS, H.CONTENT_TYPE_NOTES,
                 H.CONTENT_TYPE_HASH, H.CONTENT_TYPE_TIMESTAMP, H.CONTENT_TYPE_TITLE,
                 H.CONTENT_TYPE_HTTP_HEADERS, H.CONTENT_TYPE_VARIABLE, H.CONTENT_TYPE_VETO]

        def formula(tag_name='p'):
            return P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name=tag_name)], content_to_fetch=P.HTML_CONTENT_STRING)

        def content(name, content_type, info, tag_name='p'):
            return P.ContentParser(name=name, content_type=content_type, formula=formula(tag_name), additional_info=info)

        def json_content(name):
            return P.ContentParser(name=name, content_type=H.CONTENT_TYPE_MAPPINGS, formula=P.ParseFormulaJSON(), additional_info='')

        def p(*texts):
            return ''.join('<p>' + t + '</p>' for t in texts)

        stamp = (H.CONTENT_TYPE_TIMESTAMP, H.TIMESTAMP_TYPE_MODIFIED_DOMAIN)
        hex256 = (H.CONTENT_TYPE_HASH, ('sha256', 'hex'))
        b64md5 = (H.CONTENT_TYPE_HASH, ('md5', 'base64'))
        cases = [
            ('local timestamp', content('s', *stamp), p('1700000000')),
            ('local timestamp in summer', content('s', *stamp), p('1690000000')),
            ('timestamp before 1970', content('s', *stamp), p('-86400')),
            ('timestamp with a sign and leading zeros', content('s', *stamp), p('+0042')),
            ('timestamp with underscores', content('s', *stamp), p('1_700_000_000')),
            ('timestamp too large for a date', content('s', *stamp), p('99999999999999')),
            ('timestamp too large for any integer type', content('s', *stamp), p('123456789012345678901234567890')),
            ('timestamp in year one', content('s', *stamp), p('-62135596800')),
            ('timestamp past year 9999 in local time', content('s', *stamp), p('253402300799')),
            ('timestamp with doubled underscores', content('s', *stamp), p('1__2')),
            ('timestamp of minus zero', content('s', *stamp), p('-000')),
            ('timestamp with a decimal point', content('s', *stamp), p('1700000000.5')),
            ('hex hash of one digit', content('h', *hex256), p('a')),
            ('hex hash with a vertical tab', content('h', *hex256), p('ab\x0bcd')),
            ('hex hash with an ideographic space', content('h', *hex256), p('ab\u3000cd')),
            ('hex hash with spaces', content('h', *hex256), p('ab cd\tef')),
            ('hex hash of odd length', content('h', *hex256), p('abc')),
            ('hex hash that is base64', content('h', *hex256), p('aGVsbG8=')),
            ('hex hash with a bad second digit', content('h', *hex256), p('a z')),
            ('hex hash with a non-ascii letter', content('h', *hex256), p('abé1')),
            ('base64 hash with junk', content('h', *b64md5), p('aGVs!bG8=')),
            ('base64 hash one too long', content('h', *b64md5), p('abcde')),
            ('base64 hash without padding', content('h', *b64md5), p('abc')),
            ('base64 hash with text after padding', content('h', *b64md5), p('QQ==QUJD')),
            ('base64 hash with data after one pad', content('h', *b64md5), p('QQ=a=')),
            ('base64 hash of two padded groups', content('h', *b64md5), p('QQ==QQ==')),
            ('base64 hash with data after completed padding', content('h', *b64md5), p('QQ=a=b==')),
            ('base64 hash with padding first', content('h', *b64md5), p('=QUJD')),
            ('base64 hash of one character and pads', content('h', *b64md5), p('Q===')),
            ('base64 hash with pads between quads', content('h', *b64md5), p('QUJD====QQ')),
            ('base64 hash with a pad inside a quad', content('h', *b64md5), p('QU=JD')),
            ('base64 hash with a non-ascii letter', content('h', *b64md5), p('aGVsbG8é')),
            ('tag of a namespace alone', content('t', H.CONTENT_TYPE_MAPPINGS, None), p('series:')),
            ('tag of a colon alone', content('t', H.CONTENT_TYPE_MAPPINGS, None), p(':')),
            ('tag of a leading colon', content('t', H.CONTENT_TYPE_MAPPINGS, None), p(':D')),
            ('tag with surrounding gumpf', content('t', H.CONTENT_TYPE_MAPPINGS, 'creator'), p('  Some   Body  ')),
            ('urls of one priority, sorted', content('u', H.CONTENT_TYPE_URLS, (H.URL_TYPE_DESIRED, 25)), p('https://example.com/b', 'https://example.com/a')),
            ('a parse error', json_content('j'), '<html>' + p('not json') + '</html>'),
            ('no document', content('t', H.CONTENT_TYPE_MAPPINGS, ''), ''),
        ]
        offset = datetime.datetime.now().astimezone().utcoffset().total_seconds()
        out = {'zone': os.environ['TZ'], 'offset_seconds': int(offset), 'content': [], 'page': [], 'subsidiary': []}
        for name, parser, text in cases:
            panel = G.EditContentParserPanel(root, parser, P.ParsingTestData({}, [text]), kinds)
            panel._test_panel._SetExampleData(text)
            panel._test_panel.TestParse()
            out['content'].append({'name': name, 'tuple': parser.GetSerialisableTuple(), 'document': text,
                                   'results': results_text(panel)})
            panel.deleteLater()

        def div(class_name):
            return P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='div', tag_attributes={'class': class_name})], content_to_fetch=P.HTML_CONTENT_HTML)

        def sub(page, class_name='post', sort=False):
            return P.SubsidiaryPageParser(formula=div(class_name), sort_posts_by_source_time=sort, page_parser=page)

        def page(name, contents=(), subs=()):
            return P.PageParser(name, content_parsers=list(contents), subsidiary_page_parsers=list(subs))

        match = S.StringMatch(match_type=S.STRING_MATCH_FIXED, match_value='nope', example_string='nope')
        tag_b = content('tag', H.CONTENT_TYPE_MAPPINGS, 'series', 'b')
        url_i = content('file', H.CONTENT_TYPE_URLS, (H.URL_TYPE_DESIRED, 50), 'i')
        title_h = content('title', H.CONTENT_TYPE_TITLE, 7, 'h1')
        time_s = content('time', *stamp, tag_name='s')
        veto_q = content('stop', H.CONTENT_TYPE_VETO, (True, match), 'q')

        top_only = page('top only', [title_h, tag_b])
        merged = page('merged', [title_h], [sub(page('post', [tag_b, url_i]))])
        two_subs = page('two subsidiaries', subs=[sub(page('b posts', [tag_b]), 'b'), sub(page('A posts', [url_i]), 'a')])
        nested = page('nested', [title_h], [sub(page('thread', [tag_b], [sub(page('reply', [url_i]), 'reply')]), 'thread')])
        vetoing = page('veto in a post', subs=[sub(page('post', [tag_b, veto_q]))])
        by_time = page('sorted by time', subs=[sub(page('post', [tag_b, time_s]), sort=True)])
        failing = page('failing', [title_h, json_content('json')])
        many = page('many', subs=[sub(page('post', [tag_b]))])

        posts = '<div class="post"><b>one</b><i>https://example.com/1</i></div><div class="post"><b>two</b><i>https://example.com/2</i></div>'
        page_cases = [
            ('top-level content alone', top_only, '<h1>Thread</h1><b>x</b><b>y</b>'),
            ('top-level content with no results', top_only, '<p>nothing</p>'),
            ('top-level content merged into each post', merged, '<h1>Thread</h1>' + posts),
            ('two subsidiaries in name order', two_subs, '<div class="b"><b>bee</b></div><div class="a"><i>https://example.com/a</i></div>'),
            ('a subsidiary in a subsidiary', nested, '<h1>Top</h1><div class="thread"><b>t1</b><div class="reply"><i>https://example.com/r1</i></div><div class="reply"><i>https://example.com/r2</i></div></div><div class="thread"><b>t2</b></div>'),
            ('a veto in one post', vetoing, '<div class="post"><b>kept</b></div><div class="post"><b>dropped</b><q>nope</q></div>'),
            ('posts sorted by source time', by_time, '<div class="post"><b>old</b><s>1600000000</s></div><div class="post"><b>new</b><s>1700000000</s></div>'),
            ('an error inside a page', failing, '<html><h1>Thread</h1></html>'),
            ('a thousand posts', many, ''.join('<div class="post"><b>' + str(i) + '</b></div>' for i in range(1000))),
            ('no document', top_only, ''),
        ]
        for name, parser, text in page_cases:
            panel = G.EditPageParserPanel(root, parser, test_data=P.ParsingTestData({}, [text]))
            panel._test_panel._SetExampleData(text)
            panel._test_panel.TestParse()
            out['page'].append({'name': name, 'tuple': parser.GetSerialisableTuple(), 'document': text,
                                'results': results_text(panel)})
            panel.deleteLater()

        subsidiary_cases = [
            ('posts of a subsidiary', sub(page('post', [tag_b, url_i])), posts),
            ('a subsidiary in a subsidiary', sub(page('thread', [tag_b], [sub(page('reply', [url_i]), 'reply')]), 'thread'), '<div class="thread"><b>t1</b><div class="reply"><i>https://example.com/r1</i></div></div>'),
            ('a veto in one post', sub(page('post', [tag_b, veto_q])), '<div class="post"><b>kept</b></div><div class="post"><b>dropped</b><q>nope</q></div>'),
            ('no posts', sub(page('post', [tag_b])), '<p>nothing</p>'),
            ('an error', P.SubsidiaryPageParser(formula=P.ParseFormulaHTML(tag_rules=[P.ParseRuleHTML(tag_name='html')], content_to_fetch=P.HTML_CONTENT_HTML), page_parser=page('post', [json_content('json')])), '<html><b>x</b></html>'),
            ('no document', sub(page('post', [tag_b])), ''),
        ]
        for name, parser, text in subsidiary_cases:
            panel = G.EditSubsidiaryPageParserPanel(root, parser, test_data=P.ParsingTestData({}, [text]))
            panel._test_panel._SetExampleData(text)
            panel._test_panel.TestParse()
            parent = page('parent', subs=[parser])
            out['subsidiary'].append({'name': name, 'parent_tuple': parent.GetSerialisableTuple(), 'document': text,
                                      'results': results_text(panel)})
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
