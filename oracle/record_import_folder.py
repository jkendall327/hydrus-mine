#!/usr/bin/env python3
"""Record the reference running an import folder.

A folder of files with sidecars beside them: tags in `.txt` sidecars, URLs
in `.json` sidecars (read with a formula), a note in a suffixed `.txt`
sidecar (under a forced name), a modified time in another, and filename
tagging (the filename, the directory, a regex, a tag for all). One file is
already in the client (so it is redundant), one is not a file hydrus can
import, one was modified too recently to be taken, and some are in a
subfolder. New files are moved to another folder with their sidecars,
redundant ones are deleted with theirs, and errors are left.

A fresh client is set up with the folder (and the one file) and saved
(oracle/fixtures/legacy_db/import_folder.tar.gz, with the folder's path
recorded so the test can rewrite it); then the folder does its work once.
Recorded (oracle/fixtures/import_folder_run.json): each file seed's path
(relative to the folder), status and note; each imported file's storage
tags per service, URLs, notes and modified time; and both folders' contents
afterwards. The source files are in oracle/fixtures/import_folder/.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_folder.py
"""

import json
import os
import shutil
import sys
import tarfile
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

SOURCE = os.path.join( HERE, 'fixtures', 'import_folder' )
OUT = os.path.join( HERE, 'fixtures', 'import_folder_run.json' )
DB_OUT = os.path.join( HERE, 'fixtures', 'legacy_db', 'import_folder.tar.gz' )

PORT = 45983
NAME = 'drop box'
OLD_MTIME = 1_600_000_000


def generate():
    """The folder's contents, from the auto-resolution fixture images."""

    if os.path.exists( SOURCE ):

        shutil.rmtree( SOURCE )


    images = os.path.join( HERE, 'fixtures', 'auto_resolution' )

    files = {
        'a.png' : 'p00_a.png',
        'b p12.jpg' : 'p01_e.jpg',
        'dupe.gif' : 'p02_h.gif',
        os.path.join( 'sub', 'c.png' ) : 'p03_a.png',
        os.path.join( 'sub', 'deeper', 'd.bmp' ) : 'p00_c.bmp',
        'recent.png' : 'p04_a.png',
    }

    for ( name, source ) in files.items():

        path = os.path.join( SOURCE, name )

        os.makedirs( os.path.dirname( path ), exist_ok = True )

        shutil.copy( os.path.join( images, source ), path )


    texts = {
        'a.png.txt' : 'blue eyes\ncharacter:Samus Aran\n\npage 2\r\npage 10\n',
        'a.png.json' : json.dumps( { 'urls' : [ 'https://example.com/post/1', 'https://example.com/post/1', 'not a url', 'https://example.com/a b' ], 'other' : [ 'x' ] } ),
        'a.png.time.txt' : '1500000000',
        'b p12.jpg.txt' : 'series:metroid',
        'b p12.notes.txt' : 'the first line\nthe second line',
        os.path.join( 'sub', 'c.png.txt' ) : 'ümlaut\n::)\n',
        'dupe.gif.txt' : 'should not matter',
        'notes.txt' : 'a text file with no image, so not a sidecar',
    }

    for ( name, text ) in texts.items():

        with open( os.path.join( SOURCE, name ), 'w', encoding = 'utf-8', newline = '' ) as f:

            f.write( text )




def place( folder ):
    """Copy the source files to `folder`, with settled times except the
    recent file."""

    shutil.copytree( SOURCE, folder )

    for ( root, dirs, files ) in os.walk( folder ):

        for name in files:

            path = os.path.join( root, name )

            mtime = time.time() if name == 'recent.png' else OLD_MTIME

            os.utime( path, ( mtime, mtime ) )




