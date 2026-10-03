#!/usr/bin/env python3
"""Record what the "filename tagging" dialog's "sidecars" tab shows for
each file.

The tab (`EditLocalImportFilenameTaggingPanel._MetadataRoutersPanel`)
lists the files to import with a "metadata" column: for each router whose
sidecars give anything for the file, the strings through the router's
processor and human-sorted, then by its destination: 'to "my tags": a, b'
(the tags cleaned and sorted), "1 note: ..." / "3 notes: ..., ...", "1
URL: ..." / "2 URLs: ...", or a timestamp ("archived time: 2023-01-02
03:04:05", "Could not parse time! ...", "archived time: 2 times?"); the
column joins them with " | " (`_GetPrettyStrings`).

This makes random .txt and .json sidecars (tags, multi-line notes, URLs,
times in seconds and not, blank lines and repeats) for 60 files in a
temporary folder, and random routers from them (.txt and .json sources
with suffixes, random processors) to a file's tags, notes, URLs and
timestamps; and records each file's sidecars, each case's routers, and
what the tab shows for each file. Times are shown in UTC (TZ is set).
The reference gathers a router's sidecar rows in a set before its
processor, so a step that cares about order (a slice), or rows the human
sort ties ("samus" and "Samus"), come out in an arbitrary order; the cases
here have neither (two PYTHONHASHSEEDs record the same).

Usage: QT_QPA_PLATFORM=offscreen python oracle/dump_sidecar_previews.py > oracle/fixtures/sidecar_previews.json
"""

import json
import os
import random
import sys
import tempfile
import time
import types

os.environ[ 'TZ' ] = 'UTC'
time.tzset()

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.client import ClientConstants as CC
from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientOptions
from hydrus.client import ClientStrings
from hydrus.client import ClientTime
from hydrus.client.caches import ClientCaches
from hydrus.client.metadata import ClientMetadataMigration as MM
from hydrus.client.metadata import ClientMetadataMigrationExporters as E
from hydrus.client.metadata import ClientMetadataMigrationImporters as I
from hydrus.client.parsing import ClientParsing as P
from hydrus.core import HydrusConstants as HC

NAMES = {
    CC.DEFAULT_LOCAL_TAG_SERVICE_KEY : 'my tags',
    CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY : 'downloader tags',
    CC.LOCAL_FILE_SERVICE_KEY : 'my files',
}

UNKNOWN = bytes.fromhex( 'ab' * 32 )

services_manager = types.SimpleNamespace( GetNameSafe = lambda key: NAMES.get( key, 'unknown service' ) )

CG.client_controller = types.SimpleNamespace( parsing_cache = ClientCaches.ParsingCache(), new_options = ClientOptions.ClientOptions(), services_manager = services_manager )

from hydrus.client.gui.importing import ClientGUIImport

rng = random.Random( 4417 )

SUFFIXES = [ '', '', 'tags', 'notes' ]

ROWS = [
    'samus', 'creator:Someone', 'series:metroid', 'character:samus aran', '  spaced  ', 'page:10', 'page:9', '', '', ':colon', 'a, b',
    'a note\nwith two lines', 'one line note', 'x' * 80, 'line\n\nthree',
    'https://example.com/post/1', 'https://example.com/post/22?q=' + 'z' * 60,
    '1672628645', '1672628645.75', '0', '-5', ' 1700000000 ', 'not a time', '1e9', 'nan',
]


def processor():

    p = ClientStrings.StringProcessor()

    steps = []

    for _ in range( rng.choice( [ 0, 0, 0, 1, 2 ] ) ):

        steps.append( rng.choice( [
            lambda: ClientStrings.StringConverter( conversions = [ ( ClientStrings.STRING_CONVERSION_PREPEND_TEXT, 'pre:' ) ] ),
            lambda: ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_REGEX, match_value = r'[a-z]' ),
            lambda: ClientStrings.StringSplitter( separator = ',' ),
            lambda: ClientStrings.StringSorter( sort_type = 1, asc = False ),
        ] )() )


    p.SetProcessingSteps( steps )

    return p


