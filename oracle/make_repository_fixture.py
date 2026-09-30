#!/usr/bin/env python3
"""Make the `repositories` fixture database: `basic` with a tag repository
and a file repository (never connected to a server) holding pending and
petitioned content, for the `/manage_services` scenarios.

On the tag repository: two mappings pended, a sibling and a parent pended
and petitioned. On the file repository: two files pended.

Writes oracle/fixtures/legacy_db/repositories.tar.gz and its manifest (the
basic manifest plus the new services' keys).

Usage: QT_QPA_PLATFORM=offscreen python oracle/make_repository_fixture.py
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

LEGACY = os.path.join( HERE, 'fixtures', 'legacy_db' )

TAG_REPOSITORY = bytes.fromhex( '7a6' * 21 + '7' )
FILE_REPOSITORY = bytes.fromhex( 'f11e' * 16 )


def populate( session, hashes ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientServices
    from hydrus.client.metadata import ClientContentUpdates as CU

    controller = session.controller

    services = list( controller.services_manager.GetServices() )

    for ( key, service_type, name ) in ( ( TAG_REPOSITORY, HC.TAG_REPOSITORY, 'a tag repository' ), ( FILE_REPOSITORY, HC.FILE_REPOSITORY, 'a file repository' ) ):

        dictionary = ClientServices.GenerateDefaultServiceDictionary( service_type )

        services.append( ClientServices.GenerateService( key, service_type, name, dictionary ) )


    controller.WriteSynchronous( 'update_services', services )

    for _ in range( 50 ):

        if TAG_REPOSITORY in [ s.GetServiceKey() for s in controller.services_manager.GetServices() ]:

            break


        time.sleep( 0.1 )


    ( h0, h1, h2 ) = [ bytes.fromhex( h ) for h in hashes[ : 3 ] ]

    tag_updates = [
        CU.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_PEND, ( 'pended tag', ( h0, h1 ) ) ),
        CU.ContentUpdate( HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_PEND, ( 'old name', 'new name' ), reason = 'a better name' ),
        CU.ContentUpdate( HC.CONTENT_TYPE_TAG_PARENTS, HC.CONTENT_UPDATE_PEND, ( 'child', 'parent' ), reason = 'it is one' ),
    ]

    controller.WriteSynchronous( 'content_updates', CU.ContentUpdatePackage.STATICCreateFromContentUpdates( TAG_REPOSITORY, tag_updates ) )

    file_updates = [ CU.ContentUpdate( HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_PEND, ( h1, h2 ) ) ]

    controller.WriteSynchronous( 'content_updates', CU.ContentUpdatePackage.STATICCreateFromContentUpdates( FILE_REPOSITORY, file_updates ) )

    return controller.Read( 'nums_pending' )


def main():

    manifest = json.load( open( os.path.join( LEGACY, 'basic.manifest.json' ) ) )

    hashes = [ f[ 'hash' ] for f in manifest[ 'files' ] ]

    work = tempfile.mkdtemp( prefix = 'hydrus_repositories_' )

    try:

        db_dir = os.path.join( work, 'db' )

        with tarfile.open( os.path.join( LEGACY, 'basic.tar.gz' ) ) as tar:

            tar.extractall( db_dir, filter = 'data' )


        pending = hydrus_driver.run_client( db_dir, lambda s: populate( s, hashes ) )

        with tarfile.open( os.path.join( LEGACY, 'repositories.tar.gz' ), 'w:gz' ) as tar:

            for name in sorted( os.listdir( db_dir ) ):

                if name.endswith( '.db' ) or name == 'client_files':

                    tar.add( os.path.join( db_dir, name ), arcname = name )




    finally:

        shutil.rmtree( work, ignore_errors = True )


    manifest[ 'service_keys' ][ 'tag_repository' ] = TAG_REPOSITORY.hex()
    manifest[ 'service_keys' ][ 'file_repository' ] = FILE_REPOSITORY.hex()

    with open( os.path.join( LEGACY, 'repositories.manifest.json' ), 'w' ) as f:

        json.dump( manifest, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( 'wrote repositories fixture; pending:', { k.hex() : v for ( k, v ) in pending.items() } )


if __name__ == '__main__':

    main()
