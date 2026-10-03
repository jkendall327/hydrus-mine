#!/usr/bin/env python3
"""Record how the reference describes string processing.

String processors (in parsers, URL classes and sidecar routers) show what
they do: a processor's button lists its steps (`GetProcessingStrings`:
each conversion of a converter, and each other step's `ToString`), and
each step says what it is (`ToString`, `simple` for its kind alone,
`with_type` with its kind in capitals first: "SORT: sorting human sort
(ascending)"). A processor summarises itself ("some conversion, sorting")
and says whether it makes changes.

This builds random processors from every kind of step, converters with
every kind of conversion, matches of every kind, slicers, sorters,
splitters, joiners and tag filters, and records each as the reference
serialises it, with its summary, whether it makes changes, its processing
strings, and each step's four descriptions.

Usage: python oracle/dump_string_descriptions.py > oracle/fixtures/string_descriptions.json
"""

import json
import os
import random
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.client import ClientStrings as S
from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusTags

rng = random.Random( 688 )

TEXTS = [ 'x', 'pre:', '', 'a "quoted" thing', '日本', '\\n', ' ' ]


def conversion():

    kind = rng.choice( [
        S.STRING_CONVERSION_REMOVE_TEXT_FROM_BEGINNING,
        S.STRING_CONVERSION_REMOVE_TEXT_FROM_END,
        S.STRING_CONVERSION_CLIP_TEXT_FROM_BEGINNING,
        S.STRING_CONVERSION_CLIP_TEXT_FROM_END,
        S.STRING_CONVERSION_PREPEND_TEXT,
        S.STRING_CONVERSION_APPEND_TEXT,
        S.STRING_CONVERSION_APPEND_RANDOM,
        S.STRING_CONVERSION_ENCODE,
        S.STRING_CONVERSION_DECODE,
        S.STRING_CONVERSION_REVERSE,
        S.STRING_CONVERSION_REGEX_SUB,
        S.STRING_CONVERSION_DATE_DECODE,
        S.STRING_CONVERSION_DATEPARSER_DECODE,
        S.STRING_CONVERSION_DATE_ENCODE,
        S.STRING_CONVERSION_INTEGER_ADDITION,
        S.STRING_CONVERSION_HASH_FUNCTION,
    ] )

    if kind in ( S.STRING_CONVERSION_REMOVE_TEXT_FROM_BEGINNING, S.STRING_CONVERSION_REMOVE_TEXT_FROM_END, S.STRING_CONVERSION_CLIP_TEXT_FROM_BEGINNING, S.STRING_CONVERSION_CLIP_TEXT_FROM_END ):

        data = rng.choice( [ 0, 1, 3, 1000, 12345 ] )

    elif kind in ( S.STRING_CONVERSION_PREPEND_TEXT, S.STRING_CONVERSION_APPEND_TEXT ):

        data = rng.choice( TEXTS )

    elif kind == S.STRING_CONVERSION_APPEND_RANDOM:

        data = ( rng.choice( [ 'abc', '0123456789', 'x' ] ), rng.choice( [ 1, 5, 1500 ] ) )

    elif kind in ( S.STRING_CONVERSION_ENCODE, S.STRING_CONVERSION_DECODE ):

        data = rng.choice( [ 0, 1, 2, 3, 4, 5 ] )

    elif kind == S.STRING_CONVERSION_REGEX_SUB:

        data = ( rng.choice( [ r'\d+', 'a', r"it's" ] ), rng.choice( [ '', 'b', r'\1 "x"' ] ) )

    elif kind == S.STRING_CONVERSION_DATE_DECODE:

        data = ( rng.choice( [ '%Y-%m-%d', '%d/%m/%Y %H:%M' ] ), rng.choice( [ HC.TIMEZONE_UTC, HC.TIMEZONE_LOCAL, HC.TIMEZONE_OFFSET ] ), rng.choice( [ 0, 3600, -18000 ] ) )

    elif kind == S.STRING_CONVERSION_DATE_ENCODE:

        data = ( rng.choice( [ '%Y-%m-%d', 'it\'s %H' ] ), rng.choice( [ HC.TIMEZONE_UTC, HC.TIMEZONE_LOCAL ] ) )

    elif kind == S.STRING_CONVERSION_INTEGER_ADDITION:

        data = rng.choice( [ 0, 5, -3, 1000 ] )

    elif kind == S.STRING_CONVERSION_HASH_FUNCTION:

        data = rng.choice( [ 'md5', 'sha1', 'sha256', 'sha512' ] )

    else:

        data = None


    return ( kind, data )


