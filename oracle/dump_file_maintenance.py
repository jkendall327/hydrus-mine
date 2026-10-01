#!/usr/bin/env python3
"""Record what the reference's file maintenance does to damaged files.

On a fresh copy of the basic fixture, each scenario damages one file
before the client boots (wrong metadata in the database, a file renamed to
the wrong extension, a forced filetype, lost hashes, garbage or extra bytes
in the file, a missing file, a wrong-size or missing thumbnail, a stray
copy beside the file), then the client queues the scenario's job for it.
With the archived-file delete lock on and the background maintenance off,
`ForceMaintenance` runs just those job types (their follow-up jobs stay
queued, so they can be seen), with the GUI's `ImportURL` (which the
integrity checks call to redownload a file) recording its calls.

Afterwards, for each scenario's file: its row in files_info, its forced
filetype, which of its files and thumbnail are on disk (and the
thumbnail's size), its queued jobs, its local domains and deletion
records, its inbox state, and whether it still has its other hashes and
modified time. Then what `missing_and_invalid_files` holds, and the URLs
sent to `ImportURL`, with how the downloader normalises them.

The edits are written out declaratively, so a port can make the same ones.

Usage: python oracle/dump_file_maintenance.py      (writes fixtures/file_maintenance.json)
"""

import json
import os
import re
import shutil
import sqlite3
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'file_maintenance.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )
BY_NAME = { f[ 'name' ]: f[ 'hash' ] for f in MANIFEST[ 'files' ] }

PORT = 45962

# the local file services in the basic fixture
DOMAINS = ( 'hydrus local file storage', 'combined local file domains', 'my files', 'trash', 'art' )

GARBAGE = b'this is not a file hydrus can read\n' * 64

# the reference's job codes (ClientFilesMaintenance)
FILE_METADATA = 0
REFIT_THUMBNAIL = 2
DELETE_NEIGHBOUR_DUPES = 4
PRESENCE_REMOVE_RECORD = 5
DATA_REMOVE_RECORD = 6
PRESENCE_TRY_URL = 11
DATA_TRY_URL = 12
DATA_SILENT_DELETE = 13
PRESENCE_TRY_URL_ELSE_REMOVE_RECORD = 14
DATA_TRY_URL_ELSE_REMOVE_RECORD = 15
PRESENCE_LOG_ONLY = 18
PRESENCE_DELETE_RECORD = 21