def json_formula():

    rules = [ ( P.JSON_PARSE_RULE_TYPE_DICT_KEY, ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = 'rows', example_string = 'rows' ) ), ( P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None ) ]

    return P.ParseFormulaJSON( parse_rules = rules, content_to_fetch = P.JSON_CONTENT_STRING )


def converter():

    return ClientStrings.StringConverter( example_string = 'my_image.jpg.txt' )


def importer():

    return rng.choice( [
        lambda: I.SingleFileMetadataImporterTXT( string_processor = processor(), remove_actual_filename_ext = False, suffix = rng.choice( SUFFIXES ), filename_string_converter = converter(), separator = '\n' ),
        lambda: I.SingleFileMetadataImporterJSON( string_processor = processor(), remove_actual_filename_ext = False, suffix = rng.choice( SUFFIXES ), filename_string_converter = converter(), json_parsing_formula = json_formula() ),
        # (not a sidecar: the tab leaves it out)
        lambda: I.SingleFileMetadataImporterMediaURLs( string_processor = processor() ),
    ] )()


def stub():

    return rng.choice( [
        lambda: ClientTime.TimestampData.STATICSimpleStub( HC.TIMESTAMP_TYPE_ARCHIVED ),
        lambda: ClientTime.TimestampData.STATICSimpleStub( HC.TIMESTAMP_TYPE_MODIFIED_FILE ),
        lambda: ClientTime.TimestampData( timestamp_type = HC.TIMESTAMP_TYPE_IMPORTED, location = CC.LOCAL_FILE_SERVICE_KEY ),
    ] )()


def exporter():

    return rng.choice( [
        lambda: E.SingleFileMetadataExporterMediaTags( service_key = rng.choice( [ CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY, UNKNOWN ] ) ),
        lambda: E.SingleFileMetadataExporterMediaNotes( forced_name = None ),
        lambda: E.SingleFileMetadataExporterMediaURLs(),
        lambda: E.SingleFileMetadataExporterMediaTimestamps( timestamp_data_stub = stub() ),
    ] )()


def router():

    importers = [ importer() for _ in range( rng.choice( [ 1, 1, 1, 2 ] ) ) ]

    string_processor = None if rng.random() < 0.7 else processor()

    return MM.SingleFileMetadataRouter( importers = importers, string_processor = string_processor, exporter = exporter() )


def rows():

    return [ rng.choice( ROWS ) for _ in range( rng.choice( [ 0, 1, 1, 1, 2, 3, 5 ] ) ) ]


def main():

    directory = tempfile.mkdtemp()

    files = []

    for i in range( 60 ):

        name = f'file{i}.jpg'
        sidecars = {}

        for suffix in SUFFIXES[ 1: ]:

            stem = name if suffix == '' else f'{name}.{suffix}'

            if rng.random() < 0.6:

                sidecars[ f'{stem}.txt' ] = '\n'.join( rows() )


            if rng.random() < 0.4:

                sidecars[ f'{stem}.json' ] = json.dumps( { 'rows' : rows() } )



        for ( sidecar, text ) in sidecars.items():

            with open( os.path.join( directory, sidecar ), 'w', encoding = 'utf-8' ) as f:

                f.write( text )



        files.append( { 'name' : name, 'sidecars' : sidecars } )


    cases = []

    for _ in range( 40 ):

        routers = [ router() for _ in range( rng.choice( [ 1, 1, 2, 3 ] ) ) ]

        panel = types.SimpleNamespace( _metadata_routers_control = types.SimpleNamespace( GetValue = lambda routers = routers: routers ) )

        shown = {}

        for f in files:

            path = os.path.join( directory, f[ 'name' ] )

            strings = ClientGUIImport.EditLocalImportFilenameTaggingPanel._MetadataRoutersPanel._GetPrettyStrings( panel, path )

            shown[ f[ 'name' ] ] = strings


        cases.append( { 'routers' : [ r.GetSerialisableTuple() for r in routers ], 'shown' : shown } )


    names = { key.hex() : name for ( key, name ) in NAMES.items() }

    json.dump( { 'names' : names, 'files' : files, 'cases' : cases }, sys.stdout, indent = 1, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
