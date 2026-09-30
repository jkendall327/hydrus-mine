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


def fixture_similarity_data():
    """A pixel hash and a perceptual hash of jpeg_02.jpg's family, read from
    the fixture database, for system:similar to data."""

    import sqlite3
    import tarfile
    import tempfile

    with tempfile.TemporaryDirectory() as d:

        with tarfile.open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.tar.gz' ) ) as t:

            for name in ( 'client.db', 'client.master.db' ):

                t.extract( name, d )



        c = sqlite3.connect( os.path.join( d, 'client.db' ) )
        c.execute( 'ATTACH ? AS m', ( os.path.join( d, 'client.master.db' ), ) )

        wanted = bytes.fromhex( BY_NAME[ 'jpeg_02.jpg' ] )

        ( pixel_hash, ) = c.execute( 'SELECT p.hash FROM m.hashes f JOIN pixel_hash_map x ON x.hash_id = f.hash_id JOIN m.hashes p ON p.hash_id = x.pixel_hash_id WHERE f.hash = ?', ( wanted, ) ).fetchone()
        ( phash, ) = c.execute( 'SELECT p.phash FROM m.hashes f JOIN m.shape_perceptual_hash_map x ON x.hash_id = f.hash_id JOIN m.shape_perceptual_hashes p USING ( phash_id ) WHERE f.hash = ?', ( wanted, ) ).fetchone()

        c.close()


    return ( pixel_hash.hex(), phash.hex() )


