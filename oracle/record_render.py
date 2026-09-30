#!/usr/bin/env python3
"""Record the reference's `/get_files/render` on the `basic` fixture.

Rendered images are encoded by OpenCV (libpng, libjpeg-turbo, libwebp), so
their bytes can't be matched exactly; what is recorded is what they decode
to: for PNG (lossless) the decoded pixels' sha256, and for JPEG and WebP the
decoded pixels themselves (renders are kept small), to compare within a
tolerance. Errors are recorded as the Client API reports them.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_render.py
"""

import base64
import hashlib
import io
import json
import os
import shutil
import sys
import tarfile
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

OUT = os.path.join( HERE, 'fixtures', 'render.json' )

PORT = 45995

STATIC = [ 'jpeg_00.jpg', 'png_00.png', 'png_alpha_00.png', 'gif_static.gif', 'bitmap.bmp', 'tiff_image.tiff', 'webp_static.webp', 'webp_alpha.webp', 'dupe_jpeg_02_small.jpg' ]


def requests( by_name ):

    out = []

    for name in STATIC:

        h = by_name[ name ]

        out.append( ( name, { 'hash' : h } ) )
        out.append( ( name, { 'hash' : h, 'render_format' : 2, 'render_quality' : 9 } ) )
        out.append( ( name, { 'hash' : h, 'width' : 40, 'height' : 30 } ) )
        out.append( ( name, { 'hash' : h, 'width' : 17, 'height' : 23, 'render_format' : 1 } ) )
        out.append( ( name, { 'hash' : h, 'width' : 17, 'height' : 23, 'render_format' : 1, 'render_quality' : 50 } ) )
        out.append( ( name, { 'hash' : h, 'width' : 17, 'height' : 23, 'render_format' : 33 } ) )
        out.append( ( name, { 'hash' : h, 'width' : 17, 'height' : 23, 'render_format' : 33, 'render_quality' : 101 } ) )


    h = by_name[ 'png_00.png' ]

    out += [
        # bigger than the original, and the same size
        ( 'png_00.png', { 'hash' : h, 'width' : 500, 'height' : 20 } ),
        ( 'png_00.png', { 'hash' : h, 'width' : 20, 'height' : 500 } ),
        ( 'png_00.png', { 'hash' : h, 'download' : 'true' } ),
        # only one of width and height: not resized
        ( 'png_00.png', { 'hash' : h, 'width' : 20 } ),
        # errors
        ( 'png_00.png', { 'hash' : h, 'render_format' : 99 } ),
        ( 'png_00.png', { 'hash' : h, 'width' : 0, 'height' : 10 } ),
        ( 'png_00.png', { 'hash' : h, 'width' : 10, 'height' : -1 } ),
        ( 'video_with_audio.mp4', { 'hash' : by_name[ 'video_with_audio.mp4' ] } ),
        ( 'gif_animated.gif', { 'hash' : by_name[ 'gif_animated.gif' ] } ),
        ( 'document.pdf', { 'hash' : by_name[ 'document.pdf' ] } ),
        ( None, { 'hash' : 'ab' * 32 } ),
        ( None, { 'file_id' : 999999 } ),
        ( None, {} ),
        ( 'png_00.png', { 'hash' : h, 'key' : 'restricted' } ),
    ]

    return out


def describe( content_type, body, small ):

    from PIL import Image

    image = Image.open( io.BytesIO( body ) )
    image.load()

    out = { 'format' : image.format, 'mode' : image.mode, 'size' : list( image.size ) }

    pixels = image.tobytes()

    out[ 'pixels_sha256' ] = hashlib.sha256( pixels ).hexdigest()

    if small:

        out[ 'pixels' ] = base64.b64encode( pixels ).decode( 'ascii' )


    return out


def record( session, manifest ):

    by_name = { f[ 'name' ] : f[ 'hash' ] for f in manifest[ 'files' ] }

    out = []

    for ( name, params ) in requests( by_name ):

        params = dict( params )

        key = params.pop( 'key', 'full' )

        api = hydrus_driver.Api( session.port, access_key = None )

        headers = { 'Hydrus-Client-API-Access-Key' : manifest[ 'access_keys' ][ key ] }

        ( status, content_type, body ) = api.request( 'GET', '/get_files/render', query = { k : str( v ) for ( k, v ) in params.items() }, headers = headers )

        row = { 'file' : name, 'params' : params, 'key' : key, 'status' : status, 'content_type' : content_type.split( ';' )[0] }

        if content_type.startswith( 'application/json' ):

            row[ 'json' ] = json.loads( body )

        else:

            small = 'width' in params and params[ 'width' ] * params.get( 'height', 1 ) <= 1000

            row[ 'image' ] = describe( content_type, body, small )


        out.append( row )


    return out


def main():

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    work = tempfile.mkdtemp( prefix = 'hydrus_render_' )

    try:

        db_dir = os.path.join( work, 'db' )

        with tarfile.open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.tar.gz' ) ) as tar:

            tar.extractall( db_dir, filter = 'data' )


        result = hydrus_driver.run_client( db_dir, lambda s: record( s, manifest ), port = PORT )

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( { 'renders' : result }, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result )} renders' )


if __name__ == '__main__':

    main()
