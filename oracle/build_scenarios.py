#!/usr/bin/env python3
"""Generate Client API conformance scenarios.

A scenario is a list of HTTP requests against a fixture database. The
reference client's responses are recorded by `record_api.py`; the Rust server
replays the same requests and must match. Scenarios are plain JSON so both
sides read the same file.

Step format:
    {
      "method": "GET" | "POST",
      "path": "/get_files/search_files",
      "query": [ [ "tags", "[\\"blue eyes\\"]" ], ... ],   # exact query params, already strings
      "json": {...},                                     # POST JSON body
      "body_file": "import_media/x.png",                # POST raw bytes, path under oracle/fixtures
      "key": "full" | "restricted" | "none" | "<hex>",  # access key to send (default full)
      "compare": "exact" | "unordered_lists"            # hint for list ordering (default exact)
    }

Read-only scenarios (no step mutates anything) are replayed on one shared
boot; every mutating scenario gets a fresh copy of its fixture.

Usage: python oracle/build_scenarios.py
"""

import json
import os

HERE = os.path.dirname( os.path.abspath( __file__ ) )
OUT = os.path.join( HERE, 'scenarios' )

MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

FILES = MANIFEST[ 'files' ]
BY_NAME = { f[ 'name' ]: f[ 'hash' ] for f in FILES }
ALL_HASHES = [ f[ 'hash' ] for f in FILES ]

KEYS = {
    'my_tags': b'local tags'.hex(),
    'downloader_tags': b'downloader tags'.hex(),
    'all_known_tags': b'all known tags'.hex(),
    'my_files': b'local files'.hex(),
    'trash': b'trash'.hex(),
    'all_local_files': b'all local files'.hex(),
    'all_local_media': b'all local media'.hex(),
    'all_known_files': b'all known files'.hex(),
    'deleted_files': b'all deleted files'.hex(),
    'favourites': b'favourites'.hex(),
    **MANIFEST[ 'service_keys' ],
}


def enc( value ):
    """Encode a query value the way API clients do: JSON for structures and
    bools, plain strings for strings."""

    if isinstance( value, str ):

        return value


    return json.dumps( value )


def get( path, compare = 'exact', key = 'full', **params ):

    return { 'method': 'GET', 'path': path, 'query': [ [ k, enc( v ) ] for ( k, v ) in params.items() ], 'key': key, 'compare': compare }


def post( path, body, key = 'full' ):

    return { 'method': 'POST', 'path': path, 'json': body, 'key': key }


def search( tags, compare = 'exact', **params ):

    params.setdefault( 'return_hashes', True )

    return get( '/get_files/search_files', compare = compare, tags = tags, **params )


def metadata( hashes, **params ):

    return get( '/get_files/file_metadata', hashes = hashes, **params )


def scenario( name, description, steps, read_only, fixture = 'basic' ):

    return { 'name': name, 'description': description, 'fixture': fixture, 'read_only': read_only, 'steps': steps }


def access_scenarios():

    steps = [
        get( '/api_version', key = 'none' ),
        get( '/verify_access_key' ),
        get( '/verify_access_key', key = 'restricted' ),
        get( '/verify_access_key', key = 'none' ),
        get( '/verify_access_key', key = 'ab' * 32 ),
        get( '/verify_access_key', key = 'not hex' ),
        get( '/get_services' ),
        get( '/get_services', key = 'restricted' ),
        get( '/get_service', service_name = 'my tags' ),
        get( '/get_service', service_key = KEYS[ 'stars' ] ),
        get( '/get_service', service_key = KEYS[ 'favourites' ] ),
        get( '/get_service', service_key = KEYS[ 'counter' ] ),
        get( '/get_service', service_name = 'no such service' ),
        get( '/get_service', service_key = 'ff' * 32 ),
        get( '/no/such/endpoint' ),
        get( '/add_tags/clean_tags', tags = [ ' Blue_Eyes ', 'system:inbox', ':D', 'Character:Samus Aran', '', '-negated' ] ),
    ]

    return scenario( 'access', 'versions, access keys, services, cleaning', steps, read_only = True )


