#!/usr/bin/env python3
"""Record the reference running export folders.

A fresh client imports a few images and tags them (namespaced and not, with
URLs), then gets three export folders:

- "regular": a regular export of files tagged `export me`, named
  `[series]/[creator] - {hash}` (so into subfolders), with the files' tags
  in `.txt` sidecars and their URLs in `.json` sidecars under
  `{"urls": ...}` (one of which already has other JSON in it);
- "sync": a synchronising export of the same files as symlinks, named
  `{#} {file_id} {nn tags}`, into a folder that already holds other files, an
  empty subfolder, and a file where an export will go;
- "delete after": a regular export of the file tagged `delete me`, named
  `(blue eyes){hash}`, which deletes it from the client afterwards.

The client is saved with the folders set up
(oracle/fixtures/legacy_db/export_folder.tar.gz, with the folders' paths
recorded so the test can rewrite them); then each folder does its work
once. Recorded (oracle/fixtures/export_folder_run.json): every folder's
contents afterwards (sizes, sidecar text, which file a link points at),
which files the client still has, and each folder's stored state. Then
"regular" is run now with a path that doesn't exist, and the popups that
shows are recorded too.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_export_folder.py
"""

import json
import os
import shutil
import sys
import tarfile
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

OUT = os.path.join( HERE, 'fixtures', 'export_folder_run.json' )
DB_OUT = os.path.join( HERE, 'fixtures', 'legacy_db', 'export_folder.tar.gz' )

PORT = 45985

IMAGES = os.path.join( HERE, 'fixtures', 'auto_resolution' )

FILES = [
    ( 'p00_a.png', [ 'export me', 'blue eyes', 'series:metroid', 'creator:someone', 'character:samus aran' ], [ 'https://example.com/post/1' ] ),
    ( 'p01_e.jpg', [ 'export me', 'series:metroid', 'creator:a/b', 'page:2' ], [ 'https://example.com/post/2', 'https://example.com/post/3' ] ),
    ( 'p02_h.gif', [ 'export me', 'ümlaut' ], [] ),
    ( 'p03_a.png', [ 'delete me', 'blue eyes' ], [] ),
    ( 'p04_a.png', [ 'not exported' ], [] ),
]


def setup( s, work ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.exporting import ClientExportingFiles
    from hydrus.client.metadata import ClientMetadataMigration as MM
    from hydrus.client.metadata import ClientMetadataMigrationExporters as E
    from hydrus.client.metadata import ClientMetadataMigrationImporters as I
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchPredicate
    from hydrus.client.search import ClientSearchTagContext
    from hydrus.client import ClientLocation

    my_tags = CC.DEFAULT_LOCAL_TAG_SERVICE_KEY

    hashes = []

    for ( name, tags, urls ) in FILES:

        h = s.api.post( '/add_files/add_file', { 'path' : os.path.join( IMAGES, name ) } )[ 'hash' ]

        s.api.post( '/add_tags/add_tags', { 'hash' : h, 'service_keys_to_tags' : { my_tags.hex() : tags } } )

        if urls:

            s.api.post( '/add_urls/associate_url', { 'hash' : h, 'urls_to_add' : urls } )


        hashes.append( h )


    def search( tag ):

        location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )
        tag_context = ClientSearchTagContext.TagContext( service_key = CC.COMBINED_TAG_SERVICE_KEY )
        predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, tag ) ]

        return ClientSearchFileSearchContext.FileSearchContext( location_context = location_context, tag_context = tag_context, predicates = predicates )


    tags_router = MM.SingleFileMetadataRouter( importers = [ I.SingleFileMetadataImporterMediaTags( service_key = my_tags ) ], exporter = E.SingleFileMetadataExporterTXT() )
    urls_router = MM.SingleFileMetadataRouter( importers = [ I.SingleFileMetadataImporterMediaURLs() ], exporter = E.SingleFileMetadataExporterJSON( nested_object_names = [ 'urls' ] ) )

    folders = [
        ClientExportingFiles.ExportFolder( 'regular', path = os.path.join( work, 'regular' ), export_type = HC.EXPORT_FOLDER_TYPE_REGULAR, file_search_context = search( 'export me' ), metadata_routers = [ tags_router, urls_router ], phrase = '[series]/[creator] - {hash}' ),
        ClientExportingFiles.ExportFolder( 'sync', path = os.path.join( work, 'sync' ), export_type = HC.EXPORT_FOLDER_TYPE_SYNCHRONISE, export_symlinks = True, file_search_context = search( 'export me' ), phrase = '{#} {file_id} {nn tags}' ),
        ClientExportingFiles.ExportFolder( 'delete after', path = os.path.join( work, 'delete' ), export_type = HC.EXPORT_FOLDER_TYPE_REGULAR, delete_from_client_after_export = True, file_search_context = search( 'delete me' ), phrase = '(blue eyes){hash}' ),
    ]

    for folder in folders:

        s.write( 'serialisable', folder )


    HC.options[ 'delete_to_recycle_bin' ] = False

    s.write( 'save_options', HC.options )

    return hashes


