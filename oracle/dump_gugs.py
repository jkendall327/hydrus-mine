#!/usr/bin/env python3
"""Record the reference's gallery URL generators on random searches.

Random GUGs (URL templates with the replacement phrase in the path or the
query, missing, repeated or empty; assorted separators) built with the
reference's classes turn random searches (reserved characters, percent
signs, unicode, empty terms) into gallery URLs, under both settings of the
client options that change the result. Nested GUGs are run against a domain
manager holding random GUGs, found by key or by name. Each domain manager is
kept serialised, with some page parsers, to check migrating them.

Usage: python oracle/dump_gugs.py > oracle/fixtures/gugs.json
"""

import json
import os
import random
import sys
import types

sys.path.insert( 0, os.path.dirname( __file__ ) )

import dump_page_parsers as PP

from hydrus.core import HydrusData
from hydrus.core import HydrusExceptions
from hydrus.core import HydrusSerialisable
from hydrus.client import ClientGlobals as CG
from hydrus.client.networking import ClientNetworkingDomain
from hydrus.client.networking import ClientNetworkingGUG

# serialising a domain manager serialises its default tag import options,
# which ask whether the client has booted (it has not: keep every service)
CG.client_controller.IsBooted = lambda: False

rng = random.Random( 106 )
PP.rng = rng
PP.F.rng = rng

TEMPLATES = [
    ( 'https://example.com/search?q=%tags%&index=0', '%tags%' ),
    ( 'https://example.com/tags/%tags%', '%tags%' ),
    ( 'https://example.com/%tags%/page/1', '%tags%' ),
    ( 'https://example.com/search?tags=%tags%#results', '%tags%' ),
    ( 'https://example.com/a?x=1&y=%tags%-%tags%', '%tags%' ),
    ( 'https://example.com//double/%tags%', '%tags%' ),
    ( 'https://example.com/search?q={q}&b=a b', '{q}' ),
    ( 'https://example.com/search', '%tags%' ),
    ( 'https://example.com/search?q=%tags%', '' ),
    ( 'example.com/search?q=%tags%', '%tags%' ),
    ( 'https://example.com/path?%tags%', '%tags%' ),
    ( 'https://exämple.com/s?q=%tags%', '%tags%' ),
    ( 'https://example.com/s/%tags%?page=1', '%tags%' ),
    ( 'https://example.com/s?x=1/%tags%', '%tags%' ),
    ( 'https://example.com/%tags%%20x', '%tags%' ),
]

SEPARATORS = [ '+', '+', ' ', '%20', ',', '_', '%2B', '&tags=', '' ]

TERMS = [ 'blue_eyes', '6+girls', 'rating:safe', 'a&b', 'x=y', '100%', '50%off', 'caf%C3%A9', 'café', '日本', 'a/b', 'what?', '#tag', 'order:score', '-solo', '"quoted"', "o'neil", '%20', 'a%20b', '', '~or', '*wild*', 'semi;colon', '%zz' ]


def random_gug( name = None ):

    ( template, phrase ) = rng.choice( TEMPLATES )

    return ClientNetworkingGUG.GalleryURLGenerator(
        name if name is not None else rng.choice( [ 'booru tag search', 'artist lookup', 'site search' ] ),
        url_template = template,
        replacement_phrase = phrase,
        search_terms_separator = rng.choice( SEPARATORS ),
        initial_search_text = rng.choice( [ 'tag search', 'artist id' ] ),
        example_search_text = random_query()
    )


def random_query():

    return rng.choice( [ ' ', '  ' ] ).join( rng.choice( TERMS ) for _ in range( rng.randint( 1, 4 ) ) )


def set_options( percent_twenty, collapse ):

    CG.client_controller.new_options.SetBoolean( 'replace_percent_twenty_with_space_in_gug_input', percent_twenty )
    CG.client_controller.new_options.SetBoolean( 'remove_leading_url_double_slashes', collapse )


def run( f ):

    try:

        return { 'urls': list( f() ) }

    except HydrusExceptions.GUGException as e:

        return { 'error': str( e ) }


def main():

    cases = []

    for _ in range( 600 ):

        gug = random_gug()
        query = random_query()
        percent_twenty = rng.random() < 0.3
        collapse = rng.random() < 0.3

        set_options( percent_twenty, collapse )

        case = { 'gug': gug.GetSerialisableTuple(), 'query': query, 'percent_twenty_is_space': percent_twenty, 'collapse_leading_slashes': collapse }
        case.update( run( lambda: gug.GenerateGalleryURLs( query ) ) )

        cases.append( case )


    managers = []

    for _ in range( 40 ):

        dm = ClientNetworkingDomain.NetworkDomainManager()

        CG.client_controller.network_engine = types.SimpleNamespace( domain_manager = dm )

        gugs = [ random_gug( name = rng.choice( [ 'a', 'b', 'c', 'd', 'e' ] ) ) for _ in range( rng.randint( 2, 6 ) ) ]

        nested = []

        for i in range( rng.randint( 1, 3 ) ):

            names = []

            for _ in range( rng.randint( 1, 4 ) ):

                roll = rng.random()

                if roll < 0.4:

                    # by key (with whatever name)
                    g = rng.choice( gugs )
                    names.append( ( g.GetGUGKey(), rng.choice( [ g.GetName(), 'renamed' ] ) ) )

                elif roll < 0.8:

                    # by name, the key being stale
                    names.append( ( HydrusData.GenerateKey(), rng.choice( [ 'a', 'b', 'c', 'd', 'e', 'f' ] ) ) )

                else:

                    names.append( ( rng.choice( gugs ).GetGUGKey(), '' ) )


            nested.append( ClientNetworkingGUG.NestedGalleryURLGenerator( 'nested ' + str( i ), initial_search_text = 'nested search', gug_keys_and_names = names ) )


        dm.SetGUGs( gugs + nested )

        if rng.random() < 0.5:

            dm.SetGUGKeysToDisplay( [ g.GetGUGKey() for g in gugs + nested if rng.random() < 0.5 ] )


        parsers = [ PP.page_parser( rng.choice( [ 'html', 'json' ] ) ) for _ in range( rng.randint( 0, 3 ) ) ]

        for ( i, p ) in enumerate( parsers ):

            p.SetName( 'parser ' + str( i ) )


        dm.SetParsers( parsers )

        runs = []

        for n in nested:

            for _ in range( 3 ):

                query = random_query()
                percent_twenty = rng.random() < 0.3
                collapse = rng.random() < 0.3

                set_options( percent_twenty, collapse )

                r = { 'nested': n.GetName(), 'query': query, 'percent_twenty_is_space': percent_twenty, 'collapse_leading_slashes': collapse }
                r.update( run( lambda: n.GenerateGalleryURLs( query ) ) )

                runs.append( r )



        managers.append( {
            'manager': dm.GetSerialisableTuple(),
            'gugs': [ [ g.GetGUGKey().hex(), g.GetName() ] for g in dm.GetGUGs() ],
            'keys_to_display': sorted( k.hex() for k in dm.GetGUGKeysToDisplay() ),
            'parsers': [ [ p.GetParserKey().hex(), p.GetName() ] for p in dm.GetParsers() ],
            'runs': runs,
        } )


    json.dump( { 'cases': cases, 'managers': managers }, sys.stdout, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
