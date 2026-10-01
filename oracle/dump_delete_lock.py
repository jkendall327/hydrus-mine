#!/usr/bin/env python3
"""Record what the reference's archived-file delete lock does.

With `delete_lock_for_archived_files` on, on the basic fixture: Client API
deletes from each file domain (/add_files/delete_files), duplicate
decisions that delete a file with and without the default merge
(/manage_file_relationships/set_file_relationships), and emptying the
trash (DAEMONMaintainTrash with a maximum age of zero). After every step
the database is committed and each file's inbox state and local domains
are read straight from it, with the step's status and any error.

Two phases, each on a fresh copy of the fixture: the lock alone, and the
lock with `delete_lock_reinbox_deletees_after_duplicate_filter`. Each also
writes the options in hydrus-rs's settings format.

Usage: python oracle/dump_delete_lock.py      (writes fixtures/delete_lock.json)
"""

import json
import os
import sqlite3
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'delete_lock.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )
BY_NAME = { f[ 'name' ]: f[ 'hash' ] for f in MANIFEST[ 'files' ] }

PORT = 45961
DOMAINS = ( 'my files', 'art', 'trash', 'hydrus local file storage' )


def snapshot( db_dir, pool ):
    """Each of `pool`'s files' inbox state and local domains, from the database."""

    c = sqlite3.connect( f'file:{db_dir}/client.db?mode=ro', uri = True )
    c.execute( f"ATTACH 'file:{db_dir}/client.master.db?mode=ro' AS master" )

    services = { name: service_id for ( service_id, name ) in c.execute( 'SELECT service_id, name FROM services' ) }

    def one( sql, *args ):

        row = c.execute( sql, args ).fetchone()

        return None if row is None else row[0]


    out = {}

    for name in pool:

        hash_id = one( 'SELECT hash_id FROM master.hashes WHERE hash = ?', bytes.fromhex( BY_NAME[ name ] ) )

        out[ name ] = {
            'inbox': one( 'SELECT 1 FROM file_inbox WHERE hash_id = ?', hash_id ) is not None,
            'domains': sorted( d for d in DOMAINS if one( f'SELECT 1 FROM current_files_{services[ d ]} WHERE hash_id = ?', hash_id ) ),
        }


    c.close()

    return out


def run_phase( phase_name, out_path ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    def hook( session ):

        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client import ClientDaemons

        controller = session.controller
        api = hydrus_driver.Api( session.port )

        reinbox = phase_name == 'reinbox'

        controller.new_options.SetBoolean( 'delete_lock_for_archived_files', True )
        controller.new_options.SetBoolean( 'delete_lock_reinbox_deletees_after_duplicate_filter', reinbox )

        settings = { 'archived': True, 'reinbox_after_archive_delete': False, 'reinbox_after_duplicate_filter': reinbox, 'reinbox_in_auto_resolution': False }

        def settle():

            time.sleep( 0.05 )
            controller.ForceDatabaseCommit()
            controller.WriteSynchronous( 'null' )


        # eight files only in my files, the first four archived
        candidates = sorted( n for n in BY_NAME if n.startswith( ( 'jpeg_', 'png_' ) ) )
        start = snapshot( db_dir, candidates )
        pool = [ n for n in candidates if start[ n ][ 'domains' ] == [ 'hydrus local file storage', 'my files' ] ][ : 8 ]
        ( a, b, c, d, e, f, g, h ) = [ BY_NAME[ n ] for n in pool ]

        seed_steps = [
            ( '/add_files/archive_files', { 'hashes': [ a, b, c, d ] } ),
            ( '/add_files/unarchive_files', { 'hashes': [ e, f, g, h ] } ),
        ]

        for ( path, body ) in seed_steps:

            api.post( path, body )


        settle()

        initial = snapshot( db_dir, pool )

        storage = CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY.hex()
        my_files = CC.LOCAL_FILE_SERVICE_KEY.hex()
        trash = CC.TRASH_SERVICE_KEY.hex()
        all_my_files = CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY.hex()

        def delete( hashes, service_key ):

            return ( '/add_files/delete_files', { 'hashes': hashes, 'file_service_key': service_key } )


        def decide( better, worse, merge ):

            return ( '/manage_file_relationships/set_file_relationships', { 'relationships': [ { 'hash_a': better, 'hash_b': worse, 'relationship': 4, 'do_default_content_merge': merge, 'delete_b': True } ] } )


        if reinbox:

            plan = [
                decide( a, b, True ),
                decide( c, d, False ),
                'empty the trash',
            ]

        else:

            plan = [
                delete( [ a, e ], storage ),
                delete( [ a, e ], my_files ),
                delete( [ a, e ], trash ),
                delete( [ a, e ], storage ),
                delete( [ e ], storage ),
                delete( [ b ], all_my_files ),
                decide( f, c, True ),
                decide( g, d, False ),
                delete( [ h ], my_files ),
                'empty the trash',
            ]


        steps = []

        for item in plan:

            if item == 'empty the trash':

                # (the age limit counts whole seconds)
                time.sleep( 1.1 )

                HC.options[ 'trash_max_age' ] = 0
                HC.options[ 'trash_max_size' ] = None

                ClientDaemons.DAEMONMaintainTrash()

                settle()

                steps.append( { 'empty_trash': True, 'state': snapshot( db_dir, pool ) } )

                continue


            ( path, body ) = item

            ( status, content_type, raw ) = api.request( 'POST', path, json_body = body )

            settle()

            step = { 'path': path, 'json': body, 'status': status, 'state': snapshot( db_dir, pool ) }

            if status != 200:

                error = json.loads( raw )

                step[ 'error' ] = error[ 'error' ]
                step[ 'exception_type' ] = error[ 'exception_type' ]


            steps.append( step )


        return { 'name': phase_name, 'settings': settings, 'pool': pool, 'seed_steps': [ { 'path': p, 'json': b } for ( p, b ) in seed_steps ], 'initial': initial, 'steps': steps }


    result = hydrus_driver.run_client( db_dir, hook, port = PORT )

    with open( out_path, 'w' ) as f:

        json.dump( result, f )



def main():

    import hydrus_driver

    if len( sys.argv ) > 1 and sys.argv[1] == '--child':

        run_phase( sys.argv[2], sys.argv[3] )

        return


    phases = []

    for name in ( 'lock', 'reinbox' ):

        path = OUT + f'.{name}.tmp'

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', name, path )

        phases.append( json.load( open( path ) ) )

        os.remove( path )


    with open( OUT, 'w' ) as f:

        json.dump( { 'phases': phases }, f, indent = 1, sort_keys = True )
        f.write( '\n' )



if __name__ == '__main__':

    main()