def place( work ):
    """The folders, with what is already in them."""

    for name in ( 'regular', 'sync', 'delete' ):

        os.makedirs( os.path.join( work, name ), exist_ok = True )


    os.makedirs( os.path.join( work, 'regular', 'metroid' ) )

    # an existing sidecar with other JSON in it, for the second file
    with open( os.path.join( work, 'regular', 'metroid', 'EXISTING.json' ), 'w' ) as f:

        f.write( '{}' )


    os.makedirs( os.path.join( work, 'sync', 'empty', 'emptier' ) )

    for name in ( 'stale.png', 'stale.png.txt', os.path.join( 'empty', '..', 'other.bin' ) ):

        with open( os.path.join( work, 'sync', name ), 'w' ) as f:

            f.write( 'leftover' )




def run_folders( s ):

    from hydrus.core import HydrusSerialisable

    stored = {}

    for name in sorted( s.read( 'serialisable_names', HydrusSerialisable.SERIALISABLE_TYPE_EXPORT_FOLDER ) ):

        folder = s.read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_EXPORT_FOLDER, name )

        folder.DoWork()

        folder = s.read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_EXPORT_FOLDER, name )

        stored[ name ] = folder.GetSerialisableTuple()


    current = sorted( s.api.get( '/get_files/search_files', tags = json.dumps( [ 'system:everything' ] ), return_hashes = 'true' )[ 'hashes' ] )

    # then "regular" with its folder gone, run now
    folder = s.read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_EXPORT_FOLDER, 'regular' )

    folder._path = folder._path + ' (gone)'
    folder._run_now = True

    broken_popups = hydrus_driver.popups_sent( folder.DoWork )

    return ( stored, current, broken_popups )


def listing( folder ):

    out = {}

    for ( root, dirs, files ) in os.walk( folder ):

        for name in dirs:

            path = os.path.join( root, name )

            if not os.listdir( path ):

                out[ os.path.relpath( path, folder ) + os.sep ] = 'empty dir'



        for name in files:

            path = os.path.join( root, name )

            rel = os.path.relpath( path, folder )

            if os.path.islink( path ):

                out[ rel ] = 'link to ' + os.path.basename( os.readlink( path ) )

            elif name.endswith( '.txt' ) or name.endswith( '.json' ):

                with open( path, 'rb' ) as f:

                    out[ rel ] = f.read().decode( 'utf-8' )


            else:

                out[ rel ] = os.path.getsize( path )




    return dict( sorted( out.items() ) )


def main():

    if len( sys.argv ) > 1:

        ( step, db_dir, work ) = sys.argv[ 1 : 4 ]

        if step == 'setup':

            hashes = hydrus_driver.run_client( db_dir, lambda s: setup( s, work ), port = PORT )

            with open( os.path.join( db_dir, 'setup.json' ), 'w' ) as f:

                json.dump( hashes, f )



        elif step == 'run':

            ( stored, current, broken_popups ) = hydrus_driver.run_client( db_dir, run_folders, port = PORT )

            with open( os.path.join( db_dir, 'run.json' ), 'w' ) as f:

                json.dump( { 'stored' : stored, 'current' : current, 'broken_popups' : broken_popups }, f )




        return


    work = tempfile.mkdtemp( prefix = 'hydrus_export_folder_' )

    try:

        db_dir = os.path.join( work, 'db' )

        hydrus_driver.run_in_subprocess( __file__, 'setup', db_dir, work )

        with open( os.path.join( db_dir, 'setup.json' ) ) as f:

            hashes = json.load( f )


        os.remove( os.path.join( db_dir, 'setup.json' ) )

        with tarfile.open( DB_OUT, 'w:gz' ) as tar:

            for name in sorted( os.listdir( db_dir ) ):

                if name.endswith( '.db' ) or name == 'client_files':

                    tar.add( os.path.join( db_dir, name ), arcname = name )




        place( work )

        hydrus_driver.run_in_subprocess( __file__, 'run', db_dir, work )

        with open( os.path.join( db_dir, 'run.json' ) ) as f:

            run = json.load( f )


        result = {
            'work' : work,
            'hashes' : hashes,
            'current' : run[ 'current' ],
            'stored' : run[ 'stored' ],
            'broken_popups' : run[ 'broken_popups' ],
            'folders' : { name : listing( os.path.join( work, name ) ) for name in ( 'regular', 'sync', 'delete' ) },
        }

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
