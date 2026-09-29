#!/usr/bin/env python3
"""Build a reference (v688) client database with representative content.

Boots the real Python client headlessly on a fresh db dir, then populates it
through the same paths real use does: the Client API for everything it can
do, and the controller's own Write commands for the rest (extra services,
siblings, parents, display application, similar-file search).

The result is packed as oracle/fixtures/legacy_db/<name>.tar.gz along with a
JSON manifest describing what was put in, which conformance scenarios and the
Rust importer tests use.

Usage: python oracle/make_fixture_db.py [--name basic]
"""

import argparse
import hashlib
import json
import os
import shutil
import sys
import tarfile
import tempfile

sys.path.insert( 0, os.path.dirname( __file__ ) )

import hydrus_driver

HERE = os.path.dirname( os.path.abspath( __file__ ) )
MEDIA_DIR = os.path.join( HERE, 'fixtures', 'import_media' )
OUT_DIR = os.path.join( HERE, 'fixtures', 'legacy_db' )

PORT = 45901

# keys for services this script creates; fixed so scenarios can name them
SECOND_TAGS_KEY = bytes.fromhex( '5ec0' * 16 )
ART_DOMAIN_KEY = bytes.fromhex( 'a270' * 16 )
STARS_KEY = bytes.fromhex( '57a2' * 16 )
COUNTER_KEY = bytes.fromhex( 'c0de' * 16 )

TAG_VOCAB = [
    'blue eyes', 'blue_eyes', 'bleu eyes', 'red hair', 'smile', 'outdoors', 'indoors', 'safe', 'explicit',
    'character:samus aran', 'samus', 'series:metroid', 'character:link', 'series:the legend of zelda',
    'creator:anonymous artist', 'creator:someone else', 'meta:highres', 'meta:lowres',
    'page:1', 'page:2', 'page:3', 'page:10', 'page:11', 'chapter:2', 'volume:3',
    'title:a test image: with colons', '::)', 'キャラクター:初音ミク', 'éclair', 'multi word tag with spaces',
]


def sha256_of( path ):

    h = hashlib.sha256()

    with open( path, 'rb' ) as f:

        h.update( f.read() )


    return h.hexdigest()


def tags_for( index, name ):
    """Deterministic, varied tag sets."""

    tags = { TAG_VOCAB[ ( index * 7 + k * 3 ) % len( TAG_VOCAB ) ] for k in range( index % 5 + 1 ) }

    if name.startswith( 'jpeg' ):

        tags.add( 'format:jpeg' )


    if index % 3 == 0:

        tags.add( 'safe' )


    return sorted( tags )


