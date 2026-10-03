#!/usr/bin/env python3
"""Record how the reference describes sidecar routers.

The sidecars button and the routers list show each router as its
`ToString( pretty = True )` says ("Taking notes from media, applying some
sorting, sending to .json sidecar (notes)."), which says what each of its
importers and its exporter are, by their own `ToString`s, and whether each
string processor makes changes (`StringProcessor.ToString`: "some
conversion, sorting"). The button says "no sidecars", the one router, or
"3 sidecar actions" (elided to 64 characters).

This builds random routers from every kind of importer and exporter
(media notes, tags, timestamps of every kind and urls; .txt and .json
sidecars) with random processors, service keys (some unknown to the
client, "unknown service") and sidecar suffixes, and records each as the
reference serialises it, with its pretty and plain descriptions; and
lists of 0-3 of them with the button's label.

Usage: python oracle/dump_sidecar_descriptions.py > oracle/fixtures/sidecar_descriptions.json
"""

import json
import os
import random
import sys
import types

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
from hydrus.client.metadata import ClientTags
from hydrus.client.parsing import ClientParsing as P
from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusNumbers
from hydrus.core import HydrusTags
from hydrus.core import HydrusText

NAMES = {
    CC.DEFAULT_LOCAL_TAG_SERVICE_KEY : 'my tags',
    CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY : 'downloader tags',
    CC.COMBINED_TAG_SERVICE_KEY : 'all known tags',
    CC.LOCAL_FILE_SERVICE_KEY : 'my files',
    CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY : 'all local files',
    CC.TRASH_SERVICE_KEY : 'trash',
}

UNKNOWN = bytes.fromhex( 'ab' * 32 )

services_manager = types.SimpleNamespace( GetNameSafe = lambda key: NAMES.get( key, 'unknown service' ) )

CG.client_controller = types.SimpleNamespace( parsing_cache = ClientCaches.ParsingCache(), new_options = ClientOptions.ClientOptions(), services_manager = services_manager )

rng = random.Random( 688 )

SUFFIXES = [ '', '', 'tags', 'urls', 'a.b' ]
SEPARATORS = [ '\n', '\n', ', ', '||' ]


def processor():

    p = ClientStrings.StringProcessor()

    steps = []

    for _ in range( rng.choice( [ 0, 0, 1, 1, 2, 3 ] ) ):

        steps.append( rng.choice( [
            lambda: ClientStrings.StringConverter( conversions = [ ( ClientStrings.STRING_CONVERSION_PREPEND_TEXT, 'pre:' ) ] ),
            lambda: ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_REGEX, match_value = r'[a-z]' ),
            lambda: ClientStrings.StringSplitter( separator = ',' ),
            lambda: ClientStrings.StringSorter( sort_type = 1, asc = True ),
            lambda: ClientStrings.StringSlicer( index_start = 0, index_end = 2 ),
            lambda: ClientStrings.StringJoiner( joiner = ',' ),
            lambda: ClientStrings.StringTagFilter( tag_filter = HydrusTags.TagFilter() ),
        ] )() )


    p.SetProcessingSteps( steps )

    return p


def tag_service():

    return rng.choice( list( NAMES.keys() )[:3] + [ UNKNOWN ] )


def stub():

    return rng.choice( [
        lambda: ClientTime.TimestampData.STATICSimpleStub( HC.TIMESTAMP_TYPE_ARCHIVED ),
        lambda: ClientTime.TimestampData.STATICSimpleStub( HC.TIMESTAMP_TYPE_MODIFIED_FILE ),
        lambda: ClientTime.TimestampData.STATICSimpleStub( HC.TIMESTAMP_TYPE_MODIFIED_AGGREGATE ),
        lambda: ClientTime.TimestampData( timestamp_type = HC.TIMESTAMP_TYPE_IMPORTED, location = rng.choice( [ CC.LOCAL_FILE_SERVICE_KEY, CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, UNKNOWN ] ) ),
        lambda: ClientTime.TimestampData( timestamp_type = HC.TIMESTAMP_TYPE_DELETED, location = CC.TRASH_SERVICE_KEY ),
        lambda: ClientTime.TimestampData( timestamp_type = HC.TIMESTAMP_TYPE_PREVIOUSLY_IMPORTED, location = CC.LOCAL_FILE_SERVICE_KEY ),
        lambda: ClientTime.TimestampData( timestamp_type = HC.TIMESTAMP_TYPE_LAST_VIEWED, location = rng.choice( [ 0, 1, 2, 3, 4, 5 ] ) ),
        lambda: ClientTime.TimestampData( timestamp_type = HC.TIMESTAMP_TYPE_MODIFIED_DOMAIN, location = 'example.com' ),
    ] )()


