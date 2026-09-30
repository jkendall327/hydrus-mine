#!/usr/bin/env python3
"""Record the reference's import option defaults and how it layers them.

Random import options managers (random defaults per kind of importer, per
URL class, favourites) built with the reference's classes, each asked for
the full options of random imports (kind of importer, the importer's own
options, the URL classes involved). Each case keeps the serialised manager
and, per import, the serialised full options container the reference
produced. The reference's default manager is recorded too.

Usage: python oracle/dump_import_options.py > oracle/fixtures/import_options.json
"""

import json
import os
import random
import sys
import types

sys.path.insert( 0, os.path.dirname( __file__ ) )

import dump_formulas # the fake controller

from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusExceptions
from hydrus.core import HydrusTags
from hydrus.client import ClientConstants as CC
from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientLocation
from hydrus.client.importing.options import ImportOptionsConstants as IOC
from hydrus.client.importing.options import ImportOptionsContainer
from hydrus.client.importing.options import ImportOptionsManager
from hydrus.client.importing.options import ExternalProgramsImportOptions
from hydrus.client.importing.options import FileFilteringImportOptions
from hydrus.client.importing.options import LocationImportOptions
from hydrus.client.importing.options import NoteImportOptions
from hydrus.client.importing.options import PrefetchImportOptions
from hydrus.client.importing.options import PresentationImportOptions
from hydrus.client.importing.options import TagFilteringImportOptions
from hydrus.client.importing.options import TagImportOptions


def missing_url_class( key ):

    raise HydrusExceptions.DataMissing()


CG.client_controller.IsBooted = lambda: False
CG.client_controller.network_engine = types.SimpleNamespace( domain_manager = types.SimpleNamespace( GetURLClassFromKey = missing_url_class ) )

rng = random.Random( 143 )

TAG_SERVICE_KEYS = [ CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY, bytes.fromhex( 'ab' * 32 ) ]
FILE_SERVICE_KEYS = [ CC.LOCAL_FILE_SERVICE_KEY, bytes.fromhex( 'cd' * 32 ), CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY ]
URL_CLASS_KEYS = [ bytes.fromhex( '%02x' % i * 32 ) for i in range( 1, 5 ) ]
SPECIFIC_MIMES = [ HC.IMAGE_JPEG, HC.IMAGE_PNG, HC.ANIMATION_GIF, HC.VIDEO_MP4, HC.AUDIO_MP3, HC.APPLICATION_PDF, HC.IMAGE_WEBP ]


def maybe( value ):

    return value if rng.random() < 0.5 else None


def tag_filter():

    f = HydrusTags.TagFilter()

    for _ in range( rng.randint( 0, 3 ) ):

        f.SetRule( rng.choice( [ '', ':', 'creator:', 'series:', 'goblin', 'meta:ai generated' ] ), rng.choice( [ HC.FILTER_WHITELIST, HC.FILTER_BLACKLIST ] ) )


    return f


def location_context( keys ):

    return ClientLocation.LocationContext( current_service_keys = rng.sample( keys, rng.randint( 1, 2 ) ) )


def random_options( kind ):

    if kind == IOC.IMPORT_OPTIONS_TYPE_PREFETCH:

        o = PrefetchImportOptions.PrefetchImportOptions()
        o.SetPreImportHashCheckType( rng.randint( 0, 2 ) )
        o.SetPreImportURLCheckType( rng.randint( 0, 2 ) )
        o.SetPreImportURLCheckLooksForNeighbourSpam( rng.random() < 0.5 )
        o._fetch_metadata_even_if_url_recognised_and_file_already_in_db = rng.random() < 0.5
        o._fetch_metadata_even_if_hash_recognised_and_file_already_in_db = rng.random() < 0.5

    elif kind == IOC.IMPORT_OPTIONS_TYPE_FILE_FILTERING:

        o = FileFilteringImportOptions.FileFilteringImportOptions()
        o.SetExcludesDeleted( rng.random() < 0.5 )
        o.SetAllowsDecompressionBombs( rng.random() < 0.5 )

        if rng.random() < 0.5:

            o.SetAllowedSpecificFiletypes( rng.sample( SPECIFIC_MIMES, rng.randint( 1, 5 ) ) )


        o.SetMinSize( maybe( rng.randint( 1, 1000 ) ) )
        o.SetMaxSize( maybe( rng.randint( 1000, 10 ** 9 ) ) )
        o.SetMaxGifSize( maybe( rng.randint( 1000, 10 ** 8 ) ) )
        o.SetMinResolution( maybe( ( rng.randint( 1, 100 ), rng.randint( 1, 100 ) ) ) )
        o.SetMaxResolution( maybe( ( rng.randint( 100, 10000 ), rng.randint( 100, 10000 ) ) ) )

    elif kind == IOC.IMPORT_OPTIONS_TYPE_TAG_FILTERING:

        o = TagFilteringImportOptions.TagFilteringImportOptions( tag_blacklist = tag_filter(), tag_whitelist = rng.sample( [ 'blue eyes', 'series:x', 'solo' ], rng.randint( 0, 2 ) ) )

    elif kind == IOC.IMPORT_OPTIONS_TYPE_LOCATIONS:

        o = LocationImportOptions.LocationImportOptions()
        o.SetDestinationLocationContext( location_context( FILE_SERVICE_KEYS[ :2 ] ) )
        o.SetAutomaticallyArchives( rng.random() < 0.5 )
        o.SetShouldAssociatePrimaryURLs( rng.random() < 0.5 )
        o.SetShouldAssociateSourceURLs( rng.random() < 0.5 )
        o.SetDoAutomaticArchiveOnAlreadyInDBFiles( rng.random() < 0.5 )
        o.SetDoImportDestinationsOnAlreadyInDBFiles( rng.random() < 0.5 )

    elif kind == IOC.IMPORT_OPTIONS_TYPE_TAGS:

        services = {}

        for key in rng.sample( TAG_SERVICE_KEYS, rng.randint( 0, 2 ) ):

            services[ key ] = TagImportOptions.ServiceTagImportOptions(
                get_tags = rng.random() < 0.6,
                get_tags_filter = tag_filter(),
                additional_tags = rng.sample( [ 'from hydrus', 'series:test', 'blue' ], rng.randint( 0, 2 ) ),
                to_new_files = rng.random() < 0.7,
                to_already_in_inbox = rng.random() < 0.7,
                to_already_in_archive = rng.random() < 0.7,
                only_add_existing_tags = rng.random() < 0.3,
                only_add_existing_tags_filter = tag_filter(),
                get_tags_overwrite_deleted = rng.random() < 0.3,
                additional_tags_overwrite_deleted = rng.random() < 0.3
            )


        o = TagImportOptions.TagImportOptions( service_keys_to_service_tag_import_options = services )

    elif kind == IOC.IMPORT_OPTIONS_TYPE_NOTES:

        o = NoteImportOptions.NoteImportOptions()
        o.SetGetNotes( rng.random() < 0.7 )
        o.SetExtendExistingNoteIfPossible( rng.random() < 0.5 )
        o.SetConflictResolution( rng.randint( 0, 3 ) )
        o.SetNameWhitelist( rng.sample( [ 'comment', 'artist note' ], rng.randint( 0, 2 ) ) )
        o.SetAllNameOverride( maybe( 'site note' ) )
        o.SetNamesToNameOverrides( dict( rng.sample( [ ( 'comment', 'booru comment' ), ( 'description', 'desc' ) ], rng.randint( 0, 2 ) ) ) )

    elif kind == IOC.IMPORT_OPTIONS_TYPE_PRESENTATION:

        o = PresentationImportOptions.PresentationImportOptions()
        o.SetLocationContext( location_context( FILE_SERVICE_KEYS ) )
        o.SetPresentationStatus( rng.randint( 0, 2 ) )
        o.SetPresentationInbox( rng.randint( 0, 2 ) )

    else:

        raise NotImplementedError( kind )


    return o


