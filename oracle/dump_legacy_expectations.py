#!/usr/bin/env python3
"""Dump what the reference (v688) sees in a fixture database, for hydrus-legacy's tests.

Opens oracle/fixtures/legacy_db/<name>.tar.gz with plain sqlite and the
reference's own classes (HydrusSerialisable.CreateFromSerialisableTuple,
ClientServices.GenerateService, the YAML options loader, the file storage
classes) and writes a normalised JSON of everything hydrus-legacy must read
the same way:

* every service with its settings as the reference loads them (defaults
  applied), and the reference's default settings per service type
* Client API permissions, tag display settings, favourite searches and the
  client options, including exactly the values /manage_database/get_client_options
  reports, plus tag filter verdicts on sample tags
* the legacy YAML options as the Client API reports them
* the full contents of every primary table (the fixture is small) and the
  classification of every table as primary or derived
* every stored serialised object's identity and a hash of its dump
* the file and thumbnail path of every local file, and path examples at
  other storage granularities
* upgrade vectors: old-version objects and what the reference upgrades them to
* samples of Python's json float and string formatting

Usage: python oracle/dump_legacy_expectations.py [--name basic]
Writes oracle/fixtures/legacy_db/<name>.expected.json.
"""

import argparse
import hashlib
import json
import os
import random
import sqlite3
import struct
import sys
import tarfile
import tempfile

REPO_ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )

sys.path.insert( 0, REPO_ROOT )

os.environ.setdefault( 'QT_QPA_PLATFORM', 'offscreen' )

import yaml

from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusGlobals as HG
from hydrus.core import HydrusSerialisable
from hydrus.core import HydrusTags
from hydrus.core.files import HydrusFilesPhysicalStorage

from hydrus.client import ClientAPI
from hydrus.client import ClientConstants as CC
from hydrus.client import ClientDefaults
from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientLocation
from hydrus.client import ClientOptions
from hydrus.client import ClientServices
from hydrus.client.files import ClientFilesPhysical
from hydrus.client.media import ClientMediaCollect
from hydrus.client.media import ClientMediaSort
from hydrus.client.metadata import ClientTagSorting
from hydrus.client.metadata import ClientTagsHandling
from hydrus.client.search import ClientSearchFavouriteSearches
from hydrus.client.search import ClientSearchFileSearchContext
from hydrus.client.search import ClientSearchTagContext

HERE = os.path.dirname( os.path.abspath( __file__ ) )
FIXTURE_DIR = os.path.join( HERE, 'fixtures', 'legacy_db' )

SCHEMAS = [
    ( 'main', 'client.db' ),
    ( 'external_caches', 'client.caches.db' ),
    ( 'external_mappings', 'client.mappings.db' ),
    ( 'external_master', 'client.master.db' ),
]

# Tables that are derived from primary data (or pure maintenance
# bookkeeping) and which hydrus-legacy deliberately does not read. Everything
# else is primary. Per-service tables are matched by prefix.
DERIVED_MAIN = {
    'service_info', 'shape_vptree', 'shape_maintenance_branch_regen', 'shape_search_cache_numbers',
    'duplicates_files_auto_resolution_rule_count_cache', 'analyze_timestamps', 'vacuum_timestamps',
    'last_shutdown_work_time', 'deferred_delete_tables', 'sqlite_sequence', 'sqlite_stat1',
}
DERIVED_MAIN_PREFIXES = ( 'duplicate_files_auto_resolution_pair_decisions_', 'duplicate_files_auto_resolution_pending_actions_' )
PRIMARY_CACHES = { 'file_maintenance_jobs' }

SAMPLE_TAGS = [ 'safe', 'explicit', 'series:metroid', 'rating:safe', 'character:samus aran', 'blue eyes', '::)', 'meta:highres', 'page:1', 'title:a: b', 'studio:nintendo', 'creator:anonymous artist' ]


