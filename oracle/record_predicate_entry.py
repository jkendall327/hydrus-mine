#!/usr/bin/env python3
"""Record what entering predicates does to a search's predicates in the
reference (`ListBoxTagsActiveSearchPredicates._EnterPredicates`).

A predicate already there is removed when entered again; entering one
removes those it is mutually exclusive with (`IsMutuallyExclusive`:
system:everything with anything, a predicate and its inverse, two
system:limits); the list then sorts itself. In the running client, on the
`basic` fixture, for each scenario this sets a search page's predicates to
`start`, enters `enter` (each in turn), and records `shown`, the list as
it then shows it.

It also records `inverses`: each of `INVERSES` as the reference writes it,
and its inverse (`GetInverseCopy`), or none.

Predicates are written as typed: "OR:a|b" an OR of a and b, "system:local"
and "system:not local" made as such (the parser has no words for them),
"system:..." otherwise parsed as the reference's
system predicate parser parses it, "-tag" an excluded tag, else a tag.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_predicate_entry.py
       (writes fixtures/predicate_entry.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'predicate_entry.json' )

SCENARIOS = [
    # system:everything goes once anything else comes
    ( [ 'system:everything' ], [ 'system:inbox' ] ),
    ( [ 'system:everything' ], [ 'blue eyes' ] ),
    ( [ 'system:everything', 'blue eyes' ], [ 'system:archive' ] ),
    # but comes beside others
    ( [ 'blue eyes', 'system:inbox' ], [ 'system:everything' ] ),
    ( [], [ 'system:everything' ] ),
    # entered again, it goes
    ( [ 'system:inbox' ], [ 'system:inbox' ] ),
    ( [ 'blue eyes', 'series:metroid' ], [ 'blue eyes' ] ),
    ( [ 'system:filesize < 1MB' ], [ 'system:filesize < 1MB' ] ),
    ( [ 'system:everything' ], [ 'system:everything' ] ),
    # and its inverse goes
    ( [ 'system:inbox' ], [ 'system:archive' ] ),
    ( [ 'system:archive', 'blue eyes' ], [ 'system:inbox' ] ),
    ( [ 'blue eyes' ], [ '-blue eyes' ] ),
    ( [ '-blue eyes' ], [ 'blue eyes' ] ),
    ( [ 'series:*anything*' ], [ '-series:*anything*' ] ),
    ( [ 'system:has audio' ], [ 'system:no audio' ] ),
    ( [ 'system:has exif' ], [ 'system:no exif' ] ),
    ( [ 'system:has icc profile' ], [ 'system:no icc profile' ] ),
    ( [ 'system:is the best quality file of its duplicate group' ], [ 'system:is not the best quality file of its duplicate group' ] ),
    # one system:limit replaces another
    ( [ 'system:limit is 64', 'blue eyes' ], [ 'system:limit is 256' ] ),
    # others stay together
    ( [ 'system:width = 1920' ], [ 'system:width = 1080' ] ),
    ( [ 'system:filesize < 1MB' ], [ 'system:filesize > 10KB' ] ),
    ( [ 'blue eyes' ], [ 'series:metroid' ] ),
    ( [ 'system:inbox' ], [ 'system:has audio' ] ),
    # the order shown: by the parsable text, in human order (numbers
    # without commas?), a namespace as "namespace:*", a wildcard as typed
    ( [], [ 'system:width = 1080', 'system:width = 950', 'system:width = 12000' ] ),
    ( [], [ '-series:*anything*', 'series:metroid', 'sam*', '-blue*', 'blue eyes', '-creator:*anything*' ] ),
    ( [], [ 'system:filesize < 2MB', 'system:filesize < 10KB', 'system:filesize < 300B' ] ),
    # an OR sorts by its first predicate
    ( [], [ 'series:metroid', 'OR:samus|blue eyes', 'character:link', 'OR:system:inbox|creator:someone' ] ),
    ( [ 'OR:samus|blue eyes' ], [ 'OR:samus|blue eyes' ] ),
    ( [], [ 'blue eyes long', 'OR:samus|blue eyes' ] ),
    ( [ 'OR:samus|blue eyes' ], [ 'OR:blue eyes|samus' ] ),
    ( [ 'system:local' ], [ 'system:not local' ] ),
    # several in turn, and the order shown
    ( [], [ 'series:metroid', 'blue eyes', 'system:inbox', 'system:limit is 64', '-character:link', 'creator:*anything*', 'system:filesize < 1MB' ] ),
    ( [ 'system:inbox', 'blue eyes' ], [ 'system:archive', 'blue eyes', 'system:inbox' ] ),
]


# each predicate's inverse (`GetInverseCopy`), which entering it removes
INVERSES = [
    'system:everything', 'system:inbox', 'system:archive', 'system:limit is 64', 'system:local', 'system:not local',
    'system:rating for stars > 2', 'system:rating for stars < 2', 'system:rating for stars = 3', 'system:rating for favourites > 0',
    'blue eyes', '-blue eyes', 'series:*anything*', '-series:*anything*', 'sam*', '-sam*',
    'system:has audio', 'system:no audio', 'system:has transparency', 'system:no transparency',
    'system:has exif', 'system:no exif', 'system:has embedded metadata', 'system:no embedded metadata',
    'system:has icc profile', 'system:no icc profile', 'system:has forced filetype', 'system:no forced filetype',
    'system:has xmp', 'system:has iptc',
    'system:is the best quality file of its duplicate group', 'system:is not the best quality file of its duplicate group',
    'system:has notes', 'system:no notes', 'system:number of notes > 2', 'system:number of notes \u2260 0',
    'system:has urls', 'system:no urls', 'system:number of urls < 3',
    'system:number of words = 0', 'system:number of words > 0', 'system:number of words > 10',
    'system:has duration', 'system:no duration', 'system:duration > 5 seconds',
    'system:number of frames = 0', 'system:number of frames > 0',
    'system:width = 0', 'system:height > 0', 'system:filesize < 1MB', 'system:num tags > 0',
    'system:ratio = 16:9', 'system:ratio \u2260 16:9', 'system:ratio taller than 1:1', 'system:ratio wider than 16:9', 'system:ratio is square', 'system:ratio \u2248 16:9',
    'system:has rating for favourites', 'system:no rating for favourites', 'system:rating for favourites = like',
    'system:rating for stars > 2/5', 'system:has rating for stars',
    'system:rating for counter > 3', 'system:rating for counter < 3', 'system:rating for counter < 0', 'system:rating for counter = 2', 'system:has count for counter',
    'system:all like/dislike ratings rated', 'system:any numerical ratings not rated', 'system:only favourites rated',
]


def record( session ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.search import ClientSearchParseSystemPredicates
    from hydrus.client.search import ClientSearchPredicate

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def make( text ):

        if text.startswith( 'OR:' ):

            return ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_OR_CONTAINER, value = [ make( t ) for t in text[ 3 : ].split( '|' ) ] )


        if text in ( 'system:local', 'system:not local' ):

            predicate_type = ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_LOCAL if text == 'system:local' else ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_NOT_LOCAL

            return ClientSearchPredicate.Predicate( predicate_type )


        if text.startswith( 'system:' ):

            [ predicate ] = ClientSearchParseSystemPredicates.ParseSystemPredicateStringsToPredicates( [ text ] )

            return predicate


        inclusive = not text.startswith( '-' )

        value = text if inclusive else text[ 1 : ]

        if '*' in value and not value.endswith( ':*anything*' ):

            return ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_WILDCARD, value = value, inclusive = inclusive )

        if value.endswith( ':*anything*' ):

            return ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_NAMESPACE, value = value[ : -len( ':*anything*' ) ], inclusive = inclusive )


        return ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, value = value, inclusive = inclusive )


    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    page = qt( lambda: gui._notebook.NewPageQuery( location ) )

    ac = None

    for _ in range( 600 ):

        sidebar = qt( lambda: page.GetSidebar() )

        if sidebar is not None and hasattr( sidebar, '_tag_autocomplete' ):

            ac = sidebar._tag_autocomplete

            break


        time.sleep( 0.05 )


    listbox = ac._predicates_listbox

    def shown():

        return [ p.ToString() for p in listbox._GetPredicatesFromTerms( listbox._ordered_terms ) ]


    def run( start, enter ):

        listbox.SetPredicates( [ make( t ) for t in start ] )

        started = shown()

        steps = []

        for text in enter:

            listbox.EnterPredicates( { make( text ) } )

            steps.append( shown() )


        return { 'start' : start, 'enter' : enter, 'started' : started, 'shown' : steps }


    def inverse( text ):

        try:

            predicate = make( text )

        except Exception as e:

            return { 'text' : text, 'error' : str( e ) }


        inverse = predicate.GetInverseCopy()

        return { 'typed' : text, 'text' : predicate.ToString(), 'inverse' : None if inverse is None else inverse.ToString() }


    return {
        'scenarios' : [ qt( lambda: run( start, enter ) ) for ( start, enter ) in SCENARIOS ],
        'inverses' : [ qt( lambda: inverse( text ) ) for text in INVERSES ],
    }


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