def search_more_scenarios():
    """Sorts and predicates the first search scenario does not cover."""

    import hashlib

    s = []

    for t in ( 1, 7, 10, 11, 12, 14, 15, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27 ):

        for asc in ( True, False ):

            s.append( search( [ 'system:everything' ], file_sort_type = t, file_sort_asc = asc ) )



    # import time sorts in other domains
    s.append( search( [ 'system:everything' ], file_sort_type = 2, file_sort_asc = False, file_service_key = KEYS[ 'trash' ] ) )
    s.append( search( [ 'system:everything' ], file_sort_type = 2, deleted_file_service_key = KEYS[ 'my_files' ] ) )
    s.append( search( [ 'system:everything' ], file_sort_type = 2, file_service_keys = [ KEYS[ 'art' ], KEYS[ 'trash' ] ] ) )

    ( pixel_hash, phash ) = fixture_similarity_data()
    md5 = hashlib.md5( open( os.path.join( HERE, 'fixtures', 'import_media', 'jpeg_03.jpg' ), 'rb' ).read() ).hexdigest()

    predicates = [
        # file properties
        'system:framerate < 30', 'system:framerate > 5', 'system:num frames > 10', 'system:number of frames < 2', 'system:has frames',
        'system:num urls > 2', 'system:num urls = 0', 'system:has urls', 'system:no urls', 'system:num urls ~= 2',
        'system:has transparency', 'system:no transparency', 'system:has forced filetype', 'system:no forced filetype',
        'system:has xmp', 'system:no iptc', 'system:has software/source metadata', 'system:no human-readable metadata',
        'system:has width', 'system:no height', 'system:has words', 'system:width = 0', 'system:height <= 200', 'system:width >= 1000',
        'system:width ~= 640', 'system:height != 480', 'system:duration ~= 2 seconds',
        'system:filesize != 5 KB', 'system:filesize > 0 B', 'system:num pixels ~= 300 kilopixels', 'system:num pixels = 307200 pixels', 'system:num pixels != 307200 pixels',
        'system:ratio ~= 4:3', 'system:ratio is square', 'system:ratio is portrait', 'system:ratio is landscape', 'system:ratio = 4:3',
        'system:filetype = image/png', 'system:filetype = static gif', 'system:filetype != image', 'system:filetype = image project file', 'system:filetype = archive, pdf',
        # times
        'system:archived time > 2025-01-01', 'system:archived time < 2025-01-01', 'system:archived time ~= 2026-09-20',
        'system:time imported < 100 years', 'system:time imported > 100 years', 'system:time imported ~= 2026-09-20',
        'system:modified date > 2020-09-15', 'system:modified date < 2020-09-15', 'system:modified time ~= 2020-09-20',
        'system:last viewed time > 2023-01-01', 'system:last viewed time < 3 years',
        # hashes and similar files
        f'system:hash = {md5} md5', f'system:hash != {BY_NAME[ "jpeg_00.jpg" ]}',
        f'system:similar to {BY_NAME[ "jpeg_02.jpg" ]} distance 20', f'system:similar to data {pixel_hash}', f'system:similar to data {phash} distance 4',
        # file services
        'system:file service is pending to my files', 'system:file service is not pending to my files', 'system:file service currently in my files', 'system:file service is not currently in trash',
        'system:file service currently in no such service',
        # tag counts
        'system:number of character tags > 0', 'system:number of series tags = 1', 'system:number of unnamespaced tags ~= 3', 'system:number of filename tags = 0',
        'system:number of tags < 3', 'system:number of tags != 4', 'system:number of tags ~= 5', 'system:number of page tags > 0',
        # views
        'system:views ~= 5', 'system:views < 3', 'system:viewtime < 3 seconds', 'system:views in preview > 0', 'system:viewtime in media, preview ~= 10 seconds', 'system:views in client api = 0',
        # relationships
        'system:num file relationships < 2 duplicates', 'system:num file relationships ~= 1 alternates', 'system:num file relationships > 1 potential duplicates', 'system:num file relationships = 0 false positives',
        # tag as number
        'system:tag as number page ~= 10', 'system:tag as number page > 10', 'system:tag as number volume < 5', 'system:tag as number * > 5',
        # urls and notes
        'system:has domain donmai.us', 'system:does not have domain example.com', 'system:has url matching regex \\d{4}$', 'system:has url matching regex post/[0-4]$',
        'system:has note with name nope', 'system:no note with name translation', 'system:num notes ~= 2', 'system:num notes < 2',
        # ratings
        'system:rating for stars ~= 3/5', 'system:rating for stars < 2/5', 'system:rating for stars > 1/5', 'system:rating for stars = 5/5', 'system:rating for stars is like',
        'system:rating for favourites = 1/5', 'system:does not have a rating for stars',
        'system:rating for counter < 5', 'system:rating for counter > 5', 'system:rating for counter = 0', 'system:has a rating for counter', 'system:does not have a rating for counter',
        'system:rating for no such service > 1/5',
        'system:all ratings rated', 'system:any ratings rated', 'system:all ratings not rated', 'system:any ratings not rated',
        'system:all like/dislike ratings rated', 'system:any inc/dec ratings rated', 'system:only favourites rated', 'system:only favourites, stars rated', 'system:only stars not rated',
        'system:only favourites (amongst favourites, stars) rated',
        # advanced tags
        'system:has tag in "my tags", ignoring siblings/parents: "samus"',
        'system:has tag in "my tags": "samus"',
        'system:has tag in "my tags": "series:metroid"',
        'system:has tag in "downloader tags": "series:metroid"',
        'system:has tag in "all known tags", ignoring siblings/parents: "character:samus aran"',
        'system:has tag with status in deleted: "blue eyes"',
        'system:has tag with status in deleted, current: "blue eyes"',
        'system:does not have tag in "my tags": "safe"',
        'system:has tag in "no such service": "safe"',
    ]

    for text in predicates:

        s.append( search( [ text ], file_sort_type = 20 ) )


    tag_searches = [
        [ '*:*' ], [ 'character:*' ], [ 'char*:*' ], [ '*s*' ], [ 'blue *' ], [ 'b*e*' ], [ '*:blue eyes' ], [ '*:blue_eyes' ],
        [ '-*eyes' ], [ '-series:*' ], [ 'sa*e' ], [ 'series:the legend*' ], [ 'キャラ*:*' ], [ '*:初音*' ], [ 'éc*' ], [ '*:canonical' ],
        [ 'safe', '-series:*' ], [ 'meta:*', '-meta:lowres' ], [ 'studio:*' ], [ 'studio:nintendo', '-character:*' ],
        [ [ '-safe', 'smile' ] ], [ [ 'system:archive', 'character:*' ], '-smile' ], [ [ 'blue eyes', [ 'smile', 'explicit' ] ] ],
        [ [ 'system:width > 1000', 'system:has audio', '-safe' ], 'system:no duration' ],
        [ 'safe', 'system:limit = 4' ], [ 'system:everything', 'system:limit = 100' ], [ 'system:limit = 0' ],
    ]

    for tags in tag_searches:

        s.append( search( tags, file_sort_type = 20 ) )


    # tag and file domains
    for ( tag_service, file_service ) in ( ( 'second_tags', 'art' ), ( 'my_tags', 'trash' ) ):

        for tags in ( [ 'system:everything' ], [ 'safe' ], [ 'system:no tags' ], [ 'system:number of tags > 3' ] ):

            s.append( search( tags, file_sort_type = 20, tag_service_key = KEYS[ tag_service ], file_service_key = KEYS[ file_service ] ) )



    # the reference cannot sort "all known files" results, so compare them as sets
    # (its tag counts there are of storage tags, as it has no display cache for it)
    for tag_service in ( 'my_tags', 'downloader_tags' ):

        for tags in ( [ 'system:everything' ], [ 'safe' ], [ 'system:no tags' ], [ 'filename:*' ] ):

            s.append( search( tags, compare = 'unordered_lists', file_sort_type = 20, tag_service_key = KEYS[ tag_service ], file_service_key = KEYS[ 'all_known_files' ] ) )



    s.append( search( [ 'blue eyes' ], file_sort_type = 20, include_current_tags = False ) )
    s.append( search( [ 'system:has tags' ], file_sort_type = 20, include_pending_tags = False ) )
    s.append( search( [ 'system:no tags' ], file_sort_type = 20, include_current_tags = False, include_pending_tags = False ) )
    s.append( search( [ 'series:metroid' ], file_sort_type = 20, tag_service_name = 'my tags' ) )
    s.append( search( [ 'safe' ], file_sort_type = 20, file_service_name = 'art' ) )
    s.append( search( [ 'safe' ], file_sort_type = 20, tag_service_name = 'no such service' ) )
    s.append( search( [ 'safe' ], file_sort_type = 20, file_service_key = KEYS[ 'my_tags' ] ) )
    s.append( search( [ 'safe' ], file_sort_type = 99 ) )
    s.append( search( [ 'safe' ], file_sort_type = 20, deleted_file_service_keys = [ KEYS[ 'my_files' ], KEYS[ 'trash' ] ] ) )
    s.append( get( '/get_files/search_files', tags = [] ) )
    s.append( get( '/get_files/search_files', tags = [ 'safe' ], return_file_ids = False, return_hashes = True, file_sort_type = 20 ) )
    s.append( get( '/get_files/search_files', tags = [ 'safe' ], file_sort_asc = False ) )
    s.append( search( [ '-' ] ) )

    # restricted key permissions
    for tags in ( [ 'safe', 'blue eyes' ], [ 'safe', 'system:inbox' ], [ 'safe*' ], [ '-safe' ], [ [ 'safe', 'smile' ] ], [ 'safe', '-smile' ], [ 'blue eyes', '-safe' ] ):

        s.append( search( tags, file_sort_type = 20, key = 'restricted' ) )


    return scenario( 'search_more', 'more file searches: every sort, more predicates, wildcards, domains and permissions', s, read_only = True )


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
    # the fixture has one potential group, so a random one is always it
    s.append( get( '/manage_file_relationships/get_random_potentials', compare = 'unordered_lists' ) )
    s.append( get( '/manage_file_relationships/get_random_potentials', compare = 'unordered_lists', tags_1 = [ 'system:filetype is png' ], potentials_search_type = 1 ) )
    s.append( get( '/manage_file_relationships/get_random_potentials', compare = 'unordered_lists', pixel_duplicates = 0 ) )
    s.append( get( '/manage_file_relationships/get_random_potentials', compare = 'unordered_lists', max_hamming_distance = 0 ) )

    # the group is two jpegs and a png: file searches with system predicates
    png = [ 'system:filetype is png' ]
    jpeg = [ 'system:filetype is jpeg' ]

    for search_type in ( 0, 1 ):

        for tags in ( png, jpeg, [ 'system:inbox' ], [ 'system:archive' ], [ 'system:width > 50', 'system:filetype is image' ] ):

            s.append( get( '/manage_file_relationships/get_potentials_count', tags_1 = tags, potentials_search_type = search_type ) )



    s.append( get( '/manage_file_relationships/get_potentials_count', tags_1 = png, tags_2 = jpeg, potentials_search_type = 2 ) )
    s.append( get( '/manage_file_relationships/get_potential_pairs', compare = 'unordered_lists', tags_1 = png, potentials_search_type = 0 ) )
    s.append( get( '/manage_file_relationships/get_random_potentials', compare = 'unordered_lists', tags_1 = jpeg, potentials_search_type = 1 ) )
    s.append( get( '/manage_file_relationships/get_potentials_count', tags_1 = [ 'system:filetype is jpeg' ], tag_service_key_1 = KEYS[ 'my_tags' ] ) )

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

    headers = '/manage_headers/set_headers'
    cookies = '/manage_cookies/set_cookies'

    out.append( scenario( 'network', 'cookies and custom HTTP headers (what browser extensions send)', [
        get( '/manage_headers/get_headers' ),
        get( '/manage_headers/get_headers', domain = 'example.com' ),
        get( '/manage_headers/get_headers', domain = 'localhost' ),
        post( headers, { 'headers': { 'User-Agent': { 'value': 'Test UA' } } } ),
        get( '/manage_headers/get_headers' ),
        post( headers, { 'domain': 'example.com', 'headers': { 'X-Test': { 'value': '1', 'approved': 'pending', 'reason': 'why' }, 'X-Other': { 'value': '2' } } } ),
        get( '/manage_headers/get_headers', domain = 'example.com' ),
        post( headers, { 'domain': 'example.com', 'headers': { 'X-Test': { 'approved': 'denied' } } } ),
        get( '/manage_headers/get_headers', domain = 'example.com' ),
        post( headers, { 'domain': 'example.com', 'headers': { 'X-Missing': { 'approved': 'denied' } } } ),
        post( headers, { 'domain': 'example.com', 'headers': { 'X-Test': {} } } ),
        post( headers, { 'domain': 'example.com', 'headers': { 'X-Test': { 'approved': 'maybe' } } } ),
        post( headers, { 'domain': 'example.com', 'headers': { 'X-Test': { 'value': None } } } ),
        get( '/manage_headers/get_headers', domain = 'example.com' ),
        post( headers, { 'domain': None, 'headers': { 'User-Agent': { 'value': 'Test UA', 'reason': 'unchanged when the value is' } } } ),
        get( '/manage_headers/get_headers' ),
        post( '/manage_headers/set_user_agent', { 'user-agent': 'Another UA' } ),
        get( '/manage_headers/get_headers' ),
        post( '/manage_headers/set_user_agent', { 'user-agent': '' } ),
        get( '/manage_headers/get_headers' ),
        post( cookies, { 'cookies': [
            [ 'sid', 'abc', '.example.com', '/', 1900000000 ],
            [ 'pref', 'x', 'example.com', '/a', None ],
            [ 'early', 'e', 'example.com', '/', None ],
            [ 'other', '1', 'sub.example.co.uk', '/', None ],
        ] } ),
        get( '/manage_cookies/get_cookies', domain = 'example.com' ),
        get( '/manage_cookies/get_cookies', domain = 'www.example.com' ),
        get( '/manage_cookies/get_cookies', domain = 'example.co.uk' ),
        get( '/manage_cookies/get_cookies', domain = 'co.uk' ),
        post( cookies, { 'cookies': [ [ 'pref', None, 'example.com', '/a', None ], [ 'sid', 'changed', '.example.com', '/', None ] ] } ),
        get( '/manage_cookies/get_cookies', domain = 'example.com' ),
        post( cookies, { 'cookies': [ [ 'a', 'b' ] ] } ),
        post( cookies, { 'cookies': [ [ 1, 'b', 'example.com', '/', None ] ] } ),
        post( cookies, { 'cookies': [ [ 'a', 'b', 'example.com', '/', 'soon' ] ] } ),
        get( '/manage_cookies/get_cookies', domain = 'localhost' ),
        get( '/manage_cookies/get_cookies', key = 'restricted', domain = 'example.com' ),
        post( headers, { 'headers': { 'X': { 'value': '1' } } }, key = 'restricted' ),
    ], read_only = False ) )

    out.extend( random_relationship_scenarios() )

    return out


