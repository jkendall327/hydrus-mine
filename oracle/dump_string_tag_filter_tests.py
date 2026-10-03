#!/usr/bin/env python3
"""Record how the reference's string tag filter step tests a string
(`StringTagFilter.Test`, which its editor shows under the example: "Example
matches ok!", or "Example does not match - " and why: '"x" was not a valid
tag!' or '"x" did not pass the tag filter!').

For a few tag filters (as their rules) and texts (tags in any case and
spacing, namespaced or not, invalid ones), this records whether each
passes and why not.

Usage: python oracle/dump_string_tag_filter_tests.py > oracle/fixtures/string_tag_filter_tests.json
"""

import json
import os
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.client import ClientStrings
from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusExceptions
from hydrus.core import HydrusTags

FILTERS = [
    [],
    [ [ '', 'black' ] ],
    [ [ ':', 'black' ], [ 'series:', 'white' ] ],
    [ [ 'samus', 'black' ] ],
    [ [ 'creator:', 'black' ], [ 'creator:someone', 'white' ] ],
]

TEXTS = [ 'samus', '  Samus  ', 'series:metroid', 'creator:someone', 'creator:else', 'character:samus', '', '   ', 'series:', ':colon', 'blue eyes' ]


def main():

    cases = []

    for rules in FILTERS:

        tag_filter = HydrusTags.TagFilter()

        for ( s, r ) in rules:

            tag_filter.SetRule( s, HC.FILTER_BLACKLIST if r == 'black' else HC.FILTER_WHITELIST )


        step = ClientStrings.StringTagFilter( tag_filter = tag_filter )

        results = []

        for text in TEXTS:

            try:

                step.Test( text )

                results.append( [ text, None ] )

            except HydrusExceptions.StringMatchException as e:

                results.append( [ text, str( e ) ] )



        cases.append( { 'rules' : rules, 'results' : results } )


    json.dump( { 'cases' : cases }, sys.stdout, indent = 1, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