KINDS = [
    IOC.IMPORT_OPTIONS_TYPE_PREFETCH,
    IOC.IMPORT_OPTIONS_TYPE_FILE_FILTERING,
    IOC.IMPORT_OPTIONS_TYPE_TAG_FILTERING,
    IOC.IMPORT_OPTIONS_TYPE_LOCATIONS,
    IOC.IMPORT_OPTIONS_TYPE_TAGS,
    IOC.IMPORT_OPTIONS_TYPE_NOTES,
    IOC.IMPORT_OPTIONS_TYPE_PRESENTATION,
]


def random_slice( p = 0.35 ):

    c = ImportOptionsContainer.ImportOptionsContainer()

    for kind in KINDS:

        if rng.random() < p:

            c.SetImportOptions( random_options( kind ) )



    return c


def random_manager():

    m = ImportOptionsManager.ImportOptionsManager.STATICGetDefaultInitialisedManager()

    for caller in IOC.IMPORT_OPTIONS_CALLER_TYPES_EDITABLE_CANONICAL_ORDER:

        if rng.random() < 0.6:

            if caller == IOC.IMPORT_OPTIONS_CALLER_TYPE_GLOBAL:

                full = random_slice( p = 1.0 )
                full.SetImportOptions( ExternalProgramsImportOptions.ExternalProgramsImportOptions() )

                m.SetDefaultImportOptionsContainerForCallerType( caller, full )

            else:

                m.SetDefaultImportOptionsContainerForCallerType( caller, random_slice() )




    for key in rng.sample( URL_CLASS_KEYS, rng.randint( 0, 3 ) ):

        m.SetDefaultImportOptionsContainerForURLClass( key, random_slice() )


    if rng.random() < 0.3:

        m.AddFavourite( 'my favourite', random_slice() )


    return m


def main():

    cases = []

    for _ in range( 120 ):

        m = random_manager()

        lookups = []

        for _ in range( 6 ):

            caller = rng.choice( [ IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS, IOC.IMPORT_OPTIONS_CALLER_TYPE_SUBSCRIPTION, IOC.IMPORT_OPTIONS_CALLER_TYPE_WATCHER_URLS, IOC.IMPORT_OPTIONS_CALLER_TYPE_LOCAL_IMPORT, IOC.IMPORT_OPTIONS_CALLER_TYPE_LOCAL_IMPORT_FOLDER, IOC.IMPORT_OPTIONS_CALLER_TYPE_CLIENT_API, IOC.IMPORT_OPTIONS_CALLER_TYPE_GLOBAL, IOC.IMPORT_OPTIONS_CALLER_TYPE_URL_CLASS ] )
            specific = random_slice() if rng.random() < 0.5 else ImportOptionsContainer.ImportOptionsContainer()
            url_class_keys = rng.sample( URL_CLASS_KEYS, rng.randint( 0, 3 ) )

            full = m._GenerateFullImportOptionsContainer( specific, caller, url_class_keys )

            lookups.append( {
                'caller': caller,
                'specific': specific.GetSerialisableTuple(),
                'url_class_keys': [ k.hex() for k in url_class_keys ],
                'full': full.GetSerialisableTuple(),
            } )


        cases.append( { 'manager': m.GetSerialisableTuple(), 'lookups': lookups } )


    default_manager = ImportOptionsManager.ImportOptionsManager.STATICGetDefaultInitialisedManager()

    json.dump( { 'default_manager': default_manager.GetSerialisableTuple(), 'cases': cases }, sys.stdout, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