def import_reference_modules():
    """Serialisable classes register themselves on import; import them all."""

    import importlib

    skip = { 'hydrus.hydrus_client_boot', 'hydrus.hydrus_server_boot', 'hydrus.client.ClientWindowsIntegration', 'hydrus.client.ClientMacIntegration' }

    for ( root, dirs, files ) in os.walk( os.path.join( REPO_ROOT, 'hydrus' ) ):

        if 'test' in root:

            continue


        for filename in sorted( files ):

            if filename.endswith( '.py' ) and filename != '__init__.py':

                module = os.path.relpath( os.path.join( root, filename ), REPO_ROOT )[ :-3 ].replace( os.sep, '.' )

                if module not in skip:

                    importlib.import_module( module )






class StubServicesManager( object ):

    def FilterValidServiceKeys( self, service_keys ):

        return list( service_keys )


    def ServiceExists( self, service_key ):

        return True



class StubController( object ):
    """Just enough of a controller for objects that subscribe to pubsub or look up services."""

    def __init__( self, db_dir ):

        self.db_dir = db_dir
        self.services_manager = StubServicesManager()
        self.new_options = None


    def GetDBDir( self ):

        return self.db_dir


    def IsBooted( self ):

        # not booted: objects re-serialise without consulting live services
        return False


    def sub( self, *args, **kwargs ):

        pass


    def pub( self, *args, **kwargs ):

        pass



def classify( schema, table ):

    if schema == 'external_caches':

        return 'primary' if table in PRIMARY_CACHES else 'derived'


    if schema == 'main' and ( table in DERIVED_MAIN or table.startswith( DERIVED_MAIN_PREFIXES ) ):

        return 'derived'


    return 'primary'


def plain( value ):
    """Normalise a loaded Python value to JSON: tuples to lists, bytes to hex,
    serialisable objects to their tuples."""

    if isinstance( value, HydrusSerialisable.SerialisableBase ):

        return json.loads( json.dumps( value.GetSerialisableTuple() ) )

    elif isinstance( value, bytes ):

        return value.hex()

    elif isinstance( value, dict ):

        return [ [ plain( k ), plain( v ) ] for ( k, v ) in value.items() ]

    elif isinstance( value, ( list, tuple, set, frozenset ) ):

        return [ plain( v ) for v in value ]


    return value


def sql_value( value ):

    if isinstance( value, bytes ):

        return value.hex()


    return value


def primary_key( db, schema, table ):

    columns = db.execute( f'PRAGMA {schema}.table_info( "{table}" )' ).fetchall()

    pk = [ row[1] for row in sorted( columns, key = lambda r: r[5] ) if row[5] > 0 ]

    return pk if len( pk ) > 0 else [ 'rowid' ]


def dump_tables( db ):

    tables = {}
    classification = {}

    for ( schema, _ ) in SCHEMAS:

        names = [ name for ( name, ) in db.execute( f'SELECT name FROM {schema}.sqlite_master WHERE type = "table" ORDER BY name' ) ]

        for name in names:

            kind = classify( schema, name )

            classification[ f'{schema}.{name}' ] = kind

            if kind != 'primary':

                continue


            order = ', '.join( primary_key( db, schema, name ) )
            cursor = db.execute( f'SELECT * FROM {schema}.{name} ORDER BY {order}' )
            columns = [ d[0] for d in cursor.description ]
            rows = cursor.fetchall()

            if name.startswith( 'json_dumps' ) or name == 'options':

                # contents are checked through the objects section; keep the rest small
                rows = []


            tables[ f'{schema}.{name}' ] = {
                'columns': columns,
                'count': db.execute( f'SELECT count(*) FROM {schema}.{name}' ).fetchone()[0],
                'rows': [ [ sql_value( v ) for v in row ] for row in rows ],
            }



    return ( tables, classification )


def service_settings( service ):

    return { key: plain( value ) for ( key, value ) in service._GetSerialisableDictionary().items() }


