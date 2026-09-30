#!/usr/bin/env python3
"""Record the reference's string processors on random lists of strings.

Sidecar routers run a string processor over the strings they gather (by
default a human sort), and so can each sidecar importer. This records
processors built from the steps that aren't covered by the parsing
formula cases: sorters of every kind, with and without a regex, and tag
filters, mixed with a few conversions and filters. Each case keeps the
processor as the reference serialises it, the strings in, and what
`ProcessStrings` gives back.

Usage: python oracle/dump_string_processing.py > oracle/fixtures/string_processing.json
"""

import json
import os
import random
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusTags
from hydrus.client import ClientStrings

rng = random.Random( 688 )

WORDS = [
    'blue eyes', 'Blue Eyes', 'page 2', 'page 10', 'page 1', 'page 01', 'file10.jpg', 'file9.jpg',
    'character:samus aran', 'series:metroid', 'creator:someone', 'Character:Samus Aran',
    '  padded  ', 'a', 'B', 'b', 'Z', 'zebra', 'éclair', 'Éclair', 'straße', 'strasse', '日本',
    ':)', '::)', 'system:inbox', '-negated', 'title:page 3 of 12', 'rating:safe', 'rating:explicit',
    '123', '45', '0', '007', 'v2.10', 'v2.9', 'https://example.com/post/5', 'meta:tagme', '',
    'x' * 5, 'series:', ':', 'a:b:c', 'Ⅻ', 'ß', 'ǅ', '1e5', '-3', '+4',
]

REGEXES = [ r'\d+', r'[a-z]+', r'page \d+', r':.*', r'^x', r'(?P<n>\d)\d', r'Z', r'' ]

SLICES = [ '', ':', 'series:', 'character:', 'rating:', 'blue eyes', 'samus aran', 'meta:tagme', 'page 2' ]


def sorter():

    sort_type = rng.choice( [ 0, 1, 2, 2, 3 ] )
    asc = rng.random() < 0.5
    regex = rng.choice( REGEXES ) if rng.random() < 0.4 else None

    return ClientStrings.StringSorter( sort_type = sort_type, asc = asc, regex = regex )


def tag_filter():

    f = HydrusTags.TagFilter()

    for _ in range( rng.randint( 0, 4 ) ):

        f.SetRule( rng.choice( SLICES ), rng.choice( [ HC.FILTER_WHITELIST, HC.FILTER_BLACKLIST ] ) )


    return ClientStrings.StringTagFilter( tag_filter = f )


def other_step():

    roll = rng.random()

    if roll < 0.4:

        return ClientStrings.StringConverter( conversions = [ ( ClientStrings.STRING_CONVERSION_PREPEND_TEXT, rng.choice( [ 'pre:', 'series:', 'x' ] ) ) ] )

    elif roll < 0.7:

        return ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_REGEX, match_value = rng.choice( REGEXES[ :-1 ] ) )

    else:

        return ClientStrings.StringSlicer( index_start = rng.choice( [ None, 0, 1, -2 ] ), index_end = rng.choice( [ None, 3, -1 ] ) )



def processor():

    steps = []

    for _ in range( rng.randint( 1, 3 ) ):

        roll = rng.random()

        if roll < 0.5:

            steps.append( sorter() )

        elif roll < 0.8:

            steps.append( tag_filter() )

        else:

            steps.append( other_step() )



    p = ClientStrings.StringProcessor()

    p.SetProcessingSteps( steps )

    return p


def main():

    cases = []

    for _ in range( 600 ):

        p = processor()

        strings = [ rng.choice( WORDS ) for _ in range( rng.randint( 0, 12 ) ) ]

        cases.append( {
            'processor' : p.GetSerialisableTuple(),
            'strings' : strings,
            'result' : p.ProcessStrings( strings ),
        } )


    json.dump( { 'cases' : cases }, sys.stdout, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