def search_scenarios():

    s = []

    everything_sorts = [ search( [ 'system:everything' ], file_sort_type = t, file_sort_asc = asc ) for t in ( 0, 2, 5, 6, 8, 9, 13, 16, 20 ) for asc in ( True, False ) ]

    s.extend( everything_sorts )

    # default parameters, and file ids rather than hashes
    s.append( get( '/get_files/search_files', tags = [ 'system:everything' ] ) )
    s.append( get( '/get_files/search_files', tags = [ 'system:everything' ], return_hashes = True, return_file_ids = False ) )

    tag_queries = [
        [ 'blue eyes' ], [ 'blue_eyes' ], [ 'bleu eyes' ], [ 'samus' ], [ 'character:samus aran' ], [ 'series:metroid' ],
        [ 'studio:nintendo' ], [ 'safe' ], [ '-safe' ], [ 'safe', 'smile' ], [ 'safe', '-smile' ], [ [ 'safe', 'explicit' ] ],
        [ [ 'safe', 'explicit' ], '-smile' ], [ 'character:*' ], [ 'series:*' ], [ 'page:*' ], [ '*eyes' ], [ 'blue*' ], [ 'b*' ],
        [ '*:samus*' ], [ 'キャラクター:初音ミク' ], [ '::)' ], [ 'title:a test image: with colons' ], [ 'éclair' ],
        [ 'no such tag' ], [ '-no such tag' ], [ 'filename:jpeg_00' ], [ 'source:fixture' ], [ 'second:canonical' ], [ 'second:tag' ],
        [ 'meta:low resolution' ], [ 'meta:lowres' ],
    ]

    for tags in tag_queries:

        s.append( search( tags, file_sort_type = 20 ) )


    # tag domains and file domains
    for tag_service in ( 'my_tags', 'downloader_tags', 'second_tags', 'all_known_tags' ):

        for tags in ( [ 'blue eyes' ], [ 'series:metroid' ], [ 'source:fixture' ], [ 'system:has tags' ], [ 'system:no tags' ], [ 'system:number of tags > 2' ] ):

            s.append( search( tags, file_sort_type = 20, tag_service_key = KEYS[ tag_service ] ) )



    for file_service in ( 'my_files', 'art', 'trash', 'all_local_files', 'all_local_media', 'all_known_files' ):

        for tags in ( [ 'system:everything' ], [ 'safe' ], [ 'system:inbox' ] ):

            s.append( search( tags, file_sort_type = 20, file_service_key = KEYS[ file_service ] ) )



    s.append( search( [ 'system:everything' ], file_sort_type = 20, file_service_keys = [ KEYS[ 'my_files' ], KEYS[ 'trash' ] ] ) )
    s.append( search( [ 'system:everything' ], file_sort_type = 20, deleted_file_service_key = KEYS[ 'my_files' ] ) )
    s.append( search( [ 'system:everything' ], file_sort_type = 20, deleted_file_service_key = KEYS[ 'all_local_files' ] ) )

    system_queries = [
        'system:inbox', 'system:archive', 'system:has duration', 'system:no duration', 'system:has audio', 'system:no audio',
        'system:has tags', 'system:no tags', 'system:untagged', 'system:number of tags > 2', 'system:number of tags = 1', 'system:number of tags ~= 3',
        'system:height = 480', 'system:height > 400', 'system:width < 200', 'system:width > 1000', 'system:filesize > 10 kilobytes', 'system:filesize < 5 KB',
        'system:filesize ~= 20 kilobytes', 'system:limit = 5', 'system:filetype = image/jpg', 'system:filetype = image/png, apng', 'system:filetype = video',
        'system:filetype = audio', 'system:filetype = animation', 'system:filetype = image', 'system:filetype = application', 'system:filetype = gif',
        'system:duration < 5 seconds', 'system:duration > 1500 milliseconds', 'system:ratio is wider than 16:9', 'system:ratio is 1:1', 'system:ratio taller than 1:1',
        'system:num pixels > 300000 px', 'system:num pixels < 1 megapixels', 'system:has notes', 'system:no notes', 'system:num notes = 2', 'system:num notes > 0',
        'system:has note with name comment', 'system:no note with name comment', 'system:has note with name translation',
        'system:has url matching regex danbooru', 'system:does not have a url matching regex danbooru', 'system:has url https://example.com/post/3',
        'system:does not have url https://example.com/post/3', 'system:has domain gelbooru.com', 'system:does not have domain gelbooru.com',
        'system:has a url with class danbooru file page', 'system:tag as number page < 5', 'system:tag as number page > 2',
        'system:has a rating for favourites', 'system:does not have a rating for favourites', 'system:rating for favourites is like', 'system:rating for favourites is dislike',
        'system:has a rating for stars', 'system:rating for stars > 3/5', 'system:rating for stars = 0/5', 'system:rating for counter = 3', 'system:rating for counter > 2',
        'system:views in media > 2', 'system:views in preview = 1', 'system:views > 0', 'system:viewtime in media > 5 seconds',
        'system:number of file relationships = 1 duplicates', 'system:num file relationships = 1 alternates', 'system:num file relationships = 1 false positives',
        'system:num file relationships > 0 potential duplicates', 'system:is the best quality file of its duplicate group', 'system:is not the best quality file of its duplicate group',
        'system:file service currently in art', 'system:file service is not currently in art', 'system:file service currently in trash',
        'system:modified date < 2021-01-01', 'system:modified date > 2021-01-01', 'system:time imported > 2020-01-01', 'system:last viewed time < 2024-01-01',
        'system:hash = {}'.format( BY_NAME[ 'jpeg_00.jpg' ] ), 'system:hash = {} {}'.format( BY_NAME[ 'jpeg_00.jpg' ], BY_NAME[ 'png_00.png' ] ),
        'system:similar to {} distance 8'.format( BY_NAME[ 'jpeg_02.jpg' ] ), 'system:similar to {} with distance 0'.format( BY_NAME[ 'jpeg_02.jpg' ] ),
        'system:has exif', 'system:no exif', 'system:has icc profile', 'system:has human-readable metadata', 'system:number of words < 2',
    ]

    for text in system_queries:

        s.append( search( [ text ], file_sort_type = 20 ) )


    s.append( search( [ 'system:everything', 'system:limit = 3' ], file_sort_type = 0, file_sort_asc = False ) )
    s.append( search( [ 'system:archive', 'safe', '-system:has audio' ], file_sort_type = 20 ) )
    s.append( search( [ [ 'system:inbox', 'blue eyes' ] ], file_sort_type = 20 ) )

    # errors
    s.append( search( [ 'system:not a real predicate' ] ) )
    s.append( search( [ 'system:width > banana' ] ) )
    s.append( get( '/get_files/search_files', tags = 'not json' ) )
    s.append( search( [ 'blue eyes' ], tag_service_key = 'ff' * 32 ) )

    # permissions: restricted key may only search when a whitelisted tag is present
    s.append( search( [ 'safe' ], file_sort_type = 20, key = 'restricted' ) )
    s.append( search( [ 'blue eyes' ], file_sort_type = 20, key = 'restricted' ) )
    s.append( search( [ 'system:everything' ], file_sort_type = 20, key = 'restricted' ) )

    return scenario( 'search', 'file searches: tags, siblings, parents, wildcards, domains, system predicates, sorts', s, read_only = True )