SCENARIOS = [
    { 'name': 'wrong_resolution', 'file': 'jpeg_03.jpg', 'edits': [ { 'op': 'set_info', 'width': 1, 'height': 1 } ], 'job': FILE_METADATA },
    { 'name': 'wrong_type', 'file': 'png_01.png', 'edits': [ { 'op': 'set_info', 'mime': 1 }, { 'op': 'rename_file', 'ext': '.jpg' } ], 'job': FILE_METADATA },
    { 'name': 'forced_type', 'file': 'png_03.png', 'edits': [ { 'op': 'force_mime', 'mime': 1 }, { 'op': 'rename_file', 'ext': '.jpg' } ], 'job': FILE_METADATA },
    { 'name': 'lost_hashes', 'file': 'jpeg_04.jpg', 'edits': [ { 'op': 'forget_hashes' }, { 'op': 'forget_modified' } ], 'job': FILE_METADATA },
    { 'name': 'unreadable_with_urls', 'file': 'audio.mp3', 'edits': [ { 'op': 'garbage' } ], 'job': FILE_METADATA },
    { 'name': 'unreadable_without_urls', 'file': 'jpeg_06.jpg', 'edits': [ { 'op': 'garbage' } ], 'job': FILE_METADATA },
    { 'name': 'missing_log_only', 'file': 'jpeg_08.jpg', 'edits': [ { 'op': 'remove_file' } ], 'job': PRESENCE_LOG_ONLY },
    { 'name': 'missing_remove_record', 'file': 'jpeg_10.jpg', 'edits': [ { 'op': 'remove_file' } ], 'job': PRESENCE_REMOVE_RECORD },
    { 'name': 'missing_delete_record', 'file': 'png_00.png', 'edits': [ { 'op': 'remove_file' } ], 'job': PRESENCE_DELETE_RECORD },
    { 'name': 'missing_but_locked', 'file': 'jpeg_11.jpg', 'edits': [ { 'op': 'remove_file' } ], 'job': PRESENCE_REMOVE_RECORD },
    { 'name': 'missing_try_url', 'file': 'bitmap.bmp', 'edits': [ { 'op': 'remove_file' } ], 'job': PRESENCE_TRY_URL },
    { 'name': 'missing_try_url_else_remove', 'file': 'document.pdf', 'edits': [ { 'op': 'remove_file' } ], 'job': PRESENCE_TRY_URL_ELSE_REMOVE_RECORD },
    { 'name': 'missing_no_url_else_remove', 'file': 'png_02.png', 'edits': [ { 'op': 'remove_file' } ], 'job': PRESENCE_TRY_URL_ELSE_REMOVE_RECORD },
    { 'name': 'intact_data_check', 'file': 'png_alpha_01.png', 'edits': [], 'job': DATA_REMOVE_RECORD },
    { 'name': 'invalid_silent_delete', 'file': 'png_alpha_00.png', 'edits': [ { 'op': 'append_byte' } ], 'job': DATA_SILENT_DELETE },
    { 'name': 'invalid_try_url', 'file': 'dupe_jpeg_02_q60.jpg', 'edits': [ { 'op': 'append_byte' } ], 'job': DATA_TRY_URL },
    { 'name': 'invalid_remove_record', 'file': 'png_alpha_02.png', 'edits': [ { 'op': 'append_byte' } ], 'job': DATA_REMOVE_RECORD },
    { 'name': 'invalid_try_url_else_remove', 'file': 'gif_animated.gif', 'edits': [ { 'op': 'append_byte' } ], 'job': DATA_TRY_URL_ELSE_REMOVE_RECORD },
    { 'name': 'refit_wrong_size', 'file': 'jpeg_02.jpg', 'edits': [ { 'op': 'copy_thumbnail', 'from': 'jpeg_00.jpg' } ], 'job': REFIT_THUMBNAIL },
    { 'name': 'refit_right_size', 'file': 'jpeg_05.jpg', 'edits': [], 'job': REFIT_THUMBNAIL },
    { 'name': 'refit_missing', 'file': 'jpeg_07.jpg', 'edits': [ { 'op': 'remove_thumbnail' } ], 'job': REFIT_THUMBNAIL },
    { 'name': 'refit_no_thumbnail_type', 'file': 'audio.flac', 'edits': [], 'job': REFIT_THUMBNAIL },
    { 'name': 'neighbour_dupes', 'file': 'jpeg_09.jpg', 'edits': [ { 'op': 'copy_file', 'ext': '.png' }, { 'op': 'copy_file', 'ext': '.gif' } ], 'job': DELETE_NEIGHBOUR_DUPES },
]


def find( db_dir, prefix, kind ):
    """Paths under client_files of `kind` ('f' or 't') whose names start with `prefix`."""

    out = []

    for ( root, dirs, files ) in os.walk( os.path.join( db_dir, 'client_files' ) ):

        top = os.path.relpath( root, os.path.join( db_dir, 'client_files' ) ).split( os.sep )[0]

        if not top.startswith( kind ):

            continue


        out.extend( os.path.join( root, f ) for f in files if f.startswith( prefix ) )


    return sorted( out )