def converter():

    return ClientStrings.StringConverter( example_string = 'my_image.jpg.txt' )


def json_formula():

    rules = [ ( P.JSON_PARSE_RULE_TYPE_DICT_KEY, ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = 'tags', example_string = 'tags' ) ), ( P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None ) ]

    return P.ParseFormulaJSON( parse_rules = rules, content_to_fetch = P.JSON_CONTENT_STRING )


def importer():

    return rng.choice( [
        lambda: I.SingleFileMetadataImporterMediaNotes( string_processor = processor() ),
        lambda: I.SingleFileMetadataImporterMediaTags( string_processor = processor(), service_key = tag_service(), tag_display_type = rng.choice( [ ClientTags.TAG_DISPLAY_STORAGE, ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL ] ) ),
        lambda: I.SingleFileMetadataImporterMediaTimestamps( string_processor = processor(), timestamp_data_stub = stub() ),
        lambda: I.SingleFileMetadataImporterMediaURLs( string_processor = processor() ),
        lambda: I.SingleFileMetadataImporterTXT( string_processor = processor(), remove_actual_filename_ext = False, suffix = rng.choice( SUFFIXES ), filename_string_converter = converter(), separator = rng.choice( SEPARATORS ) ),
        lambda: I.SingleFileMetadataImporterJSON( string_processor = processor(), remove_actual_filename_ext = False, suffix = rng.choice( SUFFIXES ), filename_string_converter = converter(), json_parsing_formula = json_formula() ),
    ] )()


def exporter():

    return rng.choice( [
        lambda: E.SingleFileMetadataExporterMediaNotes( forced_name = rng.choice( [ None, 'comment' ] ) ),
        lambda: E.SingleFileMetadataExporterMediaTags( service_key = tag_service() ),
        lambda: E.SingleFileMetadataExporterMediaTimestamps( timestamp_data_stub = stub() ),
        lambda: E.SingleFileMetadataExporterMediaURLs(),
        lambda: E.SingleFileMetadataExporterTXT( remove_actual_filename_ext = False, suffix = rng.choice( SUFFIXES ), filename_string_converter = converter(), separator = rng.choice( SEPARATORS ) ),
        lambda: E.SingleFileMetadataExporterJSON( remove_actual_filename_ext = False, suffix = rng.choice( SUFFIXES ), filename_string_converter = converter(), nested_object_names = rng.choice( [ [], [ 'tags' ], [ 'meta', 'tags' ] ] ) ),
    ] )()


def router():

    importers = [ importer() for _ in range( rng.choice( [ 0, 1, 1, 1, 2, 3 ] ) ) ]

    # (the default processor is a human sort)
    string_processor = None if rng.random() < 0.6 else processor()

    return MM.SingleFileMetadataRouter( importers = importers, string_processor = string_processor, exporter = exporter() )


def button_label( routers ):

    # `SingleFileMetadataRoutersButton._RefreshLabel`
    if len( routers ) == 0:

        text = 'no sidecars'

    elif len( routers ) == 1:

        text = routers[0].ToString( pretty = True )

    else:

        text = '{} sidecar actions'.format( HydrusNumbers.ToHumanInt( len( routers ) ) )


    return HydrusText.ElideText( text, 64 )


def main():

    cases = []

    for _ in range( 300 ):

        r = router()

        cases.append( {
            'router' : r.GetSerialisableTuple(),
            'pretty' : r.ToString( pretty = True ),
            'plain' : r.ToString(),
        } )


    buttons = []

    for _ in range( 40 ):

        routers = [ router() for _ in range( rng.choice( [ 0, 1, 1, 2, 3 ] ) ) ]

        buttons.append( { 'routers' : [ r.GetSerialisableTuple() for r in routers ], 'label' : button_label( routers ) } )


    names = { key.hex() : name for ( key, name ) in NAMES.items() }

    json.dump( { 'names' : names, 'cases' : cases, 'buttons' : buttons }, sys.stdout, indent = 1, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