def converter():

    return S.StringConverter( conversions = [ conversion() for _ in range( rng.choice( [ 0, 1, 1, 2, 3 ] ) ) ], example_string = rng.choice( [ 'example string', 'something' ] ) )


def match():

    kind = rng.choice( [ S.STRING_MATCH_FIXED, S.STRING_MATCH_FLEXIBLE, S.STRING_MATCH_REGEX, S.STRING_MATCH_ANY ] )

    if kind == S.STRING_MATCH_FIXED:

        value = rng.choice( [ 'tags', 'x', '' ] )

    elif kind == S.STRING_MATCH_FLEXIBLE:

        value = rng.choice( [ 0, 1, 2, 3, 4, 5, 6 ] )

    elif kind == S.STRING_MATCH_REGEX:

        value = rng.choice( [ r'\d+', '[a-z]', 'page "\\d"' ] )

    else:

        value = ''


    min_chars = rng.choice( [ None, None, 1, 5 ] )
    max_chars = rng.choice( [ None, None, 10, 5 ] )

    example = rng.choice( [ S.DEFAULT_EXAMPLE_STRING, '123', 'tags' ] )

    return S.StringMatch( match_type = kind, match_value = value, min_chars = min_chars, max_chars = max_chars, example_string = example )


def slicer():

    ( start, end ) = rng.choice( [ ( None, None ), ( 0, 1 ), ( 2, 3 ), ( -1, None ), ( 1, None ), ( None, 3 ), ( 1, 4 ), ( 3, 3 ), ( 5, 2 ), ( -3, -1 ), ( None, -2 ), ( 0, None ) ] )

    return S.StringSlicer( index_start = start, index_end = end )


def sorter():

    return S.StringSorter( sort_type = rng.choice( [ 0, 1, 2, 3 ] ), asc = rng.random() < 0.5, regex = rng.choice( [ None, None, r'\d+' ] ) )


def splitter():

    return S.StringSplitter( separator = rng.choice( [ ',', '\\n', ' ', '||' ] ), max_splits = rng.choice( [ None, None, 1, 3, 1500 ] ) )


def joiner():

    return S.StringJoiner( joiner = rng.choice( [ ',', '\\n', '' ] ), join_tuple_size = rng.choice( [ None, 2, 3 ] ) )


def tag_filter():

    f = HydrusTags.TagFilter()

    for _ in range( rng.randint( 0, 3 ) ):

        f.SetRule( rng.choice( [ '', ':', 'series:', 'blue eyes' ] ), rng.choice( [ HC.FILTER_WHITELIST, HC.FILTER_BLACKLIST ] ) )


    return S.StringTagFilter( tag_filter = f, example_string = rng.choice( [ 'blue eyes', 'series:x' ] ) )


def step():

    return rng.choice( [ converter, converter, match, slicer, sorter, splitter, joiner, tag_filter ] )()


def describe( s ):

    return [ s.ToString( simple = simple, with_type = with_type ) for simple in ( False, True ) for with_type in ( False, True ) ]


def main():

    cases = []

    for _ in range( 400 ):

        steps = [ step() for _ in range( rng.choice( [ 0, 1, 1, 2, 3 ] ) ) ]

        p = S.StringProcessor()
        p.SetProcessingSteps( steps )

        cases.append( {
            'processor' : p.GetSerialisableTuple(),
            'summary' : p.ToString(),
            'makes_changes' : p.MakesChanges(),
            'processing_strings' : p.GetProcessingStrings(),
            'steps' : [ describe( s ) for s in steps ],
        } )


    json.dump( { 'cases' : cases }, sys.stdout, indent = 1, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
