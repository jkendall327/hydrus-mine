#!/usr/bin/env python3
"""Record the reference serving rating services' SVG icons.

The `basic` fixture's rating services draw built-in shapes, so this boots
the reference on a copy of it with a custom SVG in the database directory's
`static/star_shapes`, points "favourites" at a bundled SVG, "stars" at the
custom one and a new numerical service at an SVG that doesn't exist, and
records `/get_service_rating_svg` and `/get_service` for each.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_rating_svg.py
"""

import hashlib
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

OUT = os.path.join( HERE, 'fixtures', 'rating_svg.json' )

PORT = 45993

CUSTOM_SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10"/></svg>\n'

SVGS = { 'favourites' : 'heart-cute', 'stars' : 'mine', 'missing svg' : 'gone' }


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusData
    from hydrus.client import ClientServices

    controller = session.controller

    services = []

    for service in controller.services_manager.GetServices():

        name = service.GetName()

        if name in SVGS:

            dictionary = service.GetSerialisableDictionary()
            dictionary[ 'shape' ] = None
            dictionary[ 'rating_svg' ] = SVGS[ name ]

            service = ClientServices.GenerateService( service.GetServiceKey(), service.GetServiceType(), name, dictionary )


        services.append( service )


    dictionary = ClientServices.GenerateDefaultServiceDictionary( HC.LOCAL_RATING_NUMERICAL )
    dictionary[ 'shape' ] = None
    dictionary[ 'rating_svg' ] = SVGS[ 'missing svg' ]

    services.append( ClientServices.GenerateService( HydrusData.GenerateKey(), HC.LOCAL_RATING_NUMERICAL, 'missing svg', dictionary ) )

    controller.WriteSynchronous( 'update_services', services )

    for _ in range( 50 ):

        if 'missing svg' in [ s.GetName() for s in controller.services_manager.GetServices() ]:

            break


        time.sleep( 0.1 )


    out = []

    for name in SVGS:

        [ service ] = [ s for s in controller.services_manager.GetServices() if s.GetName() == name ]

        key = service.GetServiceKey().hex()

        for path in ( '/get_service_rating_svg', '/get_service' ):

            ( status, content_type, body ) = session.api.request( 'GET', path, query = { 'service_key' : key } )

            row = { 'name' : name, 'service_key' : key, 'path' : path, 'status' : status, 'content_type' : content_type.split( ';' )[0] }

            if content_type.startswith( 'application/json' ):

                row[ 'json' ] = json.loads( body )

            else:

                row[ 'sha256' ] = hashlib.sha256( body ).hexdigest()
                row[ 'length' ] = len( body )


            out.append( row )



    return out


def main():

    work = tempfile.mkdtemp( prefix = 'hydrus_rating_svg_' )

    try:

        db_dir = os.path.join( work, 'db' )

        with tarfile.open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.tar.gz' ) ) as tar:

            tar.extractall( db_dir, filter = 'data' )


        custom = os.path.join( db_dir, 'static', 'star_shapes' )

        os.makedirs( custom )

        with open( os.path.join( custom, 'mine.svg' ), 'w' ) as f:

            f.write( CUSTOM_SVG )


        result = hydrus_driver.run_client( db_dir, record, port = PORT )

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( { 'custom_svg' : CUSTOM_SVG, 'svgs' : SVGS, 'requests' : result }, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result )} requests' )


if __name__ == '__main__':

    main()
