#!/usr/bin/env python3
"""Record the reference's parsing formulas on random documents.

Random HTML and JSON documents (including malformed HTML, awkward
attributes, entities, comments, big and odd numbers) and random formulas
built with the reference's own classes (HTML tag rules of every kind, JSON
rules of every kind, zipper, nested, static and context variable formulas,
some string processing). Each case records the formula as the reference
serialises it, the document, and what `Parse` returns, or that it raised a
ParseException.

Usage: python oracle/dump_formulas.py > oracle/fixtures/formulas.json
"""

import json
import os
import random
import re
import sys
import types

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.core import HydrusExceptions
from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientStrings
from hydrus.client.caches import ClientCaches
from hydrus.client.parsing import ClientParsing as P

CG.client_controller = types.SimpleNamespace( parsing_cache = ClientCaches.ParsingCache() )

rng = random.Random( 688 )

# html -------------------------------------------------------------------------

# mostly ordinary structure, with some of what makes HTML5 tree building
# interesting (table parts out of place, templates, scripts, upper case)
TAGS = [ 'div', 'div', 'div', 'span', 'span', 'a', 'a', 'a', 'p', 'b', 'i', 'li', 'ul', 'img', 'br', 'section', 'section', 'td', 'table', 'script', 'template', 'DIV', 'A' ]
WORDS = [ 'blue eyes', 'tag', 'Samus', 'x', '&amp;', '&lt;b&gt;', '&nbsp;', 'caf&eacute;', '日本', '  spaced  ', 'line\nbreak', 'a&b', 'it\'s', '"quoted"', 'https://example.com/post/5', '/relative/path', '' ]
CLASSES = [ 'thumb', 'tag-type-general', 'post', 'thumb post', ' thumb  extra ', '', 'a\tb' ]


def attributes():

    attrs = []

    for _ in range( rng.choice( ( 0, 0, 1, 1, 2, 3 ) ) ):

        name = rng.choice( [ 'class', 'class', 'href', 'id', 'src', 'data-id', 'title', 'rel', 'HREF', 'alt' ] )

        if name == 'class':

            value = rng.choice( CLASSES )

        elif name.lower() == 'href':

            value = rng.choice( [ 'https://example.com/post/1', '/post/2?a=1&amp;b=2', 'page.html', '', 'javascript:void(0)' ] )

        elif name == 'rel':

            value = rng.choice( [ 'next', 'nofollow noopener', '' ] )

        else:

            value = rng.choice( [ '1', 'x"y', "it's", 'a b', 'thumb', '' ] )


        quote = '\'' if '"' in value else '"'

        attrs.append( f' {name}={quote}{value}{quote}' if rng.random() < 0.9 else f' {name}' )


    return ''.join( attrs )


def element( depth ):

    tag = rng.choice( TAGS )

    if tag.lower() in ( 'br', 'img' ):

        return f'<{tag}{attributes()}>'


    parts = []

    for _ in range( rng.randint( 0, 3 if depth < 4 else 0 ) ):

        roll = rng.random()

        if roll < 0.5:

            parts.append( element( depth + 1 ) )

        elif roll < 0.92:

            parts.append( rng.choice( WORDS ) )

        else:

            parts.append( '<!-- a comment -->' )



    close = f'</{tag}>' if rng.random() < 0.93 else ''

    return f'<{tag}{attributes()}>' + ''.join( parts ) + close


def html_document():

    body = ''.join( element( 0 ) for _ in range( rng.randint( 1, 4 ) ) )

    if rng.random() < 0.1:

        body += '</p></b><i>unbalanced'


    if rng.random() < 0.5:

        return '<!DOCTYPE html><html><head><title>A title &amp; more</title></head><body>' + body + '</body></html>'


    return body


def string_match():

    roll = rng.random()

    if roll < 0.4:

        return ClientStrings.StringMatch()

    elif roll < 0.7:

        return ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = rng.choice( [ 'thumb', 'x', 'tag', 'blue eyes' ] ) )

    else:

        return ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_REGEX, match_value = rng.choice( [ r'\d+', r'^[a-z ]+$', 'e', r'post/\d' ] ) )