def dump_services( db ):

    services = []

    for ( service_id, service_key, service_type, name, dictionary_string ) in db.execute( 'SELECT service_id, service_key, service_type, name, dictionary_string FROM services ORDER BY service_id' ):

        dictionary = HydrusSerialisable.CreateFromString( dictionary_string )

        service = ClientServices.GenerateService( bytes( service_key ), service_type, name, dictionary )

        services.append( {
            'service_id': service_id,
            'key': bytes( service_key ).hex(),
            'type': service_type,
            'name': name,
            'settings': service_settings( service ),
        } )


    return services


def dump_default_service_settings():

    out = {}

    for service_type in ( HC.LOCAL_TAG, HC.LOCAL_FILE_DOMAIN, HC.LOCAL_RATING_LIKE, HC.LOCAL_RATING_NUMERICAL, HC.LOCAL_RATING_INCDEC, HC.CLIENT_API_SERVICE, HC.TAG_REPOSITORY, HC.FILE_REPOSITORY, HC.IPFS ):

        service = ClientServices.GenerateService( b'default', service_type, 'default' )

        settings = service_settings( service )

        # a fresh unknown account has a random key and creation time
        settings.pop( 'account', None )

        out[ str( service_type ) ] = settings


    return out


def tag_filter_verdicts( tag_filter ):

    return { tag: [ tag_filter.TagOK( tag ), tag_filter.TagOK( tag, apply_unnamespaced_rules_to_namespaced_tags = True ) ] for tag in SAMPLE_TAGS }


def dump_tag_filter( tag_filter ):

    return {
        'rules': [ [ s, r ] for ( s, r ) in tag_filter.GetTagSlicesToRules().items() ],
        'allows_everything': tag_filter.AllowsEverything(),
        'verdicts': tag_filter_verdicts( tag_filter ),
    }


def load_dump( db, dump_type ):

    ( version, dump ) = db.execute( 'SELECT version, dump FROM json_dumps WHERE dump_type = ?', ( dump_type, ) ).fetchone()

    return HydrusSerialisable.CreateFromSerialisableTuple( ( dump_type, version, json.loads( dump ) ) )


def dump_client_api( db ):

    manager = load_dump( db, HydrusSerialisable.SERIALISABLE_TYPE_CLIENT_API_MANAGER )

    out = []

    for permissions in manager._access_keys_to_permissions.values():

        out.append( {
            'name': permissions.GetName(),
            'access_key': permissions.GetAccessKey().hex(),
            'permits_everything': permissions.PermitsEverything(),
            'basic_permissions': sorted( permissions._basic_permissions ),
            'search_tag_filter': dump_tag_filter( permissions._search_tag_filter ),
            'has_permission': { str( p ): permissions.HasPermission( p ) for p in ClientAPI.ALLOWED_PERMISSIONS },
        } )


    return out


def dump_location_context( location_context ):

    return {
        'current': sorted( k.hex() for k in location_context.current_service_keys ),
        'deleted': sorted( k.hex() for k in location_context.deleted_service_keys ),
    }


def dump_autocomplete_options( options ):

    return {
        'service_key': options._service_key.hex(),
        'write_autocomplete_tag_domain': options._write_autocomplete_tag_domain.hex(),
        'override_write_autocomplete_location_context': options._override_write_autocomplete_location_context,
        'write_autocomplete_location_context': dump_location_context( options._write_autocomplete_location_context ),
        'search_namespaces_into_full_tags': options._search_namespaces_into_full_tags,
        'unnamespaced_search_gives_any_namespace_wildcards': options._unnamespaced_search_gives_any_namespace_wildcards,
        'namespace_bare_fetch_all_allowed': options._namespace_bare_fetch_all_allowed,
        'namespace_fetch_all_allowed': options._namespace_fetch_all_allowed,
        'fetch_all_allowed': options._fetch_all_allowed,
        'fetch_results_automatically': options._fetch_results_automatically,
        'exact_match_character_threshold': options._exact_match_character_threshold,
    }