def apply_edits( db_dir ):

    c = sqlite3.connect( os.path.join( db_dir, 'client.db' ) )
    c.execute( f"ATTACH '{db_dir}/client.master.db' AS master" )

    def hash_id( name ):

        return c.execute( 'SELECT hash_id FROM master.hashes WHERE hash = ?', ( bytes.fromhex( BY_NAME[ name ] ), ) ).fetchone()[0]


    for sc in SCENARIOS:

        name = sc[ 'file' ]
        hex_hash = BY_NAME[ name ]
        hid = hash_id( name )

        # (the stored file, before any edit adds another)
        [ path ] = find( db_dir, hex_hash, 'f' )

        for edit in sc[ 'edits' ]:

            op = edit[ 'op' ]

            if op == 'set_info':

                for column in ( 'width', 'height', 'mime' ):

                    if column in edit:

                        c.execute( f'UPDATE files_info SET {column} = ? WHERE hash_id = ?', ( edit[ column ], hid ) )



            elif op == 'force_mime':

                c.execute( 'INSERT INTO files_info_forced_filetypes ( hash_id, forced_mime ) VALUES ( ?, ? )', ( hid, edit[ 'mime' ] ) )

            elif op == 'forget_hashes':

                c.execute( 'DELETE FROM master.local_hashes WHERE hash_id = ?', ( hid, ) )

            elif op == 'forget_modified':

                c.execute( 'DELETE FROM file_modified_timestamps WHERE hash_id = ?', ( hid, ) )

            elif op == 'rename_file':

                renamed = os.path.splitext( path )[0] + edit[ 'ext' ]

                os.rename( path, renamed )

                path = renamed

            elif op == 'copy_file':

                shutil.copyfile( path, os.path.splitext( path )[0] + edit[ 'ext' ] )

            elif op == 'garbage':

                with open( path, 'wb' ) as f:

                    f.write( GARBAGE )


            elif op == 'append_byte':

                with open( path, 'ab' ) as f:

                    f.write( b'\x00' )


            elif op == 'remove_file':

                os.remove( path )

            elif op == 'copy_thumbnail':

                [ source ] = find( db_dir, BY_NAME[ edit[ 'from' ] ], 't' )
                [ dest ] = find( db_dir, hex_hash, 't' )

                shutil.copyfile( source, dest )

            elif op == 'remove_thumbnail':

                [ dest ] = find( db_dir, hex_hash, 't' )

                os.remove( dest )

            else:

                raise Exception( op )




    c.commit()
    c.close()


def snapshot( db_dir ):

    from PIL import Image

    c = sqlite3.connect( f'file:{db_dir}/client.db?mode=ro', uri = True )
    c.execute( f"ATTACH 'file:{db_dir}/client.master.db?mode=ro' AS master" )
    c.execute( f"ATTACH 'file:{db_dir}/client.caches.db?mode=ro' AS caches" )

    services = { name: service_id for ( service_id, name ) in c.execute( 'SELECT service_id, name FROM services' ) }

    def one( sql, *args ):

        row = c.execute( sql, args ).fetchone()

        return None if row is None else row[0]


    out = {}

    for sc in SCENARIOS:

        hex_hash = BY_NAME[ sc[ 'file' ] ]

        hid = one( 'SELECT hash_id FROM master.hashes WHERE hash = ?', bytes.fromhex( hex_hash ) )

        info = c.execute( 'SELECT size, mime, width, height, duration, num_frames, has_audio, num_words FROM files_info WHERE hash_id = ?', ( hid, ) ).fetchone()

        thumbnails = find( db_dir, hex_hash, 't' )

        thumbnail_size = None

        if len( thumbnails ) > 0:

            with Image.open( thumbnails[0] ) as image:

                thumbnail_size = list( image.size )



        jobs = sorted( ( job_type, time_can_start <= int( time.time() ) ) for ( job_type, time_can_start ) in c.execute( 'SELECT job_type, time_can_start FROM caches.file_maintenance_jobs WHERE hash_id = ?', ( hid, ) ) )

        out[ sc[ 'name' ] ] = {
            'info': list( info ),
            'forced_mime': one( 'SELECT forced_mime FROM files_info_forced_filetypes WHERE hash_id = ?', hid ),
            'files': sorted( os.path.basename( p )[ len( hex_hash ) : ] for p in find( db_dir, hex_hash, 'f' ) ),
            'thumbnail': thumbnail_size,
            'jobs': [ { 'job': job, 'due': due } for ( job, due ) in jobs ],
            'current': sorted( d for d in DOMAINS if one( f'SELECT 1 FROM current_files_{services[ d ]} WHERE hash_id = ?', hid ) ),
            'deleted': sorted( d for d in DOMAINS if one( f'SELECT 1 FROM deleted_files_{services[ d ]} WHERE hash_id = ?', hid ) ),
            'inbox': one( 'SELECT 1 FROM file_inbox WHERE hash_id = ?', hid ) is not None,
            'other_hashes': one( 'SELECT 1 FROM master.local_hashes WHERE hash_id = ?', hid ) is not None,
            'modified': one( 'SELECT 1 FROM file_modified_timestamps WHERE hash_id = ?', hid ) is not None,
        }


    c.close()

    return out


