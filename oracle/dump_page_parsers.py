#!/usr/bin/env python3
"""Record the reference's page parsers on random documents.

Random page parsers (content parsers of every content type, including
vetoes and variables; sometimes a string conversion first; sometimes
subsidiary page parsers that split the page into posts) built with the
reference's classes, run on random HTML and JSON documents. Each case keeps
the parser as the reference serialises it and what came out: the parsed
posts with their contents, what importers read from each post (cleaned
tags, URLs by type, notes, hashes, times, variable, headers), the page title
and the parsing context afterwards; or the veto or error.

Usage: python oracle/dump_page_parsers.py > oracle/fixtures/page_parsers.json
"""

import json
import os
import random
import re
import sys

sys.path.insert( 0, os.path.dirname( __file__ ) )

import dump_formulas as F

from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusExceptions
from hydrus.core import HydrusSerialisable
from hydrus.client import ClientStrings
from hydrus.client.parsing import ClientParsing as P
from hydrus.client.parsing import ClientParsingResults as R

rng = random.Random( 689 )
F.rng = rng

URL_TYPES = [ HC.URL_TYPE_DESIRED, HC.URL_TYPE_DESIRED, HC.URL_TYPE_SOURCE, HC.URL_TYPE_NEXT, HC.URL_TYPE_SUB_GALLERY ]
TIMESTAMP_TYPES = [ HC.TIMESTAMP_TYPE_MODIFIED_DOMAIN, HC.TIMESTAMP_TYPE_MODIFIED_FILE, HC.TIMESTAMP_TYPE_IMPORTED ]


def formula( kind ):

    return F.html_formula() if kind == 'html' else F.json_formula()


def content_parser( kind, index ):

    content_type = rng.choice( [ HC.CONTENT_TYPE_URLS, HC.CONTENT_TYPE_URLS, HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_TYPE_NOTES, HC.CONTENT_TYPE_HASH, HC.CONTENT_TYPE_TIMESTAMP, HC.CONTENT_TYPE_TITLE, HC.CONTENT_TYPE_VETO, HC.CONTENT_TYPE_VARIABLE, HC.CONTENT_TYPE_HTTP_HEADERS ] )

    if content_type == HC.CONTENT_TYPE_URLS:

        info = ( rng.choice( URL_TYPES ), rng.choice( [ 0, 50, 100 ] ) )

    elif content_type == HC.CONTENT_TYPE_MAPPINGS:

        info = rng.choice( [ None, '', 'creator', 'series', 'System' ] )

    elif content_type == HC.CONTENT_TYPE_NOTES:

        info = rng.choice( [ 'comment', 'source note' ] )

    elif content_type == HC.CONTENT_TYPE_HASH:

        info = ( rng.choice( [ 'md5', 'sha1', 'sha256' ] ), rng.choice( [ 'hex', 'base64' ] ) )

    elif content_type == HC.CONTENT_TYPE_TIMESTAMP:

        info = rng.choice( TIMESTAMP_TYPES )

    elif content_type == HC.CONTENT_TYPE_TITLE:

        info = rng.choice( [ 0, 10, 50 ] )

    elif content_type == HC.CONTENT_TYPE_VETO:

        info = ( rng.random() < 0.5, F.string_match() )

    elif content_type == HC.CONTENT_TYPE_VARIABLE:

        info = 'next_page'

    else:

        info = rng.choice( [ 'Referer', 'X-Token' ] )


    if content_type in ( HC.CONTENT_TYPE_HASH, HC.CONTENT_TYPE_TIMESTAMP ) and rng.random() < 0.6:

        # something that decodes: hex digits, base64 text, a number
        static = rng.choice( [ 'd41d8cd98f00b204e9800998ecf8427e', '1B2M2Y8AsgTpgAmY7PhCfg==', 'zz', '1600000000', '12x', ' 1_500_000_000 ' ] )

        the_formula = P.ParseFormulaStatic( static_text = static, num_to_do = rng.randint( 1, 2 ) )

    elif content_type == HC.CONTENT_TYPE_URLS and rng.random() < 0.3:

        the_formula = P.ParseFormulaStatic( static_text = rng.choice( [ '/relative/file.jpg', 'page.html?x=1 y', 'https://example.com/a b', 'see: https://example.com/post/9', 'http://https://example.com/double', '//cdn.example.com/f.png', 'javascript:void(0)' ] ) )

    else:

        the_formula = formula( kind )


    return P.ContentParser( name = rng.choice( [ 'tags', 'file url', 'post', 'Zeta', 'alpha' ] ) + str( index ), content_type = content_type, formula = the_formula, additional_info = info )