def metadata_scenarios():

    s = []

    for f in FILES:

        s.append( metadata( [ f[ 'hash' ] ] ) )


    some = ALL_HASHES[ :6 ]

    s.append( metadata( some, only_return_identifiers = True ) )
    s.append( metadata( some, only_return_basic_information = True ) )
    s.append( metadata( some, detailed_url_information = True ) )
    s.append( metadata( some, include_blurhash = True ) )
    s.append( metadata( some, include_milliseconds = True ) )
    s.append( metadata( some, include_notes = True ) )
    s.append( metadata( some, include_services_object = False ) )
    s.append( metadata( [ 'ab' * 32 ] ) )
    s.append( metadata( [ 'ab' * 32 ], create_new_file_ids = False ) )
    s.append( get( '/get_files/file_metadata', file_ids = [ 1, 2, 3 ] ) )
    s.append( get( '/get_files/file_metadata', file_ids = [ 999999 ] ) )
    s.append( get( '/get_files/file_metadata', hashes = [ 'not a hash' ] ) )
    s.append( get( '/get_files/file_metadata' ) )

    s.append( get( '/get_files/file_hashes', hash = BY_NAME[ 'jpeg_00.jpg' ], source_hash_type = 'sha256', desired_hash_type = 'md5' ) )
    s.append( get( '/get_files/file_hashes', hashes = some, source_hash_type = 'sha256', desired_hash_type = 'sha1' ) )
    s.append( get( '/get_files/file_hashes', hashes = some, source_hash_type = 'sha256', desired_hash_type = 'sha512' ) )

    return scenario( 'metadata', 'file_metadata in every shape, and hash conversion', s, read_only = True )