def dump_tag_display_manager( manager ):

    return {
        'tag_filters': [ [ display_type, [ [ k.hex(), [ [ s, r ] for ( s, r ) in f.GetTagSlicesToRules().items() ] ] for ( k, f ) in filters.items() ] ] for ( display_type, filters ) in manager._tag_display_types_to_service_keys_to_tag_filters.items() ],
        'autocomplete_options': [ dump_autocomplete_options( o ) for o in manager._tag_service_keys_to_tag_autocomplete_options.values() ],
    }


def dump_tag_context( tag_context ):

    return {
        'service_key': tag_context.service_key.hex(),
        'include_current_tags': tag_context.include_current_tags,
        'include_pending_tags': tag_context.include_pending_tags,
        'display_service_key': tag_context.display_service_key.hex(),
    }


def dump_media_sort( sort ):

    ( metatype, data ) = sort.sort_type

    if metatype == 'namespaces':

        ( namespaces, display_type ) = data
        data = [ list( namespaces ), display_type ]

    elif metatype == 'rating':

        data = data.hex()


    return { 'metatype': metatype, 'data': data, 'order': sort.sort_order, 'tag_context': dump_tag_context( sort.tag_context ) }


def dump_media_collect( collect ):

    return {
        'namespaces': list( collect.namespaces ),
        'rating_service_keys': [ k.hex() for k in collect.rating_service_keys ],
        'collect_unmatched': collect.collect_unmatched,
        'tag_context': dump_tag_context( collect.tag_context ),
    }


def dump_file_search_context( context ):

    return {
        'location_context': dump_location_context( context.GetLocationContext() ),
        'tag_context': dump_tag_context( context.GetTagContext() ),
        'search_type': context._search_type,
        'predicates': [ json.loads( json.dumps( p.GetSerialisableTuple() ) ) for p in context.GetPredicates() ],
        'search_complete': context._search_complete,
    }


def dump_favourite_searches( db ):

    manager = load_dump( db, HydrusSerialisable.SERIALISABLE_TYPE_FAVOURITE_SEARCH_MANAGER )

    out = []

    for ( folder, name, file_search_context, synchronised, media_sort, media_collect ) in manager.GetFavouriteSearchRows():

        out.append( {
            'folder': folder,
            'name': name,
            'file_search_context': dump_file_search_context( file_search_context ),
            'synchronised': synchronised,
            'media_sort': None if media_sort is None else dump_media_sort( media_sort ),
            'media_collect': None if media_collect is None else dump_media_collect( media_collect ),
        } )


    return out