def error_dir_contents( db_dir ):
    """What `missing_and_invalid_files` holds: the dated hash list as its
    hashes, text files as their lines (as sets, where the reference writes
    them from sets), other files by name."""

    error_dir = os.path.join( db_dir, 'missing_and_invalid_files' )

    out = { 'missing_hashes': [], 'text': {}, 'files': [] }

    if not os.path.exists( error_dir ):

        return out


    for filename in sorted( os.listdir( error_dir ) ):

        path = os.path.join( error_dir, filename )

        if re.fullmatch( r'\d{4}-\d\d-\d\d \d\d-\d\d-\d\d missing hashes\.txt', filename ):

            out[ 'missing_hashes' ].extend( open( path, encoding = 'utf-8' ).read().split() )

        elif filename.endswith( '.txt' ):

            out[ 'text' ][ filename ] = sorted( open( path, encoding = 'utf-8' ).read().splitlines() )

        else:

            out[ 'files' ].append( filename )



    out[ 'missing_hashes' ].sort()

    return out


def run( out_path ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    apply_edits( db_dir )

    def hook( session ):

        from hydrus.client import ClientGlobals as CG
        from hydrus.client import ClientThreading
        from hydrus.client.networking import ClientNetworkingFunctions

        controller = session.controller

        controller.new_options.SetBoolean( 'file_maintenance_during_idle', False )
        controller.new_options.SetBoolean( 'file_maintenance_during_active', False )
        controller.new_options.SetBoolean( 'delete_lock_for_archived_files', True )

        imported = []

        def import_url( url, destination_page_name ):

            normalised = CG.client_controller.network_engine.domain_manager.NormaliseURL( ClientNetworkingFunctions.EnsureURLIsEncoded( url ), for_server = True )

            imported.append( { 'url': url, 'normalised': normalised, 'page': destination_page_name } )


        controller.gui.ImportURL = import_url

        manager = controller.files_maintenance_manager

        # ForceMaintenance's loop, holding the maintenance lock throughout:
        # otherwise the background loop can take the jobs and, with
        # background maintenance off, wait on them while holding the lock
        while not manager._maintenance_lock.acquire( timeout = 1 ):

            manager._reset_background_event.set()


        try:

            for sc in SCENARIOS:

                controller.WriteSynchronous( 'file_maintenance_add_jobs_hashes', { bytes.fromhex( BY_NAME[ sc[ 'file' ] ] ) }, sc[ 'job' ], 0 )


            mandated = sorted( { sc[ 'job' ] for sc in SCENARIOS } )

            job_status = ClientThreading.JobStatus()

            while True:

                hashes_to_job_types = controller.Read( 'file_maintenance_get_jobs', mandated )

                if len( hashes_to_job_types ) == 0:

                    break


                media_results = controller.Read( 'media_results', set( hashes_to_job_types.keys() ) )

                by_hash = { m.GetHash(): m for m in media_results }

                with manager._lock:

                    manager._RunJob( { by_hash[ h ]: job_types for ( h, job_types ) in hashes_to_job_types.items() }, job_status )


                controller.WriteSynchronous( 'null' )


        finally:

            manager._maintenance_lock.release()


        time.sleep( 0.5 )
        controller.ForceDatabaseCommit()
        controller.WriteSynchronous( 'null' )

        return { 'imported': sorted( imported, key = lambda i: i[ 'url' ] ) }


    result = hydrus_driver.run_client( db_dir, hook, port = PORT )

    result[ 'files' ] = snapshot( db_dir )
    result[ 'error_dir' ] = error_dir_contents( db_dir )

    with open( out_path, 'w' ) as f:

        json.dump( result, f )



def main():

    import hydrus_driver

    if len( sys.argv ) > 1 and sys.argv[1] == '--child':

        import faulthandler

        # (a hang shows where)
        faulthandler.dump_traceback_later( 240, exit = True )

        run( sys.argv[2] )

        return


    path = OUT + '.tmp'

    hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

    result = json.load( open( path ) )

    os.remove( path )

    result[ 'scenarios' ] = SCENARIOS
    result[ 'garbage' ] = GARBAGE.decode( 'ascii' )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True )
        f.write( '\n' )



if __name__ == '__main__':

    main()