def random_relationship_scenarios():
    """Random sequences of relationship decisions, checked after every few
    steps. The group bookkeeping (merging duplicate and alternates groups,
    carrying potentials and false positives across merges) has many paths;
    random walks cover combinations hand-written steps miss."""

    import random

    images = sorted( n for n in BY_NAME if n.startswith( ( 'jpeg_', 'png_', 'dupe_' ) ) )

    out = []

    for ( seed, merges ) in ( ( 1, False ), ( 2, False ), ( 3, True ) ):

        rng = random.Random( seed )
        pool = rng.sample( images, 9 )
        hashes = [ BY_NAME[ n ] for n in pool ]

        s = []

        for i in range( 60 ):

            roll = rng.random()

            if roll < 0.08:

                s.append( post( '/manage_file_relationships/set_kings', { 'hash': rng.choice( hashes ) } ) )

            elif roll < 0.13:

                s.append( post( '/manage_file_relationships/remove_potentials', { 'hashes': rng.sample( hashes, 2 ) } ) )

            else:

                rows = []

                # merges go one pair per request: the reference computes every
                # pair's merge from the state before the request (a known
                # difference; see docs/rust/DIFFERENCES.md)
                for _ in range( 1 if merges else rng.choice( ( 1, 1, 1, 2 ) ) ):

                    ( a, b ) = rng.sample( hashes, 2 ) if rng.random() < 0.95 else ( hashes[ 0 ], hashes[ 0 ] )

                    row = { 'hash_a': a, 'hash_b': b, 'relationship': rng.choice( ( 0, 0, 1, 2, 3, 4, 4, 7 ) ), 'do_default_content_merge': merges and rng.random() < 0.7 }

                    if merges and rng.random() < 0.15:

                        row[ rng.choice( ( 'delete_a', 'delete_b' ) ) ] = True


                    rows.append( row )


                s.append( post( '/manage_file_relationships/set_file_relationships', { 'relationships': rows } ) )


            if i % 4 == 3:

                s.append( get( '/manage_file_relationships/get_file_relationships', hashes = hashes ) )
                s.append( get( '/manage_file_relationships/get_potentials_count' ) )

                # (what merges do to metadata is checked against the reference's
                # database by dump_duplicate_merges.py: its API answers come from
                # a cache that drifts after merges)




        out.append( scenario( f'relationships_random_{seed}', 'random relationship decisions' + ( ' with metadata merges' if merges else '' ), s, read_only = False ) )


    return out