def dump_client_options( db ):

    new_options = load_dump( db, HydrusSerialisable.SERIALISABLE_TYPE_CLIENT_OPTIONS )

    CG.client_controller.new_options = new_options

    # exactly what /manage_database/get_client_options reports in 'options'
    api = {
        'booleans' : new_options.GetAllBooleans(),
        'strings' : new_options.GetAllStrings(),
        'noneable_strings' : new_options.GetAllNoneableStrings(),
        'integers' : new_options.GetAllIntegers(),
        'noneable_integers' : new_options.GetAllNoneableIntegers(),
        'keys' : new_options.GetAllKeysHex(),
        'colors' : new_options.GetAllColours(),
        'media_zooms' : new_options.GetMediaZooms(),
        'slideshow_durations' : new_options.GetSlideshowDurations(),
        'default_namespace_sorts' : [ sort.ToDictForAPI() for sort in new_options.GetDefaultNamespaceSorts() ],
        'default_sort' : new_options.GetDefaultSort().ToDictForAPI(),
        'default_tag_sort' : new_options.GetDefaultTagSort( CC.TAG_PRESENTATION_SEARCH_PAGE ).ToDictForAPI(),
        'default_tag_sort_search_page' : new_options.GetDefaultTagSort( CC.TAG_PRESENTATION_SEARCH_PAGE ).ToDictForAPI(),
        'default_tag_sort_search_page_manage_tags' : new_options.GetDefaultTagSort( CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS ).ToDictForAPI(),
        'default_tag_sort_media_viewer' : new_options.GetDefaultTagSort( CC.TAG_PRESENTATION_MEDIA_VIEWER ).ToDictForAPI(),
        'default_tag_sort_media_vewier_manage_tags' : new_options.GetDefaultTagSort( CC.TAG_PRESENTATION_MEDIA_VIEWER_MANAGE_TAGS ).ToDictForAPI(),
        'fallback_sort' : new_options.GetFallbackSort().ToDictForAPI(),
        'suggested_tags_favourites' : new_options.GetAllSuggestedTagsFavourites(),
        'default_local_location_context' : new_options.GetDefaultLocalLocationContext().ToDictForAPI()
    }

    api = json.loads( json.dumps( api, default = plain ) )

    d = new_options._dictionary

    extra = {
        'floats': dict( d[ 'floats' ] ),
        'string_list': { k: list( v ) for ( k, v ) in d[ 'string_list' ].items() },
        'integer_list': { k: list( v ) for ( k, v ) in d[ 'integer_list' ].items() },
        'key_list': { k: list( v ) for ( k, v ) in d[ 'key_list' ].items() },
        'favourite_tags': new_options.GetStringList( 'favourite_tags' ),
        'favourite_tag_filters': { name: dump_tag_filter( f ) for ( name, f ) in new_options.GetFavouriteTagFilters().items() },
        'default_collect': dump_media_collect( new_options.GetDefaultCollect() ),
        'top_level_keys': list( d.keys() ),
    }

    return ( api, json.loads( json.dumps( extra, default = plain ) ) )


def dump_legacy_options( db ):

    ( text, ) = db.execute( 'SELECT options FROM options' ).fetchone()

    # ClientDB._GetOptions
    options = yaml.safe_load( text )

    default_options = ClientDefaults.GetClientDefaultOptions()

    for key in default_options:

        if key not in options:

            options[ key ] = default_options[ key ]



    # the Client API's filter
    api = { key : value for ( key, value ) in options.items() if key in default_options }

    return json.loads( json.dumps( api, default = plain ) )


def dump_stored_objects( db ):

    out = []

    def record( table, dump_type, name, version, timestamp_ms, dump ):

        if isinstance( dump, bytes ):

            dump = str( dump, 'utf-8' )


        info = json.loads( dump )

        obj_tuple = ( dump_type, version, info ) if name is None else ( dump_type, name, version, info )

        obj = HydrusSerialisable.CreateFromSerialisableTuple( obj_tuple )

        reserialised = json.dumps( obj.GetSerialisableTuple() )

        out.append( {
            'table': table,
            'type': dump_type,
            'name': name,
            'version': version,
            'current_version': HydrusSerialisable.SERIALISABLE_TYPES_TO_OBJECT_TYPES[ dump_type ].SERIALISABLE_VERSION,
            'timestamp_ms': timestamp_ms,
            'dump_sha256': hashlib.sha256( dump.encode( 'utf-8' ) ).hexdigest(),
            'dump_length': len( dump.encode( 'utf-8' ) ),
            'python_round_trip_identical': reserialised == json.dumps( list( obj_tuple ) ),
        } )


    for ( dump_type, version, dump ) in db.execute( 'SELECT dump_type, version, dump FROM json_dumps ORDER BY dump_type' ):

        record( 'json_dumps', dump_type, None, version, None, dump )


    for ( dump_type, name, version, timestamp_ms, dump ) in db.execute( 'SELECT dump_type, dump_name, version, timestamp_ms, dump FROM json_dumps_named ORDER BY rowid' ):

        record( 'json_dumps_named', dump_type, name, version, timestamp_ms, dump )


    for ( dump_type, version, dump ) in db.execute( 'SELECT dump_type, version, dump FROM json_dumps_hashed ORDER BY rowid' ):

        record( 'json_dumps_hashed', dump_type, None, version, None, dump )


    return out