def html_rule():

    roll = rng.random()

    should_test = rng.random() < 0.15

    if roll < 0.6:

        rule_type = P.HTML_RULE_TYPE_DESCENDING

    elif roll < 0.75:

        rule_type = P.HTML_RULE_TYPE_ASCENDING

    elif roll < 0.88:

        rule_type = P.HTML_RULE_TYPE_NEXT_SIBLINGS

    else:

        rule_type = P.HTML_RULE_TYPE_PREV_SIBLINGS


    tag_name = rng.choice( PRESENT_TAGS + [ None, None, 'div', 'a' ] )

    if rule_type == P.HTML_RULE_TYPE_ASCENDING:

        return P.ParseRuleHTML( rule_type = rule_type, tag_name = tag_name, tag_depth = rng.randint( 1, 3 ), should_test_tag_string = should_test, tag_string_string_match = string_match() )


    attrs = rng.choice( [ {}, {}, {}, {}, {}, { 'class': 'thumb' }, { 'class': 'post' }, { 'class': 'thumb post' }, { 'id': '1' }, { 'href': 'page.html' }, { 'rel': 'next' } ] )

    index = rng.choice( [ None, None, None, None, 0, 1, 2, -1 ] )

    return P.ParseRuleHTML( rule_type = rule_type, tag_name = tag_name, tag_attributes = attrs, tag_index = index, should_test_tag_string = should_test, tag_string_string_match = string_match() )


def string_processor():

    processor = ClientStrings.StringProcessor()

    roll = rng.random()

    if roll < 0.75:

        return processor

    steps = []

    if roll < 0.85:

        steps.append( ClientStrings.StringConverter( conversions = [ ( ClientStrings.STRING_CONVERSION_PREPEND_TEXT, 'pre:' ) ] ) )

    elif roll < 0.95:

        steps.append( string_match() )

    else:

        steps.append( ClientStrings.StringSplitter( separator = ' ' ) )


    processor.SetProcessingSteps( steps )

    return processor


def html_formula():

    content = rng.choice( [ P.HTML_CONTENT_ATTRIBUTE, P.HTML_CONTENT_ATTRIBUTE, P.HTML_CONTENT_STRING, P.HTML_CONTENT_STRING, P.HTML_CONTENT_HTML ] )

    attribute = rng.choice( PRESENT_ATTRS + [ 'href', 'href', 'class', 'missing' ] )

    if rng.random() < 0.2:

        # every element's value of an attribute that is there
        return P.ParseFormulaHTML( tag_rules = [ P.ParseRuleHTML( rule_type = P.HTML_RULE_TYPE_DESCENDING, tag_name = None, tag_attributes = {} ) ], content_to_fetch = P.HTML_CONTENT_ATTRIBUTE, attribute_to_fetch = rng.choice( PRESENT_ATTRS or [ 'href' ] ), string_processor = string_processor() )

    if rng.random() < 0.5:

        # a permissive rule on a tag that is there, so most of these find something
        names = [ t for t in PRESENT_TAGS if t not in ( 'html', 'head', 'body', 'title' ) ] or [ None ]
        rules = [ P.ParseRuleHTML( rule_type = P.HTML_RULE_TYPE_DESCENDING, tag_name = rng.choice( names ), tag_attributes = {}, tag_index = rng.choice( [ None, None, 0, 1 ] ) ) ]

        if rng.random() < 0.3:

            rules.append( html_rule() )


    else:

        rules = [ html_rule() for _ in range( rng.choice( ( 1, 1, 1, 2, 2, 3 ) ) ) ]

    return P.ParseFormulaHTML( tag_rules = rules, content_to_fetch = content, attribute_to_fetch = attribute, string_processor = string_processor() )


# json -------------------------------------------------------------------------

KEYS = [ 'posts', 'tags', 'id', 'file_url', 'a', 'b', '10', '2', 'z', 'Ä', 'name', '' ]


def json_value( depth ):

    roll = rng.random()

    if depth < 4 and roll < 0.3:

        return { rng.choice( KEYS ): json_value( depth + 1 ) for _ in range( rng.randint( 0, 4 ) ) }

    elif depth < 4 and roll < 0.55:

        return [ json_value( depth + 1 ) for _ in range( rng.randint( 0, 4 ) ) ]


    return rng.choice( [ 1, 0, -7, 2 ** 70, 1.5, 1.0, 1e20, 1e-7, 0.1, True, False, None, 'text', 'blue eyes', '日本', 'line\nbreak', '', 'https://example.com/f.jpg', '12' ] )