def random( step, *keys ):
    """Mark top-level response keys whose values are random (compared by type)."""

    step[ 'random' ] = list( keys )

    return step


def with_headers( step, **headers ):

    step[ 'headers' ] = { k.replace( '_', '-' ) : v for ( k, v ) in headers.items() }

    return step


def file_range_scenarios():

    h = BY_NAME[ 'jpeg_00.jpg' ]

    size = os.path.getsize( os.path.join( HERE, 'fixtures', 'import_media', 'jpeg_00.jpg' ) )

    def ranged( value, **params ):

        return with_headers( get( '/get_files/file', hash = h, **params ), Range = value )


    steps = [
        get( '/get_files/file', hash = h ),
        get( '/get_files/file', hash = h, download = True ),
        ranged( 'bytes=0-99' ),
        ranged( 'bytes=100-' ),
        ranged( 'bytes=-100' ),
        ranged( 'bytes=0-0' ),
        ranged( f'bytes=0-{size - 1}' ),
        ranged( f'bytes=0-{size}' ),
        ranged( f'bytes=10-{size + 5000}' ),
        ranged( f'bytes={size}-' ),
        ranged( f'bytes={size + 10}-' ),
        ranged( f'bytes={size + 10}-{size + 20}' ),
        ranged( f'bytes=-{size + 10}' ),
        ranged( 'bytes=-0' ),
        ranged( 'bytes=10-5' ),
        ranged( 'bytes=0-9,20-29' ),
        ranged( 'bytes=0-9, 20-29' ),
        ranged( 'items=0-9' ),
        ranged( 'bytes' ),
        ranged( 'bytes=5' ),
        ranged( 'bytes=-' ),
        ranged( 'bytes=abc-def' ),
        ranged( 'bytes=1-2-3' ),
        ranged( 'bytes= 5 - 9 ' ),
        ranged( 'bytes=0-99', download = True ),
        with_headers( get( '/get_files/thumbnail', hash = h ), Range = 'bytes=0-99' ),
    ]

    return scenario( 'file_ranges', 'byte ranges of files', steps, read_only = True )