def dump_file_paths( db, db_dir ):

    granularity = db.execute( 'SELECT granularity FROM current_storage_granularity' ).fetchone()[0]

    locations = dict( db.execute( 'SELECT location_id, location FROM current_client_files_locations' ) )
    prefixes_to_locations = dict( db.execute( 'SELECT prefix, location_id FROM client_files_subfolders' ) )

    # hydrus local file storage: every local file, in any local domain
    ( local_storage_id, ) = db.execute( 'SELECT service_id FROM services WHERE service_type = ?', ( HC.HYDRUS_LOCAL_FILE_STORAGE, ) ).fetchone()

    out = []

    for ( hash_id, ) in db.execute( f'SELECT hash_id FROM current_files_{local_storage_id} ORDER BY hash_id' ).fetchall():

        ( hash, ) = db.execute( 'SELECT hash FROM external_master.hashes WHERE hash_id = ?', ( hash_id, ) ).fetchone()
        ( mime, ) = db.execute( 'SELECT mime FROM files_info WHERE hash_id = ?', ( hash_id, ) ).fetchone()

        hash = bytes( hash )

        def path_for( prefix_type, filename ):

            prefix = HydrusFilesPhysicalStorage.GetPrefix( hash, prefix_type, granularity )

            base_location = ClientFilesPhysical.FilesStorageBaseLocation( locations[ prefixes_to_locations[ prefix ] ], 0 )

            subfolder = ClientFilesPhysical.FilesStorageSubfolder( prefix, base_location )

            path = subfolder.GetFilePath( filename )

            return ( os.path.relpath( path, db_dir ), os.path.exists( path ) )


        ( file_path, file_exists ) = path_for( 'f', hash.hex() + HC.mime_ext_lookup[ mime ] )
        ( thumbnail_path, thumbnail_exists ) = path_for( 't', hash.hex() + '.thumbnail' )

        if not file_exists:

            raise Exception( f'{file_path} does not exist in the fixture' )


        out.append( {
            'hash': hash.hex(),
            'mime': mime,
            'file': file_path,
            'thumbnail': thumbnail_path,
            # files without thumbnail support (e.g. zip archives) have none
            'thumbnail_exists': thumbnail_exists,
        } )


    return out


def dump_path_examples():

    base_location = ClientFilesPhysical.FilesStorageBaseLocation( '/base', 0 )

    hash = bytes.fromhex( 'a4d4f5f391c94942e38d89472fed0fc63cf097e3ee05bc40642bcd036407cdaf' )

    out = []

    for granularity in ( 1, 2, 3, 4 ):

        for prefix_type in ( 'f', 't' ):

            prefix = HydrusFilesPhysicalStorage.GetPrefix( hash, prefix_type, granularity )

            subfolder = ClientFilesPhysical.FilesStorageSubfolder( prefix, base_location )

            out.append( {
                'granularity': granularity,
                'kind': prefix_type,
                'hash': hash.hex(),
                'prefix': prefix,
                'relative_dir': os.path.relpath( subfolder.path, '/base' ),
            } )



    return out


def upgrade_vector( old_tuple ):

    obj = HydrusSerialisable.CreateFromSerialisableTuple( old_tuple )

    return { 'old': json.loads( json.dumps( old_tuple ) ), 'current': json.loads( json.dumps( obj.GetSerialisableTuple() ) ) }