def page_parser( kind, depth = 0 ):

    content_parsers = [ content_parser( kind, i ) for i in range( rng.randint( 1, 5 ) ) ]

    subsidiary = []

    if depth == 0 and rng.random() < 0.35:

        if kind == 'html':

            names = [ t for t in F.PRESENT_TAGS if t not in ( 'html', 'head', 'body', 'title' ) ] or [ 'div' ]

            splitter = P.ParseFormulaHTML( tag_rules = [ P.ParseRuleHTML( rule_type = P.HTML_RULE_TYPE_DESCENDING, tag_name = rng.choice( names ), tag_attributes = {} ) ], content_to_fetch = P.HTML_CONTENT_HTML )

        else:

            splitter = P.ParseFormulaJSON( parse_rules = [ ( P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None ) ], content_to_fetch = P.JSON_CONTENT_JSON )


        for i in range( rng.randint( 1, 2 ) ):

            sub_parser = page_parser( kind, depth + 1 )
            sub_parser.SetName( rng.choice( [ 'posts', 'Thumbs', 'entries' ] ) + str( i ) )

            subsidiary.append( P.SubsidiaryPageParser( formula = splitter, sort_posts_by_source_time = rng.random() < 0.5, page_parser = sub_parser ) )



    converter = ClientStrings.StringConverter()

    if rng.random() < 0.1:

        converter = ClientStrings.StringConverter( conversions = [ ( ClientStrings.STRING_CONVERSION_REGEX_SUB, ( 'thumb', 'THUMB' ) ) ] )


    return P.PageParser( 'page parser', string_converter = converter, subsidiary_page_parsers = subsidiary, content_parsers = content_parsers )


def describe( parsed_post ):

    contents = []

    for parsed_content in parsed_post.parsed_contents:

        d = parsed_content.parsed_content_description

        if isinstance( d, R.ParsableContentDescriptionURL ):

            info = [ d.url_type, d.priority ]

        elif isinstance( d, R.ParsableContentDescriptionTag ):

            info = d.namespace

        elif isinstance( d, R.ParsableContentDescriptionNote ):

            info = d.note_name

        elif isinstance( d, R.ParsableContentDescriptionHash ):

            info = [ d.hash_type, d.hash_encoding ]

        elif isinstance( d, R.ParsableContentDescriptionTimestamp ):

            info = d.timestamp_type

        elif isinstance( d, R.ParsableContentDescriptionTitle ):

            info = d.priority

        elif isinstance( d, R.ParsableContentDescriptionVariable ):

            info = d.temp_variable_name

        elif isinstance( d, R.ParsableContentDescriptionHTTPHeaders ):

            info = d.header_name

        else:

            info = None


        contents.append( [ d.name, d.content_type, info, parsed_content.parsed_text ] )


    return {
        'contents': contents,
        'tags': sorted( parsed_post.GetTags() ),
        'urls': { str( t ): parsed_post.GetURLs( ( t, ) ) for t in sorted( set( URL_TYPES ) ) },
        'top_file_urls': parsed_post.GetURLs( ( HC.URL_TYPE_DESIRED, ), only_get_top_priority = True ),
        'notes': parsed_post.GetNamesAndNotes(),
        'hashes': [ [ t, h.hex() ] for ( t, h ) in parsed_post.GetHashes() ],
        # (a domain modified time is capped at "5 seconds ago"; none of these are in the future)
        'timestamps': { str( t ): parsed_post.GetTimestamp( t ) for t in TIMESTAMP_TYPES },
        'variable': parsed_post.GetVariable(),
        'headers': parsed_post.GetHTTPHeaders(),
        'pursuable': parsed_post.HasPursuableURLs(),
    }


def main():

    documents = []
    cases = []

    for kind in ( 'html', 'json' ):

        for _ in range( 70 ):

            document = F.html_document() if kind == 'html' else F.json_document()

            F.PRESENT_TAGS[:] = sorted( set( t.lower() for t in re.findall( r'<([a-zA-Z]+)', document ) ) )
            F.PRESENT_KEYS[:] = sorted( set( re.findall( r'"([^"\\]*)":', document ) ) )
            F.PRESENT_ATTRS[:] = sorted( set( a.lower() for a in re.findall( r' ([a-zA-Z-]+)(?:=|[ >])', document ) ) )

            doc_index = len( documents )
            documents.append( document )

            for _ in range( 8 ):

                # parse with the parser as a client would have it: loaded from
                # its serialised form (which sorts content parsers by name)
                parser = HydrusSerialisable.CreateFromSerialisableTuple( page_parser( kind ).GetSerialisableTuple() )

                context = { 'url': rng.choice( [ 'https://example.com/post/view/123', 'https://example.com/a/b/page.html?q=1' ] ) }

                case = { 'parser': parser.GetSerialisableTuple(), 'document': doc_index, 'context': dict( context ) }

                try:

                    posts = parser.Parse( context, document )

                    case[ 'posts' ] = [ describe( p ) for p in posts ]
                    case[ 'title' ] = R.GetTitleFromParsedPosts( posts )
                    case[ 'context_after' ] = context

                except HydrusExceptions.VetoException as e:

                    case[ 'veto' ] = str( e )

                except HydrusExceptions.ParseException as e:

                    case[ 'error' ] = str( e )[ :200 ]

                except Exception as e:

                    case[ 'crash' ] = type( e ).__name__


                cases.append( case )




    json.dump( { 'documents': documents, 'cases': cases }, sys.stdout, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