def services_scenarios():

    tag_repo = '7a6' * 21 + '7'
    file_repo = 'f11e' * 16
    h = [ BY_NAME[ f'jpeg_{i:02}.jpg' ] for i in range( 3 ) ]
    counts = '/manage_services/get_pending_counts'

    steps = [
        get( counts ),
        get( counts, key = 'restricted' ),
        metadata( h ),
        post( '/add_tags/add_tags', { 'hashes': h, 'service_keys_to_actions_to_tags': { tag_repo: { '2': [ 'another pended tag' ] } } } ),
        post( '/add_tags/add_tags', { 'hash': h[ 0 ], 'service_keys_to_actions_to_tags': { tag_repo: { '3': [ 'pended tag' ] } } } ),
        get( counts ),
        post( '/manage_services/forget_pending', { 'service_key': tag_repo } ),
        get( counts ),
        metadata( h ),
        post( '/manage_services/forget_pending', { 'service_key': file_repo } ),
        get( counts ),
        metadata( h ),
        post( '/manage_services/forget_pending', { 'service_key': KEYS[ 'my_tags' ] } ),
        post( '/manage_services/forget_pending', { 'service_key': 'ab' * 32 } ),
        post( '/manage_services/forget_pending', {} ),
        post( '/manage_services/forget_pending', { 'service_key': tag_repo }, key = 'restricted' ),
        post( '/manage_services/commit_pending', { 'service_key': KEYS[ 'my_tags' ] } ),
        post( '/manage_services/commit_pending', { 'service_key': 'ab' * 32 } ),
        post( '/manage_services/commit_pending', { 'service_key': tag_repo }, key = 'restricted' ),
        get( '/get_services' ),
    ]

    return scenario( 'manage_services', 'pending counts, forgetting pending content, committing', steps, read_only = False, fixture = 'repositories' )