def dump_upgrade_vectors():
    """Old-version objects and what the reference's upgrade chains make of them."""

    my_tags = CC.DEFAULT_LOCAL_TAG_SERVICE_KEY.hex()
    my_files = CC.LOCAL_FILE_SERVICE_KEY.hex()
    tag_filter = [ HydrusSerialisable.SERIALISABLE_TYPE_TAG_FILTER, 1, [ [ 'meta:', 1 ], [ 'safe', 0 ] ] ]
    location_context = [ HydrusSerialisable.SERIALISABLE_TYPE_LOCATION_CONTEXT, 1, [ [ my_files ], [] ] ]
    tag_context_v2 = [ HydrusSerialisable.SERIALISABLE_TYPE_TAG_CONTEXT, 2, [ my_tags, True, False, my_tags ] ]

    vectors = {
        'dictionary': [
            upgrade_vector( ( 21, 1, [ [ [ 'a', 1 ], [ 'b', None ] ], [ [ 'filter', tag_filter ] ], [], [] ] ) ),
        ],
        'list': [
            upgrade_vector( ( 26, 1, [ tag_filter ] ) ),
            upgrade_vector( ( 26, 2, [ [ False, 'x' ], [ True, tag_filter ] ] ) ),
        ],
        'tag_context': [
            upgrade_vector( ( 80, 1, [ my_tags, True, False ] ) ),
        ],
        'media_sort': [
            upgrade_vector( ( 49, 1, [ 'namespaces', [ 'series', 'page' ], CC.SORT_DESC ] ) ),
            upgrade_vector( ( 49, 2, [ 'system', CC.SORT_FILES_BY_FILESIZE, CC.SORT_ASC ] ) ),
            upgrade_vector( ( 49, 2, [ 'rating', CC.DEFAULT_FAVOURITES_RATING_SERVICE_KEY.hex(), CC.SORT_DESC ] ) ),
        ],
        'media_collect': [
            upgrade_vector( ( 78, 1, [ [ 'series' ], [ CC.DEFAULT_FAVOURITES_RATING_SERVICE_KEY.hex() ], False ] ) ),
        ],
        'api_permissions': [
            upgrade_vector( ( 76, 'everything', 1, [ '00ff', [ 0, 1, 2, 3, 4, 5, 6, 7, 8, 9 ], tag_filter ] ) ),
            upgrade_vector( ( 76, 'some', 1, [ 'ff00', [ 3, 1 ], tag_filter ] ) ),
        ],
        'tag_autocomplete_options': [
            upgrade_vector( ( 85, 1, [ my_tags, my_tags, True, my_files, True, False, True ] ) ),
            upgrade_vector( ( 85, 2, [ my_tags, my_tags, False, my_files, False, True, False, True ] ) ),
            upgrade_vector( ( 85, 3, [ my_tags, my_tags, True, my_files, True, True, True, False, False, 4 ] ) ),
            upgrade_vector( ( 85, 4, [ my_tags, my_tags, True, location_context, False, False, True, False, True, None ] ) ),
        ],
        'tag_display_manager': [
            upgrade_vector( ( 79, 1, [ [ 2, [ [ my_tags, tag_filter ] ] ] ] ) ),
            upgrade_vector( ( 79, 2, [ [ [ 3, [ [ my_tags, tag_filter ] ] ] ], [ 26, 3, [ [ 2, [ 85, 1, [ my_tags, my_tags, True, my_files, True, False, True ] ] ] ] ] ] ) ),
            upgrade_vector( ( 79, 3, [ [], [ 26, 3, [] ], [ 33, 1, [] ], [ 33, 1, [] ] ] ) ),
        ],
        'file_search_context': [
            upgrade_vector( ( 15, 1, [ my_files, my_tags, False, True, [], True ] ) ),
            upgrade_vector( ( 15, 2, [ my_files, my_tags, 1, False, True, [], False ] ) ),
            upgrade_vector( ( 15, 3, [ my_files, my_tags, 1, False, True, [], False ] ) ),
            upgrade_vector( ( 15, 4, [ my_files, tag_context_v2, 1, [], True ] ) ),
        ],
    }

    return vectors


