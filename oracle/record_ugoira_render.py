#!/usr/bin/env python3
"""Record the reference's `/get_files/render` of ugoiras.

On the `basic` fixture, the corpus's four ugoiras (`fixtures/media/ugoira_*`:
frames only, an `animation.json` object, an `animation.json` list, and
EXIF-rotated frames) are imported, and each is rendered as APNG (the
default) and animated WebP (lossy, and lossless with a quality over 100),
plus an unsupported format and a download. The frames-only ugoira is
rendered again with a "ugoira frame delay array" note, then also a
"ugoira json" note (which wins), and the `animation.json` one with a note
(which loses to its JSON).

Each render is recorded as the status, content type, cache lifetime and
disposition, and what it decodes to with Pillow: the format, loop count
and each frame's duration, size, mode and pixels' sha256 (the encoders
differ, so bytes can't be compared; APNG and lossless WebP pixels can).

Usage: python oracle/record_ugoira_render.py      (writes fixtures/ugoira_render.json)
"""

import hashlib
import io
import json
import os
import sys
import urllib.parse
import urllib.request

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'ugoira_render.json' )
MEDIA = os.path.join( HERE, 'fixtures', 'media' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

PORT = 45963

UGOIRAS = [ 'ugoira_plain.zip', 'ugoira_json.zip', 'ugoira_list_json.zip', 'ugoira_rotated.zip' ]

# render params, by label
RENDERS = [
    ( 'apng', {} ),
    ( 'webp', { 'render_format': 83 } ),
    ( 'webp_lossless', { 'render_format': 83, 'render_quality': 101 } ),
]

DELAY_ARRAY = json.dumps( [ 50, 60, 70, 80, 90 ] )
UGOIRA_JSON = json.dumps( { 'frames': [ { 'file': f'{i:06}.jpg', 'delay': 200 + i } for i in range( 5 ) ] } )


def describe( body ):

    from PIL import Image, ImageSequence

    image = Image.open( io.BytesIO( body ) )

    frames = []

    for frame in ImageSequence.Iterator( image ):

        frame.load()

        frames.append( {
            'duration': round( frame.info.get( 'duration', 0 ) ),
            'size': list( frame.size ),
            'mode': frame.mode,
            'pixels_sha256': hashlib.sha256( frame.tobytes() ).hexdigest(),
        } )


    return { 'format': image.format, 'loop': image.info.get( 'loop' ), 'frames': frames }


def render( port, hash_hex, params ):

    query = urllib.parse.urlencode( { 'hash': hash_hex, **{ k: str( v ) for ( k, v ) in params.items() } } )

    req = urllib.request.Request( f'http://127.0.0.1:{port}/get_files/render?{query}', headers = { 'Hydrus-Client-API-Access-Key': MANIFEST[ 'access_keys' ][ 'full' ] } )

    try:

        with urllib.request.urlopen( req, timeout = 120 ) as response:

            ( status, headers, body ) = ( response.status, response.headers, response.read() )


    except urllib.error.HTTPError as e:

        ( status, headers, body ) = ( e.code, e.headers, e.read() )


    content_type = headers.get( 'Content-Type', '' ).split( ';' )[0]

    row = {
        'params': params,
        'status': status,
        'content_type': content_type,
        'cache_control': headers.get( 'Cache-Control' ),
        'disposition': headers.get( 'Content-Disposition' ),
    }

    if content_type.startswith( 'application/json' ):

        row[ 'json' ] = json.loads( body )

    else:

        row[ 'animation' ] = describe( body )


    return row


def record( session ):

    import hydrus_driver

    api = hydrus_driver.Api( session.port, access_key = bytes.fromhex( MANIFEST[ 'access_keys' ][ 'full' ] ) )

    hashes = {}

    for name in UGOIRAS:

        result = api.post( '/add_files/add_file', { 'path': os.path.join( MEDIA, name ) } )

        hashes[ name ] = result[ 'hash' ]


    out = []

    def render_all( name, state ):

        for ( label, params ) in RENDERS:

            out.append( { 'file': name, 'state': state, 'label': label, **render( session.port, hashes[ name ], params ) } )



    for name in UGOIRAS:

        render_all( name, 'no notes' )


    plain = 'ugoira_plain.zip'

    out.append( { 'file': plain, 'state': 'no notes', 'label': 'png', **render( session.port, hashes[ plain ], { 'render_format': 2 } ) } )
    out.append( { 'file': plain, 'state': 'no notes', 'label': 'download', **render( session.port, hashes[ plain ], { 'download': 'true' } ) } )

    notes_steps = [
        ( plain, 'delay array note', { 'ugoira frame delay array': DELAY_ARRAY } ),
        ( plain, 'both notes', { 'ugoira frame delay array': DELAY_ARRAY, 'ugoira json': UGOIRA_JSON } ),
        ( 'ugoira_json.zip', 'delay array note', { 'ugoira frame delay array': DELAY_ARRAY } ),
    ]

    for ( name, state, notes ) in notes_steps:

        api.post( '/add_notes/set_notes', { 'hash': hashes[ name ], 'notes': notes } )

        render_all( name, state )


    return { 'hashes': hashes, 'notes': [ { 'file': name, 'state': state, 'notes': notes } for ( name, state, notes ) in notes_steps ], 'renders': out }


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record, port = PORT )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "renders" ] )} renders' )


if __name__ == '__main__':

    main()