def access_more_scenarios():

    svg = '/get_service_rating_svg'

    steps = [
        random( get( '/session_key' ), 'session_key' ),
        random( get( '/session_key', key = 'restricted' ), 'session_key' ),
        get( '/session_key', key = 'none' ),
        random( get( '/client_info' ), 'boot_id', 'boot_time', 'currently_idle' ),
        random( get( '/client_info', key = 'restricted' ), 'boot_id', 'boot_time', 'currently_idle' ),
        get( '/client_info', key = 'none' ),
        get( '/request_new_permissions', key = 'none', name = 'a tool', basic_permissions = [ 0, 1 ] ),
        get( '/request_new_permissions', key = 'none' ),
        get( svg, service_key = KEYS[ 'stars' ] ),
        get( svg, service_key = KEYS[ 'favourites' ] ),
        get( svg, service_name = 'stars' ),
        get( svg, service_key = KEYS[ 'counter' ] ),
        get( svg, service_key = KEYS[ 'my_tags' ] ),
        get( svg, service_name = 'my tags' ),
        get( svg, service_name = 'no such service' ),
        get( svg, service_key = 'ff' * 32 ),
        get( svg ),
        get( svg, service_key = KEYS[ 'stars' ], key = 'restricted' ),
        get( svg, service_key = KEYS[ 'stars' ], key = 'none' ),
    ]

    return scenario( 'access_more', 'session keys, client info, permission requests, rating SVGs', steps, read_only = True )


