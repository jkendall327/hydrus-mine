#!/usr/bin/env python3
"""Record the reference's similar-files search on generated near-duplicates.

Generates small images (oracle/fixtures/similar_files/*.png): base patterns,
each with variants that should look alike to a perceptual hash (resized,
noised, brightened, shifted, cropped) and some that shouldn't (a different
pattern), plus a few flat colours, which have blank perceptual hashes. A
fresh reference client imports them all through the Client API in name
order, then runs its similar-files search at distance 2 and then at 4 (so
files searched at 2 are searched again). After each, the potential duplicate
pairs (by the kings' hashes, with their distances) and the files' search
status are recorded, with every file's perceptual hashes, to
oracle/fixtures/similar_files.json.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_similar_files.py
"""

import json
import os
import random
import shutil
import sqlite3
import sys
import tempfile

sys.path.insert( 0, os.path.dirname( __file__ ) )

import hydrus_driver

import numpy
from PIL import Image

HERE = os.path.dirname( os.path.abspath( __file__ ) )
IMAGE_DIR = os.path.join( HERE, 'fixtures', 'similar_files' )
OUT = os.path.join( HERE, 'fixtures', 'similar_files.json' )

PORT = 45979

rng = random.Random( 2 )


def base_pattern( seed ):

    r = numpy.random.default_rng( seed )

    size = 96

    # a few blurred blobs over a gradient: enough structure for a DCT hash
    y, x = numpy.mgrid[ 0 : size, 0 : size ]

    image = numpy.zeros( ( size, size, 3 ) )

    for channel in range( 3 ):

        image[ :, :, channel ] = r.uniform( 0, 255 ) * ( x / size ) + r.uniform( 0, 255 ) * ( y / size )


    for _ in range( 6 ):

        cx, cy, radius = r.uniform( 0, size ), r.uniform( 0, size ), r.uniform( 8, 30 )

        mask = ( ( x - cx ) ** 2 + ( y - cy ) ** 2 ) < radius ** 2

        image[ mask ] = r.uniform( 0, 255, 3 )


    return Image.fromarray( numpy.clip( image, 0, 255 ).astype( numpy.uint8 ) )


def variants( image, seed ):

    r = numpy.random.default_rng( seed )

    out = {}

    out[ 'resized' ] = image.resize( ( 80, 80 ), Image.BILINEAR )

    noisy = numpy.asarray( image ).astype( float ) + r.normal( 0, rng.choice( [ 4, 12, 25 ] ), ( 96, 96, 3 ) )
    out[ 'noised' ] = Image.fromarray( numpy.clip( noisy, 0, 255 ).astype( numpy.uint8 ) )

    bright = numpy.asarray( image ).astype( float ) * rng.choice( [ 0.8, 1.2 ] )
    out[ 'brightened' ] = Image.fromarray( numpy.clip( bright, 0, 255 ).astype( numpy.uint8 ) )

    shift = rng.choice( [ 2, 5, 9 ] )
    out[ 'shifted' ] = image.transform( image.size, Image.AFFINE, ( 1, 0, shift, 0, 1, 0 ) )

    crop = rng.choice( [ 4, 10 ] )
    out[ 'cropped' ] = image.crop( ( crop, crop, 96 - crop, 96 - crop ) )

    return out


def generate():

    if os.path.exists( IMAGE_DIR ):

        shutil.rmtree( IMAGE_DIR )


    os.makedirs( IMAGE_DIR )

    for i in range( 14 ):

        base = base_pattern( 1000 + i )

        base.save( os.path.join( IMAGE_DIR, f'p{i:02}_base.png' ) )

        for ( name, variant ) in variants( base, 2000 + i ).items():

            if rng.random() < 0.8:

                variant.save( os.path.join( IMAGE_DIR, f'p{i:02}_{name}.png' ) )




    for ( i, colour ) in enumerate( [ ( 10, 10, 10 ), ( 200, 30, 30 ), ( 240, 240, 240 ) ] ):

        Image.new( 'RGB', ( 64, 64 ), colour ).save( os.path.join( IMAGE_DIR, f'flat_{i}.png' ) )


    # the same pixels in another file
    shutil.copy( os.path.join( IMAGE_DIR, 'p00_base.png' ), os.path.join( IMAGE_DIR, 'p00_base_copy.bmp.png' ) )

    Image.open( os.path.join( IMAGE_DIR, 'p00_base.png' ) ).save( os.path.join( IMAGE_DIR, 'p00_same_pixels.bmp' ) )


def record( db_dir ):

    db = sqlite3.connect( os.path.join( db_dir, 'client.db' ) )

    db.execute( f"ATTACH '{os.path.join( db_dir, 'client.master.db' )}' AS m" )

    pairs = sorted(
        sorted( [ a, b ] ) + [ distance ]
        for ( a, b, distance ) in db.execute(
            'SELECT hex( ha.hash ), hex( hb.hash ), p.distance FROM potential_duplicate_pairs AS p '
            'JOIN duplicate_files AS da ON da.media_id = p.smaller_media_id '
            'JOIN duplicate_files AS db ON db.media_id = p.larger_media_id '
            'JOIN m.hashes AS ha ON ha.hash_id = da.king_hash_id '
            'JOIN m.hashes AS hb ON hb.hash_id = db.king_hash_id'
        )
    )

    status = dict(
        db.execute( 'SELECT hex( h.hash ), s.searched_distance FROM shape_search_cache AS s JOIN m.hashes AS h USING ( hash_id )' )
    )

    phashes = {}

    for ( h, p ) in db.execute( 'SELECT hex( h.hash ), hex( p.phash ) FROM m.shape_perceptual_hash_map AS map JOIN m.shape_perceptual_hashes AS p USING ( phash_id ) JOIN m.hashes AS h USING ( hash_id )' ):

        phashes.setdefault( h.lower(), [] ).append( p.lower() )


    db.close()

    return {
        'pairs' : [ [ a.lower(), b.lower(), d ] for ( a, b, d ) in pairs ],
        'searched' : { h.lower() : d for ( h, d ) in status.items() },
        'phashes' : { h : sorted( ps ) for ( h, ps ) in phashes.items() },
    }


def main():

    if len( sys.argv ) > 1:

        # one client boot per process
        ( step, db_dir ) = sys.argv[ 1 : 3 ]

        if step == 'import':

            names = sorted( os.listdir( IMAGE_DIR ) )

            def import_all( s ):

                return [ s.api.post( '/add_files/add_file', { 'path' : os.path.join( IMAGE_DIR, name ) } )[ 'hash' ] for name in names ]


            hashes = hydrus_driver.run_client( db_dir, import_all, port = PORT )

            with open( os.path.join( db_dir, 'imported.json' ), 'w' ) as f:

                json.dump( [ [ name, h ] for ( name, h ) in zip( names, hashes ) ], f )


        else:

            distance = int( sys.argv[ 3 ] )

            hydrus_driver.run_client( db_dir, lambda s: s.write( 'maintain_similar_files_search_for_potential_duplicates', distance ) )


        return


    generate()

    work = tempfile.mkdtemp( prefix = 'hydrus_similar_' )

    results = {}

    try:

        db_dir = os.path.join( work, 'db' )

        hydrus_driver.run_in_subprocess( __file__, 'import', db_dir )

        with open( os.path.join( db_dir, 'imported.json' ) ) as f:

            files = json.load( f )


        for distance in ( 2, 4 ):

            hydrus_driver.run_in_subprocess( __file__, 'search', db_dir, str( distance ) )

            results[ str( distance ) ] = record( db_dir )


    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( { 'files' : files, 'searches' : results }, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