def tag_read_scenarios():

    s = []

    for text in ( 'blue', 'blue*', 'b', 'bl', 'samus', 'character:', 'character:*', 'series:met', 'page:', '*eyes', 'キャラ', '::', 'no such', 'ÉCL', '-blue' ):

        for display in ( 'storage', 'display' ):

            s.append( get( '/add_tags/search_tags', compare = 'unordered_lists', search = text, tag_display_type = display ) )



    for tag_service in ( 'my_tags', 'downloader_tags', 'second_tags' ):

        s.append( get( '/add_tags/search_tags', compare = 'unordered_lists', search = 'b', tag_service_key = KEYS[ tag_service ] ) )


    for file_service in ( 'art', 'trash', 'all_known_files' ):

        s.append( get( '/add_tags/search_tags', compare = 'unordered_lists', search = 's', file_service_key = KEYS[ file_service ] ) )


    s.append( get( '/add_tags/search_tags', compare = 'unordered_lists', search = 'safe', key = 'restricted' ) )

    s.append( get( '/add_tags/get_siblings_and_parents', compare = 'unordered_lists', tags = [ 'blue eyes', 'blue_eyes', 'bleu eyes', 'samus', 'character:samus aran', 'series:metroid', 'studio:nintendo', 'unrelated', 'second:tag', 'filename:jpeg_00' ] ) )

    s.append( get( '/add_tags/get_favourite_tags' ) )

    return scenario( 'tags_read', 'autocomplete, siblings/parents lookup, favourites', s, read_only = True )


def url_read_scenarios():

    s = []

    for url in ( 'https://example.com/post/3', 'https://danbooru.donmai.us/posts/1003', 'https://gelbooru.com/index.php?page=post&s=view&id=2000',
                 'https://gelbooru.com/index.php?id=2000&s=view&page=post', 'https://unknown.example.org/whatever', 'not a url' ):

        s.append( get( '/add_urls/get_url_files', url = url ) )
        s.append( get( '/add_urls/get_url_info', url = url ) )


    s.append( get( '/add_urls/get_url_files', url = 'https://danbooru.donmai.us/posts/1003', doublecheck_file_system = True ) )

    return scenario( 'urls_read', 'url lookups and url classes', s, read_only = True )


def files_read_scenarios():

    s = []

    for name in ( 'jpeg_00.jpg', 'png_alpha_00.png', 'gif_animated.gif', 'video_with_audio.mp4', 'audio.mp3', 'document.pdf', 'archive.zip' ):

        h = BY_NAME[ name ]

        s.append( get( '/get_files/file', hash = h ) )
        s.append( get( '/get_files/thumbnail', hash = h ) )
        s.append( get( '/get_files/file_path', hash = h ) )
        s.append( get( '/get_files/thumbnail_path', hash = h ) )


    s.append( get( '/get_files/file', file_id = 1 ) )
    s.append( get( '/get_files/file', hash = 'ab' * 32 ) )
    s.append( get( '/get_files/thumbnail', hash = 'ab' * 32 ) )
    s.append( get( '/get_files/file', hash = ALL_HASHES[ -1 ] ) )  # fully deleted
    s.append( get( '/get_files/local_file_storage_locations' ) )
    s.append( get( '/get_files/file', hash = BY_NAME[ 'jpeg_00.jpg' ], key = 'restricted' ) )

    return scenario( 'files_read', 'fetching files, thumbnails and paths', s, read_only = True )


