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
from hydrus.client.importing.options import CheckerImportOptions
from hydrus.client.importing.options import ImportOptionsContainer
from hydrus.client.importing.options import TagImportOptions
from hydrus.client.metadata import ClientTags


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


    checkers = [ checker_case() for _ in range( 200 ) ]

    json.dump( {
        'file_seeds' : file_seeds,
        'gallery_seeds' : gallery_seeds,
        'query_logs' : query_logs,
        'subscriptions' : subscriptions,
        'checkers' : checkers,
    }, sys.stdout, indent = 1, ensure_ascii = False, sort_keys = True )

    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
