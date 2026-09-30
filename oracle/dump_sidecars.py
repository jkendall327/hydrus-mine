#!/usr/bin/env python3
"""Record the reference's sidecar routing between sidecar files.

Import and export folders move metadata through routers: importers gather
strings, the router dedupes and processes them, an exporter writes them.
When both ends are sidecars the reference's `Router.Work` needs no client,
so this runs random routers (1-3 `.txt`/`.json` importers with random
naming, separators, JSON formulas and processors; a `.txt` or `.json`
exporter, sometimes nesting into existing JSON) over a file with random
sidecars beside it, and records every file in the folder afterwards.

Each case keeps the router as the reference serialises it, the file's name,
the sidecars before (name to text), what `GetPossibleImporterSidecarPaths`
names, whether `Work` did anything or raised, and the folder afterwards.

Usage: python oracle/dump_sidecars.py > oracle/fixtures/sidecars.json
"""

import json
import os
import random
import sys
import tempfile
import types

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientStrings
from hydrus.client import ClientOptions
from hydrus.client.caches import ClientCaches
from hydrus.client.metadata import ClientMetadataMigration as MM
from hydrus.client.metadata import ClientMetadataMigrationExporters as E
from hydrus.client.metadata import ClientMetadataMigrationImporters as I
from hydrus.client.parsing import ClientParsing as P

CG.client_controller = types.SimpleNamespace( parsing_cache = ClientCaches.ParsingCache(), new_options = ClientOptions.ClientOptions() )

rng = random.Random( 688 )

FILE_NAMES = [ 'image.jpg', 'image.png', 'noext', 'two.dots.gif', 'Ünïcode 画像.webp', 'spaced name.jpeg' ]
SUFFIXES = [ '', '', 'tags', 'urls', 'a.b' ]
ROWS = [ 'blue eyes', 'Blue Eyes', 'character:samus aran', 'page 10', 'page 2', ' padded ', '日本語', 'é', '', 'https://example.com/post/5', 'series:metroid', 'a,b', 'x' ]
SEPARATORS = [ '\n', '\n', ', ', ',', '||' ]


def converter():

    if rng.random() < 0.7:

        return ClientStrings.StringConverter( example_string = 'image.jpg.txt' )


    conversion = rng.choice( [
        ( ClientStrings.STRING_CONVERSION_PREPEND_TEXT, 'x_' ),
        ( ClientStrings.STRING_CONVERSION_APPEND_TEXT, '.bak' ),
        ( ClientStrings.STRING_CONVERSION_REMOVE_TEXT_FROM_BEGINNING, 2 ),
    ] )

    return ClientStrings.StringConverter( conversions = [ conversion ], example_string = 'image.jpg.txt' )


def naming():

    return ( rng.random() < 0.3, rng.choice( SUFFIXES ), converter() )


def processor():

    p = ClientStrings.StringProcessor()

    roll = rng.random()

    if roll < 0.6:

        return p


    if roll < 0.75:

        steps = [ ClientStrings.StringSorter( sort_type = rng.choice( [ 1, 2, 3 ] ), asc = rng.random() < 0.5 ) ]

    elif roll < 0.9:

        steps = [ ClientStrings.StringConverter( conversions = [ ( ClientStrings.STRING_CONVERSION_PREPEND_TEXT, 'pre:' ) ] ) ]

    else:

        steps = [ ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_REGEX, match_value = r'[a-z]' ) ]


    p.SetProcessingSteps( steps )

    return p


def json_formula():

    roll = rng.random()

    if roll < 0.5:

        rules = [ ( P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None ) ]

    elif roll < 0.8:

        rules = [ ( P.JSON_PARSE_RULE_TYPE_DICT_KEY, ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = 'tags', example_string = 'tags' ) ), ( P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None ) ]

    else:

        rules = [ ( P.JSON_PARSE_RULE_TYPE_DICT_KEY, ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = 'meta', example_string = 'meta' ) ), ( P.JSON_PARSE_RULE_TYPE_DICT_KEY, ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = 'tags', example_string = 'tags' ) ), ( P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None ) ]


    return P.ParseFormulaJSON( parse_rules = rules, content_to_fetch = P.JSON_CONTENT_STRING )