def dump_python_json_samples():

    rng = random.Random( 688 )

    floats = [ 0.0, -0.0, 1.0, 0.1, 0.5, 1e-05, 0.0001, 1e15, 1e16, 1.5e300, 5e-324, 123.456, 2.0 ** 53, 1 / 3, 2 / 3, 1e22, 1e21, 9007199254740993.0, 100.0, 0.01, 1.7976931348623157e308 ]

    while len( floats ) < 300:

        ( f, ) = struct.unpack( '<d', rng.getrandbits( 64 ).to_bytes( 8, 'little' ) )

        if f == f and f not in ( float( 'inf' ), float( '-inf' ) ):

            floats.append( f )



    for _ in range( 100 ):

        floats.append( rng.uniform( -1000, 1000 ) )
        floats.append( rng.random() * 10 ** rng.randint( -20, 20 ) )


    strings = [ '', 'plain', 'quote " backslash \\ slash /', 'line\nbreak\ttab\r\x08\x0c', '\x00\x01\x1f\x7f', 'é初音ミク', '🙂 emoji', '  ', '﻿ bom' ]

    return {
        'floats': [ [ struct.pack( '<d', f ).hex(), json.dumps( f ) ] for f in floats ],
        'strings': [ [ s, json.dumps( s ) ] for s in strings ],
    }


def main():

    parser = argparse.ArgumentParser()
    parser.add_argument( '--name', default = 'basic' )
    args = parser.parse_args()

    import_reference_modules()

    with tempfile.TemporaryDirectory() as work:

        with tarfile.open( os.path.join( FIXTURE_DIR, args.name + '.tar.gz' ) ) as tar:

            tar.extractall( work, filter = 'data' )


        db_dir = os.path.realpath( work )

        stub = StubController( db_dir )

        HG.controller = stub
        CG.client_controller = stub

        db = sqlite3.connect( f'file:{os.path.join( db_dir, "client.db" )}?mode=ro', uri = True )

        for ( schema, filename ) in SCHEMAS[ 1: ]:

            db.execute( f'ATTACH ? AS {schema}', ( f'file:{os.path.join( db_dir, filename )}?mode=ro', ) )


        ( tables, classification ) = dump_tables( db )

        ( client_options_api, client_options_extra ) = dump_client_options( db )

        expected = {
            'fixture': args.name,
            'version': db.execute( 'SELECT version FROM version' ).fetchone()[0],
            'serialisable_types': [
                [ v, k[ len( 'SERIALISABLE_TYPE_' ): ].lower(), HydrusSerialisable.SERIALISABLE_TYPES_TO_OBJECT_TYPES[ v ].SERIALISABLE_VERSION if v in HydrusSerialisable.SERIALISABLE_TYPES_TO_OBJECT_TYPES else None ]
                for ( k, v ) in sorted( vars( HydrusSerialisable ).items(), key = lambda kv: kv[1] if isinstance( kv[1], int ) else -1 )
                if k.startswith( 'SERIALISABLE_TYPE_' ) and isinstance( v, int )
            ],
            'services': dump_services( db ),
            'default_service_settings': dump_default_service_settings(),
            'client_api_permissions': dump_client_api( db ),
            'tag_display_manager': dump_tag_display_manager( load_dump( db, HydrusSerialisable.SERIALISABLE_TYPE_TAG_DISPLAY_MANAGER ) ),
            'favourite_searches': dump_favourite_searches( db ),
            'client_options_api': client_options_api,
            'client_options_extra': client_options_extra,
            'legacy_options_api': dump_legacy_options( db ),
            'stored_objects': dump_stored_objects( db ),
            'table_classification': classification,
            'tables': tables,
            'file_paths': dump_file_paths( db, db_dir ),
            'path_examples': dump_path_examples(),
            'upgrade_vectors': dump_upgrade_vectors(),
            'python_json': dump_python_json_samples(),
        }

        db.close()


    out_path = os.path.join( FIXTURE_DIR, args.name + '.expected.json' )

    with open( out_path, 'w' ) as f:

        json.dump( expected, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {out_path} ({os.path.getsize( out_path )} bytes)' )


if __name__ == '__main__':

    main()