def relationships_read_scenarios():

    s = []

    s.append( get( '/manage_file_relationships/get_file_relationships', hashes = ALL_HASHES ) )
    s.append( get( '/manage_file_relationships/get_potentials_count' ) )
    s.append( get( '/manage_file_relationships/get_potentials_count', potentials_search_type = 1 ) )
    s.append( get( '/manage_file_relationships/get_potential_pairs', compare = 'unordered_lists' ) )
    s.append( get( '/manage_file_relationships/get_potentials_count', tags_1 = [ 'format:jpeg' ], potentials_search_type = 2, tags_2 = [ 'format:jpeg' ] ) )

    return scenario( 'relationships_read', 'duplicates and potentials', s, read_only = True )


def write_scenarios():

    out = []

    h0 = BY_NAME[ 'jpeg_00.jpg' ]
    h1 = BY_NAME[ 'jpeg_01.jpg' ]

    out.append( scenario( 'add_tags', 'adding and removing tags, then reading them back', [
        post( '/add_tags/add_tags', { 'hash': h0, 'service_keys_to_tags': { KEYS[ 'my_tags' ]: [ 'new tag', 'Blue_Eyes', 'character:link', '  spaced  out  ' ] } } ),
        metadata( [ h0 ] ),
        search( [ 'new tag' ], file_sort_type = 20 ),
        search( [ 'blue eyes' ], file_sort_type = 20 ),
        search( [ 'studio:nintendo' ], file_sort_type = 20 ),
        get( '/add_tags/search_tags', compare = 'unordered_lists', search = 'new' ),
        post( '/add_tags/add_tags', { 'hashes': [ h0, h1 ], 'service_keys_to_actions_to_tags': { KEYS[ 'my_tags' ]: { '1': [ 'new tag', 'safe' ] }, KEYS[ 'downloader_tags' ]: { '0': [ 'added via actions' ] } } } ),
        metadata( [ h0, h1 ] ),
        search( [ 'new tag' ], file_sort_type = 20 ),
        post( '/add_tags/add_tags', { 'hash': h0, 'service_keys_to_tags': { KEYS[ 'my_tags' ]: [ 'new tag' ] }, 'override_previously_deleted_mappings': False } ),
        metadata( [ h0 ] ),
        post( '/add_tags/add_tags', { 'hash': h0, 'service_keys_to_tags': { KEYS[ 'my_tags' ]: [ 'new tag' ] } } ),
        metadata( [ h0 ] ),
        post( '/add_tags/add_tags', { 'hash': h0, 'service_keys_to_tags': { KEYS[ 'all_known_tags' ]: [ 'bad' ] } } ),
        post( '/add_tags/add_tags', { 'hash': h0 } ),
        post( '/add_tags/add_tags', { 'hash': h0, 'service_keys_to_tags': { KEYS[ 'my_tags' ]: [ 'x' ] } }, key = 'restricted' ),
        post( '/add_tags/set_favourite_tags', { 'set': [ 'favourite one', 'favourite two' ] } ),
        get( '/add_tags/get_favourite_tags' ),
        post( '/add_tags/set_favourite_tags', { 'add': [ 'favourite three' ], 'remove': [ 'favourite one' ] } ),
        get( '/add_tags/get_favourite_tags' ),
    ], read_only = False ) )

    out.append( scenario( 'file_lifecycle', 'archive, trash, undelete, migrate, physical delete, clear deletion records', [
        post( '/add_files/archive_files', { 'hash': h0 } ),
        post( '/add_files/unarchive_files', { 'hash': h1 } ),
        metadata( [ h0, h1 ] ),
        search( [ 'system:inbox' ], file_sort_type = 20 ),
        post( '/add_files/delete_files', { 'hash': h0, 'reason': 'conformance test' } ),
        metadata( [ h0 ] ),
        search( [ 'system:everything' ], file_sort_type = 20, file_service_key = KEYS[ 'trash' ] ),
        post( '/add_files/undelete_files', { 'hash': h0 } ),
        metadata( [ h0 ] ),
        post( '/add_files/migrate_files', { 'hash': h1, 'file_service_key': KEYS[ 'art' ] } ),
        metadata( [ h1 ] ),
        post( '/add_files/delete_files', { 'hash': h1, 'file_service_key': KEYS[ 'all_local_files' ] } ),
        metadata( [ h1 ] ),
        get( '/get_files/file', hash = h1 ),
        post( '/add_files/clear_file_deletion_record', { 'hash': h1 } ),
        metadata( [ h1 ] ),
        post( '/add_files/migrate_files', { 'hash': h0, 'file_service_key': KEYS[ 'all_known_tags' ] } ),
        post( '/add_files/delete_files', { 'hash': 'ab' * 32 } ),
    ], read_only = False ) )

    out.append( scenario( 'notes_ratings_times', 'notes, ratings, viewing stats and timestamps', [
        post( '/add_notes/set_notes', { 'hash': h0, 'notes': { 'new': 'hello', 'comment': 'overwritten?' } } ),
        post( '/add_notes/set_notes', { 'hash': h0, 'notes': { 'new': 'hello again' }, 'merge_cleverly': True, 'extend_existing_note_if_possible': True, 'conflict_resolution': 3 } ),
        metadata( [ h0 ], include_notes = True ),
        post( '/add_notes/delete_notes', { 'hash': h0, 'note_names': [ 'new', 'missing' ] } ),
        metadata( [ h0 ], include_notes = True ),
        post( '/edit_ratings/set_rating', { 'hash': h0, 'rating_service_key': KEYS[ 'favourites' ], 'rating': None } ),
        post( '/edit_ratings/set_rating', { 'hash': h0, 'rating_service_key': KEYS[ 'stars' ], 'rating': 5 } ),
        post( '/edit_ratings/set_rating', { 'hash': h0, 'rating_service_key': KEYS[ 'counter' ], 'rating': 42 } ),
        post( '/edit_ratings/set_rating', { 'hash': h0, 'rating_service_key': KEYS[ 'stars' ], 'rating': 9 } ),
        post( '/edit_ratings/set_rating', { 'hash': h0, 'rating_service_key': KEYS[ 'favourites' ], 'rating': 3 } ),
        metadata( [ h0 ] ),
        post( '/edit_times/increment_file_viewtime', { 'hash': h0, 'canvas_type': 4, 'views': 2, 'viewtime': 3.5, 'timestamp_ms': 1_750_000_000_000 } ),
        post( '/edit_times/set_file_viewtime', { 'hash': h0, 'canvas_type': 0, 'views': 10, 'viewtime': 60.0, 'timestamp_ms': 1_750_000_000_001 } ),
        post( '/edit_times/set_time', { 'hash': h0, 'timestamp_ms': 1_500_000_000_000, 'timestamp_type': 0, 'domain': 'example.org' } ),
        post( '/edit_times/set_time', { 'hash': h0, 'timestamp': 1_400_000_000.5, 'timestamp_type': 1 } ),
        post( '/edit_times/set_time', { 'hash': h0, 'timestamp_ms': 1_300_000_000_000, 'timestamp_type': 3, 'file_service_key': KEYS[ 'my_files' ] } ),
        post( '/edit_times/set_time', { 'hash': h0, 'timestamp_ms': None, 'timestamp_type': 0, 'domain': 'example.org' } ),
        metadata( [ h0 ], include_milliseconds = True ),
    ], read_only = False ) )

    out.append( scenario( 'urls_write', 'associating and removing urls', [
        post( '/add_urls/associate_url', { 'hash': h0, 'urls_to_add': [ 'https://new.example.com/a', 'https://danbooru.donmai.us/posts/77' ], 'urls_to_delete': [ 'https://example.com/post/0' ] } ),
        post( '/add_urls/associate_url', { 'hash': h0, 'url_to_add': 'https://gelbooru.com/index.php?id=5&page=post&s=view', 'normalise_urls': True } ),
        post( '/add_urls/associate_url', { 'hash': h0, 'url_to_add': 'https://gelbooru.com/index.php?id=6&page=post&s=view', 'normalise_urls': False } ),
        metadata( [ h0 ], detailed_url_information = True ),
        get( '/add_urls/get_url_files', url = 'https://danbooru.donmai.us/posts/77' ),
        get( '/add_urls/get_url_files', url = 'https://example.com/post/0' ),
    ], read_only = False ) )

    out.append( scenario( 'add_files', 'importing files by path and by upload, duplicates, deleted files, bad input', [
        post( '/add_files/add_file', { 'path': '{MEDIA}/jpeg_05.jpg' } ),
        { 'method': 'POST', 'path': '/add_files/add_file', 'body_file': 'import_media/png_alpha_01.png', 'key': 'full' },
        { 'method': 'POST', 'path': '/add_files/add_file', 'body_file': 'import_media/../import_media/webp_animated.webp', 'key': 'full' },
        post( '/add_files/add_file', { 'path': '/definitely/not/here.png' } ),
        post( '/add_files/generate_hashes', { 'path': '{MEDIA}/jpeg_00.jpg' } ),
        { 'method': 'POST', 'path': '/add_files/generate_hashes', 'body_file': 'import_media/png_00.png', 'key': 'full' },
        post( '/add_files/add_file', { 'path': '{MEDIA}/jpeg_00.jpg', 'file_service_key': KEYS[ 'art' ] } ),
    ], read_only = False ) )

    out.append( scenario( 'relationships_write', 'setting duplicate relationships and kings', [
        post( '/manage_file_relationships/set_file_relationships', { 'relationships': [
            { 'hash_a': BY_NAME[ 'jpeg_03.jpg' ], 'hash_b': BY_NAME[ 'jpeg_04.jpg' ], 'relationship': 2, 'do_default_content_merge': False },
            { 'hash_a': BY_NAME[ 'jpeg_05.jpg' ], 'hash_b': BY_NAME[ 'jpeg_06.jpg' ], 'relationship': 0, 'do_default_content_merge': False },
        ] } ),
        get( '/manage_file_relationships/get_file_relationships', hashes = [ BY_NAME[ n ] for n in ( 'jpeg_03.jpg', 'jpeg_04.jpg', 'jpeg_05.jpg', 'jpeg_06.jpg' ) ] ),
        post( '/manage_file_relationships/set_kings', { 'hash': BY_NAME[ 'jpeg_04.jpg' ] } ),
        get( '/manage_file_relationships/get_file_relationships', hashes = [ BY_NAME[ 'jpeg_03.jpg' ], BY_NAME[ 'jpeg_04.jpg' ] ] ),
        post( '/manage_file_relationships/remove_potentials', { 'hashes': [ BY_NAME[ 'jpeg_05.jpg' ] ] } ),
        get( '/manage_file_relationships/get_potentials_count' ),
    ], read_only = False ) )

    return out


def main():

    os.makedirs( OUT, exist_ok = True )

    scenarios = [ access_scenarios(), search_scenarios(), metadata_scenarios(), tag_read_scenarios(), url_read_scenarios(), files_read_scenarios(), relationships_read_scenarios() ] + write_scenarios()

    for sc in scenarios:

        with open( os.path.join( OUT, sc[ 'name' ] + '.json' ), 'w' ) as f:

            json.dump( sc, f, indent = 1, ensure_ascii = False )
            f.write( '\n' )



    print( f'{len( scenarios )} scenarios, {sum( len( sc[ "steps" ] ) for sc in scenarios )} steps' )


if __name__ == '__main__':

    main()