def importer():

    ( remove, suffix, conv ) = naming()

    if rng.random() < 0.6:

        return I.SingleFileMetadataImporterTXT( string_processor = processor(), remove_actual_filename_ext = remove, suffix = suffix, filename_string_converter = conv, separator = rng.choice( SEPARATORS ) )

    else:

        return I.SingleFileMetadataImporterJSON( string_processor = processor(), remove_actual_filename_ext = remove, suffix = suffix, filename_string_converter = conv, json_parsing_formula = json_formula() )



def exporter():

    ( remove, suffix, conv ) = naming()

    if rng.random() < 0.5:

        return E.SingleFileMetadataExporterTXT( remove_actual_filename_ext = remove, suffix = suffix, filename_string_converter = conv, separator = rng.choice( SEPARATORS ) )

    else:

        nested = rng.choice( [ [], [], [ 'tags' ], [ 'meta', 'tags' ], [ 'x' ] ] )

        return E.SingleFileMetadataExporterJSON( remove_actual_filename_ext = remove, suffix = suffix, filename_string_converter = conv, nested_object_names = nested )



def router_processor():

    if rng.random() < 0.8:

        return None # the default: a human sort


    p = ClientStrings.StringProcessor()

    p.SetProcessingSteps( [ ClientStrings.StringSorter( sort_type = rng.choice( [ 1, 3 ] ), asc = rng.random() < 0.5 ) ] )

    return p


def txt_text():

    rows = [ rng.choice( ROWS ) for _ in range( rng.randint( 0, 6 ) ) ]

    sep = rng.choice( [ '\n', '\r\n', '\n', ', ' ] )

    text = sep.join( rows )

    if rng.random() < 0.3:

        text += sep


    return text


def json_text():

    rows = [ rng.choice( ROWS ) for _ in range( rng.randint( 0, 5 ) ) ]

    roll = rng.random()

    if roll < 0.4:

        value = rows

    elif roll < 0.7:

        value = { 'tags' : rows, 'other' : 1 }

    elif roll < 0.9:

        value = { 'meta' : { 'tags' : rows }, 'tags' : 'not a list' }

    else:

        return '{ not json'


    return json.dumps( value, ensure_ascii = rng.random() < 0.5 )


def main():

    cases = []

    for _ in range( 400 ):

        router = MM.SingleFileMetadataRouter( importers = [ importer() for _ in range( rng.randint( 1, 3 ) ) ], string_processor = router_processor(), exporter = exporter() )

        file_name = rng.choice( FILE_NAMES )

        with tempfile.TemporaryDirectory() as d:

            file_path = os.path.join( d, file_name )

            with open( file_path, 'wb' ) as f:

                f.write( b'file' )


            # sidecars: some that the importers name, some that they don't
            wanted = sorted( { os.path.basename( p ) for p in router.GetPossibleImporterSidecarPaths( file_path ) } )

            before = {}

            for name in wanted:

                if rng.random() < 0.6:

                    before[ name ] = json_text() if name.lower().endswith( '.json' ) else txt_text()



            if isinstance( router.GetExporter(), E.SingleFileMetadataExporterJSON ) and rng.random() < 0.5:

                name = os.path.basename( router.GetExporter().GetExportPath( file_path ) )

                if name not in before:

                    before[ name ] = json_text()



            for ( name, text ) in before.items():

                with open( os.path.join( d, name ), 'w', encoding = 'utf-8', newline = '' ) as f:

                    f.write( text )



            case = {
                'router' : router.GetSerialisableTuple(),
                'file' : file_name,
                'before' : before,
                'possible' : sorted( os.path.relpath( p, d ) for p in router.GetPossibleImporterSidecarPaths( file_path ) ),
            }

            try:

                case[ 'worked' ] = router.Work( None, file_path )

            except Exception as e:

                case[ 'error' ] = str( e )[ :200 ]


            after = {}

            for name in sorted( os.listdir( d ) ):

                if name == file_name:

                    continue


                with open( os.path.join( d, name ), 'rb' ) as f:

                    after[ name ] = f.read().decode( 'utf-8' )



            case[ 'after' ] = after

            cases.append( case )



    json.dump( { 'cases' : cases }, sys.stdout, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
