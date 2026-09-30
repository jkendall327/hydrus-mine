#!/usr/bin/env python3
"""Generate the small, deterministic media corpus imported into fixture databases.

The output is committed (oracle/fixtures/import_media/) so file hashes stay
stable even if encoder libraries change. Re-run only to extend the corpus.
"""
import io
import os
import random
import shutil
import subprocess
import zipfile

from PIL import Image, ImageDraw

OUT = os.path.join( os.path.dirname( __file__ ), 'fixtures', 'import_media' )

rng = random.Random( 688 )

def scene( w, h, seed, mode = 'RGB' ):
    r = random.Random( seed )
    bg = tuple( r.randrange( 256 ) for _ in range( 3 ) ) + ( ( 255, ) if mode == 'RGBA' else () )
    img = Image.new( mode, ( w, h ), bg )
    d = ImageDraw.Draw( img )
    for _ in range( 12 ):
        x0, y0 = r.randrange( w ), r.randrange( h )
        x1, y1 = x0 + r.randrange( 1, w ), y0 + r.randrange( 1, h )
        colour = tuple( r.randrange( 256 ) for _ in range( 3 ) ) + ( ( r.randrange( 0, 256 ), ) if mode == 'RGBA' else () )
        if r.random() < 0.5:
            d.rectangle( ( x0, y0, x1, y1 ), fill = colour )
        else:
            d.ellipse( ( x0, y0, x1, y1 ), fill = colour )
    return img

def save( img, name, **kwargs ):
    img.save( os.path.join( OUT, name ), **kwargs )

def main():
    if os.path.exists( OUT ):
        shutil.rmtree( OUT )
    os.makedirs( OUT )

    sizes = [ ( 64, 48 ), ( 320, 240 ), ( 640, 480 ), ( 480, 640 ), ( 1000, 200 ), ( 200, 1000 ), ( 800, 800 ), ( 123, 457 ), ( 1920, 1080 ), ( 300, 300 ), ( 50, 50 ), ( 777, 333 ) ]
    for ( i, ( w, h ) ) in enumerate( sizes ):
        save( scene( w, h, i ), f'jpeg_{i:02}.jpg', quality = 88 )

    # near duplicates of jpeg_02: re-encoded, resized, and a png version
    base = scene( 640, 480, 2 )
    save( base, 'dupe_jpeg_02_q60.jpg', quality = 60 )
    save( base.resize( ( 320, 240 ) ), 'dupe_jpeg_02_small.jpg', quality = 88 )
    save( base, 'dupe_jpeg_02.png' )

    for i in range( 4 ):
        save( scene( 200 + 40 * i, 150 + 30 * i, 100 + i ), f'png_{i:02}.png' )
    for i in range( 3 ):
        save( scene( 256, 256, 200 + i, mode = 'RGBA' ), f'png_alpha_{i:02}.png' )

    save( scene( 160, 120, 300 ).convert( 'P' ), 'gif_static.gif' )
    frames = [ scene( 120, 90, 310 + f ).convert( 'P' ) for f in range( 5 ) ]
    frames[0].save( os.path.join( OUT, 'gif_animated.gif' ), save_all = True, append_images = frames[1:], duration = 100, loop = 0 )
    frames = [ scene( 100, 100, 320 + f ) for f in range( 4 ) ]
    frames[0].save( os.path.join( OUT, 'apng_animated.png' ), save_all = True, append_images = frames[1:], duration = 150, loop = 0 )

    save( scene( 300, 200, 400 ), 'webp_static.webp', quality = 80 )
    save( scene( 300, 200, 401, mode = 'RGBA' ), 'webp_alpha.webp', lossless = True )
    frames = [ scene( 90, 90, 410 + f ) for f in range( 3 ) ]
    frames[0].save( os.path.join( OUT, 'webp_animated.webp' ), save_all = True, append_images = frames[1:], duration = 120, loop = 0 )

    save( scene( 64, 64, 500 ), 'bitmap.bmp' )
    save( scene( 128, 96, 501 ), 'tiff_image.tiff' )
    save( scene( 200, 280, 502 ), 'document.pdf' )

    with zipfile.ZipFile( os.path.join( OUT, 'archive.zip' ), 'w' ) as z:
        z.writestr( 'readme.txt', 'fixture archive\n' )

    def ffmpeg( *args ):
        subprocess.run( [ 'ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', *args ], check = True )

    ffmpeg( '-f', 'lavfi', '-i', 'testsrc=duration=1:size=160x120:rate=10', '-f', 'lavfi', '-i', 'sine=frequency=440:duration=1',
            '-c:v', 'libx264', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest', os.path.join( OUT, 'video_with_audio.mp4' ) )
    ffmpeg( '-f', 'lavfi', '-i', 'testsrc2=duration=2:size=128x96:rate=8', '-c:v', 'libvpx-vp9', '-b:v', '100k', os.path.join( OUT, 'video_silent.webm' ) )
    ffmpeg( '-f', 'lavfi', '-i', 'sine=frequency=660:duration=2', '-c:a', 'libmp3lame', '-b:a', '64k', os.path.join( OUT, 'audio.mp3' ) )
    ffmpeg( '-f', 'lavfi', '-i', 'sine=frequency=880:duration=1', '-c:a', 'flac', os.path.join( OUT, 'audio.flac' ) )

if __name__ == '__main__':
    main()
    total = sum( os.path.getsize( os.path.join( OUT, f ) ) for f in os.listdir( OUT ) )
    print( f'{len( os.listdir( OUT ) )} files, {total} bytes' )