def make_folder( s, folder, moved ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientStrings
    from hydrus.client import ClientTime
    from hydrus.client.importing import ClientImportLocal
    from hydrus.client.importing.options import FilenameTaggingOptions
    from hydrus.client.metadata import ClientMetadataMigration as MM
    from hydrus.client.metadata import ClientMetadataMigrationExporters as E
    from hydrus.client.metadata import ClientMetadataMigrationImporters as I
    from hydrus.client.parsing import ClientParsing as P

    my_tags = CC.DEFAULT_LOCAL_TAG_SERVICE_KEY

    routers = [
        MM.SingleFileMetadataRouter( importers = [ I.SingleFileMetadataImporterTXT() ], exporter = E.SingleFileMetadataExporterMediaTags( my_tags ) ),
        MM.SingleFileMetadataRouter(
            importers = [ I.SingleFileMetadataImporterJSON( json_parsing_formula = P.ParseFormulaJSON( parse_rules = [ ( P.JSON_PARSE_RULE_TYPE_DICT_KEY, ClientStrings.StringMatch( match_type = ClientStrings.STRING_MATCH_FIXED, match_value = 'urls', example_string = 'urls' ) ), ( P.JSON_PARSE_RULE_TYPE_ALL_ITEMS, None ) ], content_to_fetch = P.JSON_CONTENT_STRING ) ) ],
            exporter = E.SingleFileMetadataExporterMediaURLs()
        ),
        MM.SingleFileMetadataRouter( importers = [ I.SingleFileMetadataImporterTXT( remove_actual_filename_ext = True, suffix = 'notes' ) ], exporter = E.SingleFileMetadataExporterMediaNotes( forced_name = 'comment' ) ),
        MM.SingleFileMetadataRouter( importers = [ I.SingleFileMetadataImporterTXT( suffix = 'time' ) ], exporter = E.SingleFileMetadataExporterMediaTimestamps( ClientTime.TimestampData.STATICSimpleStub( HC.TIMESTAMP_TYPE_MODIFIED_FILE ) ) ),
    ]

    tagging = FilenameTaggingOptions.FilenameTaggingOptions()

    directories = { index : ( False, '' ) for index in ( 0, 1, 2, -3, -2, -1 ) }
    directories[ -1 ] = ( True, 'folder' )

    tagging.SimpleSetTuple( { 'from the drop box' }, ( True, 'filename' ), directories )
    tagging.AdvancedSetTuple( [ ( 'page', r'p(\d+)\.\w+$' ) ], [ r'(\w+)\.bmp' ] )

    actions = {
        CC.STATUS_SUCCESSFUL_AND_NEW : CC.IMPORT_FOLDER_MOVE,
        CC.STATUS_SUCCESSFUL_BUT_REDUNDANT : CC.IMPORT_FOLDER_DELETE,
        CC.STATUS_DELETED : CC.IMPORT_FOLDER_IGNORE,
        CC.STATUS_ERROR : CC.IMPORT_FOLDER_IGNORE,
    }

    import_folder = ClientImportLocal.ImportFolder(
        NAME,
        path = folder,
        metadata_routers = routers,
        tag_service_keys_to_filename_tagging_options = { my_tags : tagging },
        actions = actions,
        action_locations = { CC.STATUS_SUCCESSFUL_AND_NEW : moved },
        period = 3600,
        check_regularly = True,
    )

    s.write( 'serialisable', import_folder )

    # deletions go straight away, not to the recycle bin
    HC.options[ 'delete_to_recycle_bin' ] = False

    s.write( 'save_options', HC.options )

    # the duplicate is already in the client
    return s.api.post( '/add_files/add_file', { 'path' : os.path.join( folder, 'dupe.gif' ) } )[ 'hash' ]


def run_folder( s ):

    from hydrus.core import HydrusSerialisable

    import_folder = s.read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_IMPORT_FOLDER, NAME )

    import_folder.DoWork()

    import_folder = s.read( 'serialisable_named', HydrusSerialisable.SERIALISABLE_TYPE_IMPORT_FOLDER, NAME )

    seeds = [ ( f.file_seed_data, f.status, f.note, f.GetHash().hex() if f.HasHash() else None ) for f in import_folder.GetFileSeedCache().GetFileSeeds() ]

    # moved and deleted files leave the seed cache, so take every file
    hashes = sorted( s.api.get( '/get_files/search_files', tags = json.dumps( [ 'system:everything' ] ), return_hashes = 'true' )[ 'hashes' ] )

    metadata = s.api.get( '/get_files/file_metadata', hashes = json.dumps( hashes ), include_notes = 'true' )[ 'metadata' ] if hashes else []

    return ( seeds, metadata, import_folder.GetSerialisableTuple() )


def listing( folder ):

    out = {}

    for ( root, dirs, files ) in os.walk( folder ):

        for name in files:

            path = os.path.join( root, name )

            with open( path, 'rb' ) as f:

                out[ os.path.relpath( path, folder ) ] = len( f.read() )




    return dict( sorted( out.items() ) )


def main():

    if len( sys.argv ) > 1:

        ( step, db_dir, folder, moved ) = sys.argv[ 1 : 5 ]

        if step == 'setup':

            h = hydrus_driver.run_client( db_dir, lambda s: make_folder( s, folder, moved ), port = PORT )

            with open( os.path.join( db_dir, 'setup.json' ), 'w' ) as f:

                json.dump( { 'dupe' : h }, f )



        elif step == 'run':

            ( seeds, metadata, stored ) = hydrus_driver.run_client( db_dir, run_folder, port = PORT )

            with open( os.path.join( db_dir, 'run.json' ), 'w' ) as f:

                json.dump( { 'seeds' : seeds, 'metadata' : metadata, 'stored' : stored }, f )




        return


    generate()

    work = tempfile.mkdtemp( prefix = 'hydrus_import_folder_' )

    try:

        db_dir = os.path.join( work, 'db' )
        folder = os.path.join( work, 'in' )
        moved = os.path.join( work, 'moved' )

        place( folder )

        os.makedirs( moved )

        hydrus_driver.run_in_subprocess( __file__, 'setup', db_dir, folder, moved )

        with open( os.path.join( db_dir, 'setup.json' ) ) as f:

            setup = json.load( f )


        os.remove( os.path.join( db_dir, 'setup.json' ) )

        with tarfile.open( DB_OUT, 'w:gz' ) as tar:

            for name in sorted( os.listdir( db_dir ) ):

                if name.endswith( '.db' ) or name == 'client_files':

                    tar.add( os.path.join( db_dir, name ), arcname = name )




        # the recent file must stay recent for the run
        os.utime( os.path.join( folder, 'recent.png' ) )

        hydrus_driver.run_in_subprocess( __file__, 'run', db_dir, folder, moved )

        with open( os.path.join( db_dir, 'run.json' ) ) as f:

            run = json.load( f )


        result = {
            'folder' : folder,
            'moved' : moved,
            'dupe' : setup[ 'dupe' ],
            'seeds' : [ [ os.path.relpath( path, folder ), status, note, h ] for ( path, status, note, h ) in run[ 'seeds' ] ],
            'metadata' : run[ 'metadata' ],
            'in_after' : listing( folder ),
            'moved_after' : listing( moved ),
        }

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "seeds" ] )} seeds' )


if __name__ == '__main__':

    main()