def json_document():

    text = json.dumps( json_value( 0 ), ensure_ascii = rng.random() < 0.5 )

    roll = rng.random()

    if roll < 0.03:

        return text[ : len( text ) // 2 ]

    elif roll < 0.06:

        return '{"a": NaN, "b": Infinity, "a": [1, 2]}'

    elif roll < 0.08:

        return '<html>not json</html>'


    return text


def json_rule():

    roll = rng.random()

    if roll < 0.35:

        return ( P.JSON_PARSE_RULE_TYPE_DICT_KEY, rng.choice( [ ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = rng.choice( PRESENT_KEYS or KEYS ) ), ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = rng.choice( PRESENT_KEYS or KEYS ) ), string_match() ] ) )

    elif roll < 0.6:

        return ( P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None )

    elif roll < 0.75:

        return ( P.JSON_PARSE_RULE_TYPE_INDEXED_ITEM, rng.choice( [ 0, 1, 2, -1 ] ) )

    elif roll < 0.85:

        return ( P.JSON_PARSE_RULE_TYPE_TEST_STRING_ITEMS, string_match() )

    elif roll < 0.95:

        return ( P.JSON_PARSE_RULE_TYPE_ASCEND, rng.randint( 1, 2 ) )

    else:

        return ( P.JSON_PARSE_RULE_TYPE_DEMINIFY_JSON, rng.choice( [ 0, 1 ] ) )



def json_formula():

    rules = [ json_rule() for _ in range( rng.choice( ( 0, 1, 1, 2, 2, 3 ) ) ) ]

    content = rng.choice( [ P.JSON_CONTENT_STRING, P.JSON_CONTENT_STRING, P.JSON_CONTENT_JSON, P.JSON_CONTENT_DICT_KEYS ] )

    return P.ParseFormulaJSON( parse_rules = rules, content_to_fetch = content, string_processor = string_processor() )


def other_formula( kind ):

    if kind == 'html':

        inner = html_formula

    else:

        inner = json_formula


    roll = rng.random()

    if roll < 0.35:

        return P.ParseFormulaZipper( formulae = [ inner() for _ in range( rng.randint( 1, 3 ) ) ], sub_phrase = rng.choice( [ '\\1', '\\1-\\2', 'x\\2\\1\\3', 'static' ] ), string_processor = string_processor() )

    elif roll < 0.55:

        if kind == 'html':

            main = P.ParseFormulaHTML( tag_rules = [ html_rule() ], content_to_fetch = P.HTML_CONTENT_HTML )

        else:

            main = P.ParseFormulaJSON( parse_rules = [ json_rule() ], content_to_fetch = P.JSON_CONTENT_JSON )


        return P.ParseFormulaNested( main_formula = main, sub_formula = inner(), string_processor = string_processor() )

    elif roll < 0.75:

        return P.ParseFormulaStatic( static_text = rng.choice( [ 'example text', '  padded  ', 'multi\nline', '' ] ), num_to_do = rng.randint( 0, 3 ), string_processor = string_processor() )

    else:

        return P.ParseFormulaContextVariable( variable_name = rng.choice( [ 'url', 'post_index', 'missing' ] ), string_processor = string_processor() )



# recording --------------------------------------------------------------------

def run( formula, document, context, collapse ):

    try:

        return { 'result': formula.Parse( dict( context ), document, collapse ) }

    except HydrusExceptions.ParseException as e:

        return { 'error': str( e )[ :200 ] }

    except Exception as e:

        # a reference bug (e.g. a deminify rule following a cycle forever)
        return { 'crash': type( e ).__name__ }



documents = []
cases = []
PRESENT_TAGS = []
PRESENT_KEYS = []
PRESENT_ATTRS = []

for kind in ( 'html', 'json' ):

    for _ in range( 120 ):

        document = html_document() if kind == 'html' else json_document()

        PRESENT_TAGS[:] = sorted( set( t.lower() for t in re.findall( r'<([a-zA-Z]+)', document ) ) )
        PRESENT_KEYS[:] = sorted( set( re.findall( r'"([^"\\]*)":', document ) ) )
        PRESENT_ATTRS[:] = sorted( set( a.lower() for a in re.findall( r' ([a-zA-Z-]+)(?:=|[ >])', document ) ) )

        doc_index = len( documents )
        documents.append( document )

        for _ in range( 16 ):

            if rng.random() < 0.85:

                formula = html_formula() if kind == 'html' else json_formula()

            else:

                formula = other_formula( kind )


            context = { 'url': 'https://example.com/post/view/123', 'post_index': '0' }
            collapse = rng.random() < 0.7

            cases.append( {
                'formula': formula.GetSerialisableTuple(),
                'document': doc_index,
                'context': context,
                'collapse_newlines': collapse,
                **run( formula, document, context, collapse ),
            } )




json.dump( { 'documents': documents, 'cases': cases }, sys.stdout, ensure_ascii = False )
sys.stdout.write( '\n' )
