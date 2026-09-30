#!/usr/bin/env python3
"""Record how the reference reads subscriptions and their queries' history.

Random subscriptions (type 88) with query headers (87), and random query log
containers (86) holding gallery logs and file seed caches, built with the
reference's classes. Seeds are also saved in every older version the
reference can still load (by laying the fields out as that version did), so
each case keeps:

* `stored`: the serialised tuple as a database might hold it
* `expected`: what the reference re-serialises after loading it (the latest
  version, upgrades applied)
* `facts`: the loaded object's fields, read with the reference's own
  attributes, to check field positions against

Subscriptions are also saved in their old versions (1-3), with old-style
file, tag and note import options in each version the reference upgrades
from, to record what the reference converts them to.

Also records checker timing (next check time, death) for random file
histories.

Usage: python oracle/dump_subscriptions.py > oracle/fixtures/subscriptions.json
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
from hydrus.core import HydrusSerialisable
from hydrus.core import HydrusTags
from hydrus.core import HydrusTime
from hydrus.client import ClientConstants as CC
from hydrus.client import ClientGlobals as CG
from hydrus.client.importing import ClientImportFileSeeds
from hydrus.client.importing import ClientImportGallerySeeds
from hydrus.client.importing import ClientImportSubscriptionQuery
from hydrus.client.importing import ClientImportSubscriptions
from hydrus.client import ClientLocation
from hydrus.client.importing.options import CheckerImportOptions
from hydrus.client.importing.options import FileFilteringImportOptions
from hydrus.client.importing.options import FileImportOptionsLegacy
from hydrus.client.importing.options import ImportOptionsContainer
from hydrus.client.importing.options import LocationImportOptions
from hydrus.client.importing.options import NoteImportOptions
from hydrus.client.importing.options import NoteImportOptionsLegacy
from hydrus.client.importing.options import PrefetchImportOptions
from hydrus.client.importing.options import PresentationImportOptions
from hydrus.client.importing.options import TagFilteringImportOptions
from hydrus.client.importing.options import TagImportOptions
from hydrus.client.importing.options import TagImportOptionsLegacy
from hydrus.client.metadata import ClientTags
from hydrus.client.search import ClientSearchPredicate


def cannot_normalise( url, for_server = False ):

    raise HydrusExceptions.URLClassException( 'no url classes here' )


CG.client_controller.IsBooted = lambda: False
CG.client_controller.pub = lambda *args, **kwargs: None
CG.client_controller.network_engine = types.SimpleNamespace( domain_manager = types.SimpleNamespace( NormaliseURL = cannot_normalise ) )
HC.options = { 'gallery_file_limit' : None }

rng = random.Random( 88 )

TAG_SERVICE_KEYS = [ CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY, bytes.fromhex( 'ab' * 32 ) ]
TAGS = [ 'blue eyes', 'solo', 'series:metroid', 'creator:someone', 'éclair', 'キャラクター:初音ミク', 'meta:highres' ]
STATUSES = [ CC.STATUS_UNKNOWN, CC.STATUS_SUCCESSFUL_AND_NEW, CC.STATUS_SUCCESSFUL_BUT_REDUNDANT, CC.STATUS_DELETED, CC.STATUS_ERROR, CC.STATUS_VETOED, CC.STATUS_SKIPPED, CC.STATUS_SUCCESSFUL_AND_CHILD_FILES ]


def maybe( value ):

    return value if rng.random() < 0.5 else None


def some( items, most = 3 ):

    return rng.sample( items, rng.randint( 0, min( most, len( items ) ) ) )


def url( i ):

    return rng.choice( [
        f'https://safebooru.org/index.php?page=post&s=view&id={i}',
        f'https://example.com/files/{i}.jpg',
        f'https://gelbooru.com/index.php?id={i}&page=post&s=view',
        f'http://ünïcode.example/{i}?q=a b',
    ] )


def when():

    return rng.randint( 1_500_000_000, 1_700_000_000 )


def service_keys_to_tags():

    return ClientTags.ServiceKeysToTags( { key : set( some( TAGS ) ) for key in some( TAG_SERVICE_KEYS, 2 ) } )


def file_seed( i, in_cache = False ):

    if rng.random() < 0.8:

        f = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, url( i ) )
        # (a cache cannot hold a seed that failed to normalise)
        f.file_seed_data_for_comparison = rng.choice( [ f.file_seed_data, f.file_seed_data + '&normalised' ] + ( [] if in_cache else [ None ] ) )

    else:

        f = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_HDD, f'/home/someone/import/{i}.png' )


    f.created = when()
    f.modified = f.created + rng.randint( 0, 5000 )
    f.source_time = maybe( f.created - rng.randint( 0, 10 ** 6 ) )
    f.status = rng.choice( STATUSES )
    f.note = rng.choice( [ '', 'Found 3 new URLs.', 'Error: 404\n\nsome traceback' ] )
    f._referral_url = maybe( url( i + 1000 ) )
    f._request_headers = dict( some( [ ( 'Referer', 'https://a.example/' ), ( 'X-Thing', 'yes' ) ] ) )
    f._external_filterable_tags = set( some( TAGS ) )
    f._external_additional_service_keys_to_tags = service_keys_to_tags()
    f._primary_urls = set( url( i + k ) for k in range( rng.randint( 0, 2 ) ) )
    f._source_urls = set( url( i + 100 + k ) for k in range( rng.randint( 0, 2 ) ) )
    f._tags = set( some( TAGS ) )
    f._names_and_notes_dict = dict( some( [ ( 'comment', 'nice' ), ( 'artist note', 'multi\nline' ) ] ) )
    f._hashes = { hash_type : bytes( rng.randrange( 256 ) for _ in range( size ) ) for ( hash_type, size ) in some( [ ( 'sha256', 32 ), ( 'md5', 16 ), ( 'sha1', 20 ) ] ) }

    return f


def gallery_seed( i ):

    g = ClientImportGallerySeeds.GallerySeed( f'https://safebooru.org/index.php?page=post&s=list&tags=blue_eyes&pid={i * 40}', can_generate_more_pages = rng.random() < 0.5 )

    g.created = when()
    g.modified = g.created + rng.randint( 0, 5000 )
    g.status = rng.choice( STATUSES )
    g.note = rng.choice( [ '', 'Found 40 new URLs.' ] )
    g._referral_url = maybe( url( i ) )
    g._request_headers = dict( some( [ ( 'Referer', 'https://a.example/' ) ] ) )
    g._external_filterable_tags = set( some( TAGS ) )
    g._external_additional_service_keys_to_tags = service_keys_to_tags()

    return g


# field layouts of older versions, as each _UpdateSerialisableInfo expects them

FILE_SEED_FIELDS = [ 'type', 'data', 'comparison', 'created', 'modified', 'source_time', 'status', 'note', 'referral', 'headers', 'filterable', 'additional', 'primary', 'source', 'tags', 'notes', 'hashes' ]

FILE_SEED_ADDED = { 8 : 'comparison', 7 : 'headers', 6 : 'notes', 5 : 'source', 4 : 'filterable', 3 : 'additional', 2 : 'referral' }

GALLERY_SEED_FIELDS = [ 'url', 'can_generate_more_pages', 'filterable', 'additional', 'created', 'modified', 'status', 'note', 'referral', 'headers' ]

GALLERY_SEED_ADDED = { 4 : 'headers', 3 : 'filterable', 2 : 'additional' }


def downgrade( info, fields, added, version, latest ):

    values = dict( zip( fields, info ) )

    for v in range( latest, version, -1 ):

        del values[ added[ v ] ]


    if 'source' not in values and 'primary' in values:

        values[ 'urls' ] = values.pop( 'primary' ) # before version 5, 'urls'

        fields = [ 'urls' if name == 'primary' else name for name in fields ]


    return [ values[ name ] for name in fields if name in values ]


def stored_at( obj, version, fields, added ):

    ( kind, latest, info ) = obj.GetSerialisableTuple()

    if version == latest:

        return [ kind, latest, info ]


    return [ kind, version, downgrade( info, fields, added, version, latest ) ]


def sorted_tags( service_keys_to_tags ):

    return { key.hex() : sorted( tags ) for ( key, tags ) in service_keys_to_tags.items() }


def file_seed_facts( f ):

    return {
        'type' : f.file_seed_type,
        'data' : f.file_seed_data,
        'comparison' : f.file_seed_data_for_comparison,
        'created' : f.created,
        'modified' : f.modified,
        'source_time' : f.source_time,
        'status' : f.status,
        'note' : f.note,
        'referral' : f._referral_url,
        'headers' : sorted( f._request_headers.items() ),
        'filterable' : sorted( f._external_filterable_tags ),
        'additional' : sorted_tags( f._external_additional_service_keys_to_tags ),
        'primary' : sorted( f._primary_urls ),
        'source' : sorted( f._source_urls ),
        'tags' : sorted( f._tags ),
        'notes' : sorted( f._names_and_notes_dict.items() ),
        'hashes' : sorted( ( t, h.hex() ) for ( t, h ) in f._hashes.items() if h is not None ),
    }


def gallery_seed_facts( g ):

    return {
        'url' : g.url,
        'can_generate_more_pages' : g._can_generate_more_pages,
        'created' : g.created,
        'modified' : g.modified,
        'status' : g.status,
        'note' : g.note,
        'referral' : g._referral_url,
        'headers' : sorted( g._request_headers.items() ),
        'filterable' : sorted( g._external_filterable_tags ),
        'additional' : sorted_tags( g._external_additional_service_keys_to_tags ),
    }


def read_back( stored ):

    return HydrusSerialisable.CreateFromSerialisableTuple( tuple( stored ) )


def seed_cases( make, fields, added, facts, latest, count ):

    cases = []

    for i in range( count ):

        version = rng.randint( 1, latest )
        stored = stored_at( make( i ), version, fields, added )
        loaded = read_back( json.loads( json.dumps( stored ) ) )

        cases.append( { 'stored' : stored, 'expected' : list( loaded.GetSerialisableTuple() ), 'facts' : facts( loaded ) } )


    return cases


def query_log( i ):

    name = ClientImportSubscriptionQuery.GenerateQueryLogContainerName()

    container = ClientImportSubscriptionQuery.SubscriptionQueryLogContainer( name )

    gallery_seed_log = ClientImportGallerySeeds.GallerySeedLog()
    gallery_seed_log.AddGallerySeeds( [ gallery_seed( i * 10 + k ) for k in range( rng.randint( 0, 3 ) ) ] )

    file_seed_cache = ClientImportFileSeeds.FileSeedCache()
    file_seed_cache.AddFileSeeds( [ file_seed( i * 10 + k, in_cache = True ) for k in range( rng.randint( 0, 5 ) ) ] )

    container.SetGallerySeedLog( gallery_seed_log )
    container.SetFileSeedCache( file_seed_cache )

    return container


def tag_import_options():

    o = TagImportOptions.TagImportOptions()

    if rng.random() < 0.5:

        o = HydrusSerialisable.CreateFromSerialisableTuple( ( HydrusSerialisable.SERIALISABLE_TYPE_TAG_IMPORT_OPTIONS, 1, [ [ CC.DEFAULT_LOCAL_TAG_SERVICE_KEY.hex(), TagImportOptions.ServiceTagImportOptions( get_tags = True, additional_tags = some( TAGS ) ).GetSerialisableTuple() ] ] ) )


    return o


def query_header( i ):

    h = ClientImportSubscriptionQuery.SubscriptionQueryHeader()

    h.SetQueryText( rng.choice( [ 'blue_eyes', 'samus_aran solo', 'creator:someone', 'キャラクター' ] ) + str( i ) )
    h._display_name = maybe( f'display {i}' )
    h._check_now = rng.random() < 0.3
    h._last_check_time = rng.choice( [ 0, when() ] )
    h._next_check_time = rng.choice( [ 0, when() ] )
    h._paused = rng.random() < 0.3
    h._checker_status = rng.choice( [ 0, 1 ] )
    h._file_seed_cache_compaction_number = rng.choice( [ 250, 1000 ] )
    h._gallery_seed_log_compaction_number = rng.choice( [ 100, 30 ] )
    h._tag_import_options = tag_import_options()

    return h


def header_facts( h ):

    return {
        'log_name' : h.GetQueryLogContainerName(),
        'query_text' : h.GetQueryText(),
        'display_name' : h._display_name,
        'check_now' : h._check_now,
        'last_check_time' : h._last_check_time,
        'next_check_time' : h._next_check_time,
        'paused' : h._paused,
        'checker_status' : h._checker_status,
        'file_seed_compaction_number' : h._file_seed_cache_compaction_number,
        'gallery_seed_compaction_number' : h._gallery_seed_log_compaction_number,
        'tag_import_options' : list( h._tag_import_options.GetSerialisableTuple() ),
    }


def checker_options():

    faster = rng.choice( [ 300, 3600, 86400 ] )
    slower = rng.choice( [ faster, faster * 4, 86400 * 7 ] )

    return CheckerImportOptions.CheckerOptions(
        intended_files_per_check = rng.choice( [ 8, 4, 2.5, 1 ] ),
        never_faster_than = faster,
        never_slower_than = max( faster, slower ),
        death_file_velocity = ( rng.choice( [ 0, 1, 3 ] ), rng.choice( [ 86400, 86400 * 30, 86400 * 180 ] ) )
    )


def subscription( i ):

    s = ClientImportSubscriptions.Subscription( f'subscription {i}', ( bytes( rng.randrange( 256 ) for _ in range( 32 ) ), rng.choice( [ 'safebooru tag search', 'gelbooru tag search', '' ] ) ) )

    s._query_headers = [ query_header( i * 10 + k ) for k in range( rng.randint( 0, 3 ) ) ]

    logs = []

    for ( k, h ) in enumerate( s._query_headers ):

        container = query_log( i * 10 + k )
        h.SetQueryLogContainerName( container.GetName() )

        logs.append( container )

    s._checker_options = checker_options()
    s._initial_file_limit = rng.choice( [ 100, 500, None ] )
    s._periodic_file_limit = rng.choice( [ 100, 25, None ] )
    s._this_is_a_random_sample_sub = rng.random() < 0.3
    s._paused = rng.random() < 0.3
    s._no_work_until = rng.choice( [ 0, when() ] )
    s._no_work_until_reason = rng.choice( [ '', 'network error: timeout' ] )
    s._show_a_popup_while_working = rng.random() < 0.5
    s._publish_files_to_popup_button = rng.random() < 0.5
    s._publish_files_to_page = rng.random() < 0.5
    s._publish_label_override = maybe( 'my label' )
    s._merge_query_publish_events = rng.random() < 0.5

    return ( s, logs )


def subscription_facts( s ):

    ( gug_key, gug_name ) = s._gug_key_and_name

    return {
        'name' : s.GetName(),
        'gug_key' : gug_key.hex(),
        'gug_name' : gug_name,
        'queries' : [ header_facts( h ) for h in s._query_headers ],
        'checker' : list( s._checker_options.ToTuple() ),
        'initial_file_limit' : s._initial_file_limit,
        'periodic_file_limit' : s._periodic_file_limit,
        'this_is_a_random_sample' : s._this_is_a_random_sample_sub,
        'paused' : s._paused,
        'no_work_until' : s._no_work_until,
        'no_work_until_reason' : s._no_work_until_reason,
        'presentation' : list( s.GetPresentationOptions() ),
    }


def checker_case():

    options = checker_options()

    now = when()

    cache = ClientImportFileSeeds.FileSeedCache()

    seeds = []

    for k in range( rng.choice( [ 0, 1, 5, 30 ] ) ):

        f = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, f'https://example.com/{k}' )
        f.created = now - rng.randint( 0, 86400 * 60 )
        f.source_time = maybe( f.created - rng.randint( -100, 86400 * 3 ) )

        seeds.append( [ f.source_time, f.created ] )
        cache.AddFileSeeds( [ f ] )


    last_check_time = rng.choice( [ 0, now, now - rng.randint( 0, 86400 * 30 ) ] )

    HydrusTime.GetNow = lambda: now

    return {
        'options' : list( options.ToTuple() ),
        'seeds' : seeds,
        'last_check_time' : last_check_time,
        'now' : now,
        'next_check_time' : options.GetNextCheckTime( cache, last_check_time ),
        'is_dead' : options.IsDead( cache, last_check_time ),
        'death_file_velocity_period' : options.GetDeathFileVelocityPeriod(),
    }


def log_case( container ):

    stored = list( container.GetSerialisableTuple() )
    loaded = read_back( stored )

    return {
        'stored' : stored,
        'facts' : {
            'name' : loaded.GetName(),
            'gallery_seeds' : [ gallery_seed_facts( g ) for g in loaded.GetGallerySeedLog().GetGallerySeeds() ],
            'file_seeds' : [ file_seed_facts( f ) for f in loaded.GetFileSeedCache().GetFileSeeds() ],
        },
    }


# old-style import options ------------------------------------------------

FILE_SERVICE_KEYS = [ CC.LOCAL_FILE_SERVICE_KEY, bytes.fromhex( 'cd' * 32 ) ]
MIMES = [ HC.IMAGE_JPEG, HC.IMAGE_PNG, HC.ANIMATION_GIF, HC.VIDEO_MP4, HC.APPLICATION_PDF, HC.APPLICATION_ZIP, HC.APPLICATION_PSD, HC.GENERAL_APPLICATION, HC.GENERAL_IMAGE, HC.GENERAL_VIDEO, HC.GENERAL_ANIMATION, HC.GENERAL_AUDIO ]


def tag_filter():

    f = HydrusTags.TagFilter()

    for _ in range( rng.randint( 0, 2 ) ):

        f.SetRule( rng.choice( [ '', ':', 'creator:', 'goblin' ] ), rng.choice( [ HC.FILTER_WHITELIST, HC.FILTER_BLACKLIST ] ) )


    return f


def legacy_file_options():

    o = FileImportOptionsLegacy.FileImportOptionsLegacy()

    p = PrefetchImportOptions.PrefetchImportOptions()
    p.SetPreImportHashCheckType( rng.randint( 0, 2 ) )
    p.SetPreImportURLCheckType( rng.randint( 0, 2 ) )
    p.SetPreImportURLCheckLooksForNeighbourSpam( rng.random() < 0.5 )
    o.SetPrefetchImportOptions( p )

    f = FileFilteringImportOptions.FileFilteringImportOptions()
    f.SetExcludesDeleted( rng.random() < 0.5 )
    f.SetAllowsDecompressionBombs( rng.random() < 0.5 )

    if rng.random() < 0.7:

        f.SetAllowedSpecificFiletypes( ClientSearchPredicate.ConvertSummaryFiletypesToSpecific( rng.sample( MIMES, rng.randint( 1, 5 ) ), only_searchable = False ) )


    f.SetMinSize( maybe( rng.randint( 1, 1000 ) ) )
    f.SetMaxSize( maybe( rng.randint( 1000, 10 ** 9 ) ) )
    f.SetMaxGifSize( maybe( rng.randint( 1000, 10 ** 8 ) ) )
    f.SetMinResolution( maybe( ( rng.randint( 1, 100 ), rng.randint( 1, 100 ) ) ) )
    f.SetMaxResolution( maybe( ( rng.randint( 100, 10000 ), rng.randint( 100, 10000 ) ) ) )
    o.SetFileFilteringImportOptions( f )

    l = LocationImportOptions.LocationImportOptions()
    l.SetDestinationLocationContext( ClientLocation.LocationContext( current_service_keys = rng.sample( FILE_SERVICE_KEYS, rng.randint( 1, 2 ) ) ) )
    l.SetAutomaticallyArchives( rng.random() < 0.5 )
    l.SetShouldAssociatePrimaryURLs( rng.random() < 0.5 )
    l.SetShouldAssociateSourceURLs( rng.random() < 0.5 )
    l.SetDoAutomaticArchiveOnAlreadyInDBFiles( rng.random() < 0.5 )
    l.SetDoImportDestinationsOnAlreadyInDBFiles( rng.random() < 0.5 )
    o.SetLocationImportOptions( l )

    pr = PresentationImportOptions.PresentationImportOptions()
    pr.SetPresentationStatus( rng.randint( 0, 2 ) )
    pr.SetPresentationInbox( rng.randint( 0, 2 ) )
    pr.SetLocationContext( ClientLocation.LocationContext.STATICCreateSimple( rng.choice( [ CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY ] ) ) )
    o.SetPresentationImportOptions( pr )

    o.SetIsDefault( rng.random() < 0.3 )

    return o


def old_predicate( predicate_tuple ):
    """A filetype predicate as an older version might have stored it."""

    ( kind, version, [ predicate_type, value, inclusive ] ) = predicate_tuple

    old_version = rng.choice( [ version, 6, 4 ] )

    if old_version == 4:

        value = sorted( ClientSearchPredicate.ConvertSummaryFiletypesToSpecific( value, only_searchable = False ) )


    return [ kind, old_version, [ predicate_type, list( value ), inclusive ] ]


def old_presentation( presentation_tuple ):

    ( kind, version, [ location, status, inbox ] ) = presentation_tuple

    if rng.random() < 0.5:

        key = bytes.fromhex( location[ 2 ][ 0 ][ 0 ] ) if location[ 2 ][ 0 ] else None

        code = 1 if key == CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY else 0

        return [ kind, 1, [ code, status, inbox ] ]


    return [ kind, version, [ location, status, inbox ] ]


def legacy_file_options_at( o, version ):

    ( kind, latest, info ) = o.GetSerialisableTuple()

    ( prefetch, filtering, locations, presentation, is_default ) = info

    presentation = old_presentation( presentation )

    if version == 15:

        return [ kind, 15, [ prefetch, filtering, locations, presentation, is_default ] ]


    ( _, _, [ destination, archive, primary, source, archive_already, destinations_already ] ) = locations

    post = [ archive, primary, source, archive_already, destinations_already ]

    if version == 14:

        return [ kind, 14, [ prefetch, filtering, destination, post, presentation, is_default ] ]


    ( _, _, [ exclude_deleted, bombs, predicate, min_size, max_size, max_gif_size, min_res, max_res ] ) = filtering

    predicate = old_predicate( predicate )

    if version == 13:

        return [ kind, 13, [ prefetch, [ exclude_deleted, bombs, predicate, min_size, max_size, max_gif_size, min_res, max_res, destination ], post, presentation, is_default ] ]


    ( _, _, prefetch_info ) = prefetch

    ( hash_check, url_check, spam ) = prefetch_info[ :3 ]

    if version == 11:

        post = post[ :4 ]

    elif version <= 10:

        post = post[ :3 ]


    if version >= 9:

        pre = [ exclude_deleted, hash_check, url_check, spam, bombs, predicate, min_size, max_size, max_gif_size, min_res, max_res, destination ]

    else:

        pre = [ exclude_deleted, url_check == PrefetchImportOptions.DO_NOT_CHECK, hash_check == PrefetchImportOptions.DO_NOT_CHECK, bombs, predicate, min_size, max_size, max_gif_size, min_res, max_res, destination ]


    return [ kind, version, [ pre, post, presentation, is_default ] ]


def legacy_tag_options():

    services = {}

    for key in some( TAG_SERVICE_KEYS, 2 ):

        services[ key ] = TagImportOptions.ServiceTagImportOptions(
            get_tags = rng.random() < 0.5,
            get_tags_filter = tag_filter(),
            additional_tags = some( TAGS ),
            to_new_files = rng.random() < 0.5,
            to_already_in_inbox = rng.random() < 0.5,
            to_already_in_archive = rng.random() < 0.5,
            only_add_existing_tags = rng.random() < 0.5,
            only_add_existing_tags_filter = tag_filter(),
            get_tags_overwrite_deleted = rng.random() < 0.2,
            additional_tags_overwrite_deleted = rng.random() < 0.2
        )


    return TagImportOptionsLegacy.TagImportOptionsLegacy(
        fetch_tags_even_if_url_recognised_and_file_already_in_db = rng.random() < 0.5,
        fetch_tags_even_if_hash_recognised_and_file_already_in_db = rng.random() < 0.5,
        tag_filtering_import_options = TagFilteringImportOptions.TagFilteringImportOptions( tag_blacklist = tag_filter(), tag_whitelist = some( [ 'blue eyes', 'solo' ] ) ),
        tag_import_options = TagImportOptions.TagImportOptions( service_keys_to_service_tag_import_options = services ),
        is_default = rng.random() < 0.3
    )


def old_service_tag_options( stio_tuple ):

    ( kind, version, info ) = stio_tuple

    if rng.random() < 0.5 and not info[ 8 ] and not info[ 9 ]:

        return [ kind, 3, info[ :8 ] ]


    return [ kind, version, info ]


def legacy_tag_options_at( o, version ):

    ( kind, latest, info ) = o.GetSerialisableTuple()

    if version == 9:

        return [ kind, 9, info ]


    ( fetch_url, fetch_hash, filtering, tags, is_default ) = info

    ( _, _, [ blacklist, whitelist ] ) = filtering
    ( _, _, services ) = tags

    services = [ [ key, old_service_tag_options( stio ) ] for ( key, stio ) in services ]

    if version == 8:

        return [ kind, 8, [ fetch_url, fetch_hash, blacklist, whitelist, services, is_default ] ]

    elif version == 7:

        return [ kind, 7, [ fetch_url, fetch_hash, blacklist, services, is_default ] ]

    else:

        return [ kind, 6, [ fetch_url, fetch_hash, blacklist, services ] ]



def legacy_note_options():

    n = NoteImportOptions.NoteImportOptions()
    n.SetGetNotes( rng.random() < 0.5 )
    n.SetExtendExistingNoteIfPossible( rng.random() < 0.5 )
    n.SetConflictResolution( rng.randint( 0, 3 ) )
    n.SetNameWhitelist( some( [ 'comment', 'translation' ] ) )
    n.SetAllNameOverride( maybe( 'booru note' ) )
    n.SetNamesToNameOverrides( dict( some( [ ( 'comment', 'booru comment' ) ] ) ) )

    o = NoteImportOptionsLegacy.NoteImportOptionsLegacy()
    o.SetNoteImportOptions( n )
    o.SetIsDefault( rng.random() < 0.3 )

    return o


def legacy_note_options_at( o, version ):

    ( kind, latest, info ) = o.GetSerialisableTuple()

    if version == 2:

        return [ kind, 2, info ]


    ( ( _, _, note_info ), is_default ) = info

    return [ kind, 1, list( note_info ) + [ is_default ] ]


def old_subscription( i ):

    ( s, logs ) = subscription( 100 + i )

    ( kind, name, latest, info ) = s.GetSerialisableTuple()

    version = rng.randint( 1, 3 )

    headers = []

    for header in info[ 1 ]:

        ( h_kind, h_version, h_info ) = header

        if rng.random() < 0.5:

            tio = legacy_tag_options_at( legacy_tag_options(), rng.choice( [ 6, 7, 8, 9 ] ) )

            h_info = list( h_info )
            h_info[ 12 ] = tio

            if rng.random() < 0.5:

                header = [ h_kind, 2, h_info ]

            else:

                header = [ h_kind, 1, h_info[ :10 ] + h_info[ 12: ] ]



        headers.append( header )


    ( gug, _, checker, initial, periodic, random_sample, paused, container, *rest ) = info

    fio = legacy_file_options_at( legacy_file_options(), rng.randint( 8, 15 ) )
    tio = legacy_tag_options_at( legacy_tag_options(), rng.choice( [ 6, 7, 8, 9 ] ) )
    nio = legacy_note_options_at( legacy_note_options(), rng.randint( 1, 2 ) )

    if version == 3:

        old_info = [ gug, headers, checker, initial, periodic, random_sample, paused, fio, tio, nio, *rest ]

    elif version == 2:

        old_info = [ gug, headers, checker, initial, periodic, random_sample, paused, fio, tio, *rest ]

    else:

        old_info = [ gug, headers, checker, initial, periodic, paused, fio, tio, *rest ]


    return [ kind, name, version, old_info ]


def old_subscription_case( i ):

    stored = old_subscription( i )

    loaded = read_back( json.loads( json.dumps( stored ) ) )

    return {
        'stored' : stored,
        'facts' : subscription_facts( loaded ),
        'import_options' : list( loaded._import_options_container.GetSerialisableTuple() ),
    }


def compaction_case():

    now = when()

    seeds = []

    for k in range( rng.choice( [ 0, 3, 10, 40 ] ) ):

        f = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, f'https://example.com/post/{k}' )
        f.created = now - rng.randint( 0, 86400 * 20 )
        f.source_time = maybe( f.created - rng.randint( -100, 86400 * 5 ) )
        f.status = rng.choice( [ CC.STATUS_UNKNOWN, CC.STATUS_SUCCESSFUL_AND_NEW, CC.STATUS_SUCCESSFUL_BUT_REDUNDANT, CC.STATUS_SUCCESSFUL_AND_CHILD_FILES, CC.STATUS_SUCCESSFUL_AND_CHILD_FILES, CC.STATUS_VETOED ] )

        if f.status == CC.STATUS_SUCCESSFUL_AND_CHILD_FILES:

            f.note = rng.choice( [ 'Found 2 new URLs.', 'Found 0 new URLs.', 'Found 1 new URLs in 2 sub-posts.', 'Found 12 new URLs.', 'something else', '' ] )

        seeds.append( f )


    cache = ClientImportFileSeeds.FileSeedCache()
    cache.AddFileSeeds( seeds )

    keep = rng.choice( [ 0, 1, 3, 5, 250 ] )
    before = now - rng.choice( [ 0, 86400, 86400 * 5, 86400 * 30 ] )

    master = cache.GetApproxNumMasterFileSeeds()
    can_compact = cache.CanCompact( keep, before )

    if can_compact:

        cache.Compact( keep, before )


    remaining = { id( f ) for f in cache.GetFileSeeds() }

    gallery_seeds = []

    for k in range( rng.choice( [ 0, 3, 12 ] ) ):

        g = ClientImportGallerySeeds.GallerySeed( f'https://example.com/gallery?page={k}' )
        g.created = now - rng.randint( 0, 86400 * 20 )
        g.status = rng.choice( [ CC.STATUS_UNKNOWN, CC.STATUS_SUCCESSFUL_AND_NEW, CC.STATUS_VETOED ] )

        gallery_seeds.append( g )


    log = ClientImportGallerySeeds.GallerySeedLog()
    log.AddGallerySeeds( gallery_seeds )

    gallery_keep = rng.choice( [ 0, 1, 5, 100 ] )

    if log.CanCompact( gallery_keep, before ):

        log.Compact( gallery_keep, before )


    gallery_remaining = { id( g ) for g in log.GetGallerySeeds() }

    return {
        'files' : [ [ f.status, f.note, f.source_time, f.created ] for f in seeds ],
        'keep' : keep,
        'before' : before,
        'master' : master,
        'removed' : [ i for ( i, f ) in enumerate( seeds ) if id( f ) not in remaining ],
        'galleries' : [ [ g.status, g.created ] for g in gallery_seeds ],
        'gallery_keep' : gallery_keep,
        'gallery_removed' : [ i for ( i, g ) in enumerate( gallery_seeds ) if id( g ) not in gallery_remaining ],
    }


def main():

    file_seeds = seed_cases( file_seed, FILE_SEED_FIELDS, FILE_SEED_ADDED, file_seed_facts, 8, 80 )
    gallery_seeds = seed_cases( gallery_seed, GALLERY_SEED_FIELDS, GALLERY_SEED_ADDED, gallery_seed_facts, 4, 30 )

    query_logs = [ log_case( query_log( i ) ) for i in range( 8 ) ]

    subscriptions = []

    for i in range( 12 ):

        ( s, logs ) = subscription( i )

        stored = list( s.GetSerialisableTuple() )
        loaded = read_back( stored )

        subscriptions.append( { 'stored' : stored, 'facts' : subscription_facts( loaded ), 'logs' : [ log_case( container ) for container in logs ] } )


    old_subscriptions = [ old_subscription_case( i ) for i in range( 60 ) ]

    checkers = [ checker_case() for _ in range( 200 ) ]

    compactions = [ compaction_case() for _ in range( 200 ) ]

    json.dump( {
        'file_seeds' : file_seeds,
        'gallery_seeds' : gallery_seeds,
        'query_logs' : query_logs,
        'subscriptions' : subscriptions,
        'old_subscriptions' : old_subscriptions,
        'checkers' : checkers,
        'compactions' : compactions,
    }, sys.stdout, indent = 1, ensure_ascii = False, sort_keys = True )

    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