def populate( s ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientServices
    from hydrus.client.metadata import ClientContentUpdates
    from hydrus.core import HydrusConstants as HC

    api = s.api

    my_tags = CC.DEFAULT_LOCAL_TAG_SERVICE_KEY.hex()
    downloader_tags = CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY.hex()
    my_files = CC.LOCAL_FILE_SERVICE_KEY.hex()
    all_local_files = CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY.hex()
    favourites = CC.DEFAULT_FAVOURITES_RATING_SERVICE_KEY.hex()

    # --- extra services --------------------------------------------------

    services = list( s.controller.services_manager.GetServices() )

    services.append( ClientServices.GenerateService( SECOND_TAGS_KEY, HC.LOCAL_TAG, 'second tags' ) )
    services.append( ClientServices.GenerateService( ART_DOMAIN_KEY, HC.LOCAL_FILE_DOMAIN, 'art' ) )

    stars_dictionary = ClientServices.GenerateDefaultServiceDictionary( HC.LOCAL_RATING_NUMERICAL )
    stars_dictionary[ 'num_stars' ] = 5
    stars_dictionary[ 'allow_zero' ] = True
    services.append( ClientServices.GenerateService( STARS_KEY, HC.LOCAL_RATING_NUMERICAL, 'stars', stars_dictionary ) )

    services.append( ClientServices.GenerateService( COUNTER_KEY, HC.LOCAL_RATING_INCDEC, 'counter' ) )

    s.write( 'update_services', services )

    # --- files -----------------------------------------------------------

    names = sorted( os.listdir( MEDIA_DIR ) )
    files = []

    for name in names:

        path = os.path.join( MEDIA_DIR, name )

        result = api.post( '/add_files/add_file', { 'path': path } )

        files.append( { 'name': name, 'hash': result[ 'hash' ], 'import_status': result[ 'status' ], 'note': result.get( 'note', '' ) } )


    imported = [ f for f in files if f[ 'import_status' ] in ( 1, 2 ) ]
    hashes = [ f[ 'hash' ] for f in imported ]

    # --- tags ------------------------------------------------------------

    for ( i, f ) in enumerate( imported ):

        service_keys_to_tags = { my_tags: tags_for( i, f[ 'name' ] ) }

        if i % 2 == 0:

            service_keys_to_tags[ downloader_tags ] = [ 'filename:' + os.path.splitext( f[ 'name' ] )[0], 'source:fixture' ]


        if i % 4 == 1:

            service_keys_to_tags[ SECOND_TAGS_KEY.hex() ] = [ 'second:tag', 'blue eyes' ]


        api.post( '/add_tags/add_tags', { 'hash': f[ 'hash' ], 'service_keys_to_tags': service_keys_to_tags } )


    # a deleted mapping, so deleted-mapping records exist
    api.post( '/add_tags/add_tags', { 'hash': hashes[ 0 ], 'service_keys_to_actions_to_tags': { my_tags: { str( HC.CONTENT_UPDATE_DELETE ): tags_for( 0, imported[ 0 ][ 'name' ] )[ :1 ] } } } )

    # --- siblings and parents (not available through the API) -----------

    def write_relations( service_key, content_type, pairs ):

        content_updates = [ ClientContentUpdates.ContentUpdate( content_type, HC.CONTENT_UPDATE_ADD, pair ) for pair in pairs ]

        s.write( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( service_key, content_updates ) )


    write_relations( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, HC.CONTENT_TYPE_TAG_SIBLINGS, [
        ( 'blue_eyes', 'blue eyes' ),
        ( 'bleu eyes', 'blue_eyes' ),  # chain: bleu eyes -> blue_eyes -> blue eyes
        ( 'samus', 'character:samus aran' ),
        ( 'meta:lowres', 'meta:low resolution' ),
    ] )

    write_relations( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, HC.CONTENT_TYPE_TAG_PARENTS, [
        ( 'character:samus aran', 'series:metroid' ),
        ( 'series:metroid', 'studio:nintendo' ),  # grandparent
        ( 'character:link', 'series:the legend of zelda' ),
        ( 'series:the legend of zelda', 'studio:nintendo' ),
    ] )

    write_relations( SECOND_TAGS_KEY, HC.CONTENT_TYPE_TAG_SIBLINGS, [ ( 'second:tag', 'second:canonical' ) ] )

    # "my tags" siblings and parents also apply to "downloader tags"
    ( siblings_application, parents_application ) = s.read( 'tag_display_application' )

    siblings_application[ CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY ] = [ CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY, CC.DEFAULT_LOCAL_TAG_SERVICE_KEY ]
    parents_application[ CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY ] = [ CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY, CC.DEFAULT_LOCAL_TAG_SERVICE_KEY ]

    s.write( 'tag_display_application', siblings_application, parents_application )

    s.sync_tag_display()

    # --- file domains, inbox, trash, deletion ----------------------------

    api.post( '/add_files/migrate_files', { 'hashes': hashes[ 0:8 ], 'file_service_key': ART_DOMAIN_KEY.hex() } )
    api.post( '/add_files/archive_files', { 'hashes': hashes[ ::2 ] } )

    api.post( '/add_files/delete_files', { 'hashes': hashes[ -6:-3 ], 'reason': 'fixture: trashed' } )
    api.post( '/add_files/delete_files', { 'hashes': hashes[ -3: ], 'file_service_key': all_local_files, 'reason': 'fixture: fully deleted' } )

    trashed_then_restored = hashes[ -6 ]
    api.post( '/add_files/undelete_files', { 'hash': trashed_then_restored } )

    # only in "art": remove one of the migrated files from my files
    api.post( '/add_files/delete_files', { 'hash': hashes[ 1 ], 'file_service_key': my_files } )

    # --- urls, notes, ratings, times --------------------------------------

    for ( i, h ) in enumerate( hashes[ :10 ] ):

        urls = [ f'https://example.com/post/{i}', f'https://danbooru.donmai.us/posts/{1000 + i}' ]

        if i % 3 == 0:

            urls.append( f'https://gelbooru.com/index.php?page=post&s=view&id={2000 + i}' )


        api.post( '/add_urls/associate_url', { 'hash': h, 'urls_to_add': urls } )


    api.post( '/add_notes/set_notes', { 'hash': hashes[ 2 ], 'notes': { 'comment': 'a short note', 'translation': 'line one\nline two\n\nunicode: 初音ミク' } } )
    api.post( '/add_notes/set_notes', { 'hash': hashes[ 3 ], 'notes': { 'comment': 'another note' } } )

    for ( i, h ) in enumerate( hashes[ :12 ] ):

        api.post( '/edit_ratings/set_rating', { 'hash': h, 'rating_service_key': favourites, 'rating': i % 2 == 0 } )
        api.post( '/edit_ratings/set_rating', { 'hash': h, 'rating_service_key': STARS_KEY.hex(), 'rating': i % 6 } )

        if i % 3 == 0:

            api.post( '/edit_ratings/set_rating', { 'hash': h, 'rating_service_key': COUNTER_KEY.hex(), 'rating': i } )



    for ( i, h ) in enumerate( hashes[ :6 ] ):

        api.post( '/edit_times/increment_file_viewtime', { 'hash': h, 'canvas_type': CC.CANVAS_MEDIA_VIEWER, 'views': i + 1, 'viewtime': 2.5 * ( i + 1 ), 'timestamp_ms': 1_700_000_000_000 + i } )
        api.post( '/edit_times/increment_file_viewtime', { 'hash': h, 'canvas_type': CC.CANVAS_PREVIEW, 'views': 1, 'viewtime': 0.5, 'timestamp_ms': 1_700_000_100_000 + i } )


    for ( i, h ) in enumerate( hashes[ :4 ] ):

        api.post( '/edit_times/set_time', { 'hash': h, 'timestamp_ms': 1_600_000_000_000 + i * 86_400_000, 'timestamp_type': HC.TIMESTAMP_TYPE_MODIFIED_DOMAIN, 'domain': 'example.com' } )


    # --- duplicates -------------------------------------------------------

    by_name = { f[ 'name' ]: f[ 'hash' ] for f in imported }

    api.post( '/manage_file_relationships/set_file_relationships', { 'relationships': [
        { 'hash_a': by_name[ 'jpeg_02.jpg' ], 'hash_b': by_name[ 'dupe_jpeg_02_small.jpg' ], 'relationship': 4, 'do_default_content_merge': False },
        { 'hash_a': by_name[ 'png_00.png' ], 'hash_b': by_name[ 'png_01.png' ], 'relationship': 3, 'do_default_content_merge': False },
        { 'hash_a': by_name[ 'png_02.png' ], 'hash_b': by_name[ 'png_03.png' ], 'relationship': 1, 'do_default_content_merge': False },
    ] } )

    # let the similar-files system find the remaining near-duplicates of jpeg_02
    s.write( 'maintain_similar_files_search_for_potential_duplicates', 8 )

    s.sync_tag_display()

    return { 'files': files }


def pack( db_dir, name, manifest ):

    os.makedirs( OUT_DIR, exist_ok = True )

    tar_path = os.path.join( OUT_DIR, name + '.tar.gz' )

    keep = [ 'client.db', 'client.caches.db', 'client.mappings.db', 'client.master.db', 'client_files' ]

    for leftover in ( '-wal', '-shm' ):

        for db_name in keep[ :4 ]:

            if os.path.exists( os.path.join( db_dir, db_name + leftover ) ):

                raise Exception( f'{db_name}{leftover} still exists; client did not shut down cleanly' )




    with tarfile.open( tar_path, 'w:gz' ) as tar:

        for entry in keep:

            tar.add( os.path.join( db_dir, entry ), arcname = entry, filter = lambda ti: normalise_tarinfo( ti ) )



    with open( os.path.join( OUT_DIR, name + '.manifest.json' ), 'w' ) as f:

        json.dump( manifest, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {tar_path} ({os.path.getsize( tar_path )} bytes)' )


def normalise_tarinfo( ti ):

    ti.uid = ti.gid = 0
    ti.uname = ti.gname = ''

    return ti


def main():

    parser = argparse.ArgumentParser()
    parser.add_argument( '--name', default = 'basic' )
    parser.add_argument( '--keep', help = 'also copy the finished db dir here' )
    args = parser.parse_args()

    work = tempfile.mkdtemp( prefix = 'hydrus_fixture_' )
    db_dir = os.path.join( work, 'db' )

    result = hydrus_driver.run_client( db_dir, populate, port = PORT )

    manifest = {
        'reference_version': 688,
        'service_keys': {
            'second_tags': SECOND_TAGS_KEY.hex(),
            'art': ART_DOMAIN_KEY.hex(),
            'stars': STARS_KEY.hex(),
            'counter': COUNTER_KEY.hex(),
        },
        'access_keys': {
            'full': hydrus_driver.ORACLE_ACCESS_KEY.hex(),
            'restricted': hydrus_driver.ORACLE_RESTRICTED_ACCESS_KEY.hex(),
        },
        'files': result[ 'files' ],
    }

    pack( db_dir, args.name, manifest )

    if args.keep:

        shutil.copytree( db_dir, args.keep, dirs_exist_ok = True )


    shutil.rmtree( work )


if __name__ == '__main__':

    main()