def database_scenarios():

    bones = '/manage_database/mr_bones'

    read = scenario( 'manage_database', 'database statistics ("mr bones")', [
        get( bones ),
        get( bones, tags = [ 'system:inbox' ] ),
        get( bones, tags = [ 'system:everything' ] ),
        get( bones, tags = [ 'safe' ] ),
        get( bones, tags = [ 'system:filetype is image', 'system:width > 100' ] ),
        get( bones, file_service_key = KEYS[ 'my_files' ] ),
        get( bones, file_service_key = KEYS[ 'my_files' ], tags = [ 'system:archive' ] ),
        get( bones, file_service_key = KEYS[ 'art' ] ),
        get( bones, file_service_key = KEYS[ 'trash' ] ),
        get( bones, file_service_key = KEYS[ 'all_local_files' ] ),
        get( bones, file_service_key = KEYS[ 'all_local_files' ], tags = [ 'system:everything' ] ),
        get( bones, file_service_keys = [ KEYS[ 'my_files' ], KEYS[ 'trash' ] ] ),
        get( bones, deleted_file_service_key = KEYS[ 'my_files' ] ),
        get( bones, deleted_file_service_key = KEYS[ 'my_files' ], tags = [ 'system:everything' ] ),
        get( bones, file_service_key = KEYS[ 'all_known_files' ], tag_service_key = KEYS[ 'my_tags' ] ),
        get( bones, file_service_key = KEYS[ 'all_known_files' ], tag_service_key = KEYS[ 'my_tags' ], tags = [ 'safe' ] ),
        get( bones, file_service_key = KEYS[ 'all_known_files' ] ),
        get( bones, tag_service_key = KEYS[ 'my_tags' ], tags = [ 'safe' ] ),
        get( bones, tags = [ 'system:everything' ], key = 'restricted' ),
        get( bones, key = 'none' ),
        get( bones, tags = 'not a list' ),
    ], read_only = True )

    j = [ BY_NAME[ f'jpeg_{i:02}.jpg' ] for i in range( 12 ) ]

    write = scenario( 'manage_database_write', 'database statistics after changes, and locking the database', [
        post( '/manage_file_relationships/set_file_relationships', { 'relationships': [
            { 'hash_a': j[ 0 ], 'hash_b': j[ 1 ], 'relationship': 4, 'do_default_content_merge': False },
            { 'hash_a': j[ 0 ], 'hash_b': j[ 2 ], 'relationship': 4, 'do_default_content_merge': False },
            { 'hash_a': j[ 3 ], 'hash_b': j[ 4 ], 'relationship': 3, 'do_default_content_merge': False },
            { 'hash_a': j[ 3 ], 'hash_b': j[ 5 ], 'relationship': 3, 'do_default_content_merge': False },
            { 'hash_a': j[ 6 ], 'hash_b': j[ 7 ], 'relationship': 2, 'do_default_content_merge': False },
            { 'hash_a': j[ 6 ], 'hash_b': j[ 8 ], 'relationship': 3, 'do_default_content_merge': False },
        ] } ),
        post( '/edit_times/increment_file_viewtime', { 'hash': j[ 0 ], 'canvas_type': 0, 'views': 2, 'viewtime': 12.5 } ),
        post( '/edit_times/increment_file_viewtime', { 'hash': j[ 1 ], 'canvas_type': 1, 'views': 3, 'viewtime': 0.25 } ),
        post( '/edit_times/increment_file_viewtime', { 'hash': j[ 9 ], 'canvas_type': 0, 'views': 1, 'viewtime': 4 } ),
        post( '/add_files/delete_files', { 'hashes': [ j[ 2 ], j[ 9 ] ] } ),
        post( '/add_files/archive_files', { 'hashes': [ j[ 4 ], j[ 10 ] ] } ),
        get( '/manage_database/mr_bones' ),
        get( '/manage_database/mr_bones', file_service_key = KEYS[ 'my_files' ] ),
        get( '/manage_database/mr_bones', file_service_key = KEYS[ 'my_files' ], tags = [ 'system:filetype is jpeg' ] ),
        get( '/manage_database/mr_bones', file_service_key = KEYS[ 'trash' ] ),
        get( '/manage_database/mr_bones', file_service_keys = [ KEYS[ 'my_files' ], KEYS[ 'trash' ] ] ),
        get( '/manage_database/mr_bones', tags = [ 'system:archive' ] ),
        post( '/manage_database/force_commit', {} ),
        post( '/manage_database/force_commit', {}, key = 'restricted' ),
        post( '/manage_database/lock_off', {} ),
        post( '/manage_database/lock_on', {}, key = 'restricted' ),
        post( '/manage_database/lock_on', {} ),
        get( '/api_version', key = 'none' ),
        get( '/verify_access_key' ),
        get( '/get_services' ),
        get( '/manage_database/mr_bones' ),
        post( '/manage_database/lock_on', {} ),
        post( '/manage_database/force_commit', {} ),
        post( '/manage_database/lock_off', {}, key = 'restricted' ),
        post( '/manage_database/lock_off', {}, key = 'none' ),
        post( '/manage_database/lock_off', {} ),
        post( '/manage_database/lock_off', {} ),
        get( '/api_version', key = 'none' ),
        get( '/manage_database/mr_bones', tags = [ 'system:inbox' ] ),
    ], read_only = False )

    return [ read, write ]


def main():

    os.makedirs( OUT, exist_ok = True )

    scenarios = [ access_scenarios(), search_scenarios(), search_more_scenarios(), metadata_scenarios(), tag_read_scenarios(), url_read_scenarios(), files_read_scenarios(), relationships_read_scenarios() ] + write_scenarios() + database_scenarios() + [ access_more_scenarios(), file_range_scenarios(), services_scenarios() ]

    for sc in scenarios:

        with open( os.path.join( OUT, sc[ 'name' ] + '.json' ), 'w' ) as f:

            json.dump( sc, f, indent = 1, ensure_ascii = False )
            f.write( '\n' )



    print( f'{len( scenarios )} scenarios, {sum( len( sc[ "steps" ] ) for sc in scenarios )} steps' )


if __name__ == '__main__':

    main()

