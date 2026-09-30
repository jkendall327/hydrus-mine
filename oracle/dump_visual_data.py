#!/usr/bin/env python3
"""Record the reference's visual-duplicates computations, stage by stage.

Duplicates auto-resolution's "A and B are visual duplicates" test
(`ClientVisualData`) runs OpenCV operations on each file's pixels and scores
the pair. This records, for the Rust port:

- `stages`: OpenCV's building blocks on generated images, as sha256 of their
  output (exact ones) or the output itself (float ones, compared within a
  tolerance): the 8-bit Gaussian blur at the reference's sigma, 8-bit
  RGB->Lab over every colour, INTER_AREA resizes (8-bit and float) at the
  sizes the reference uses, and the float Gaussian blur of the edge map.
- `jpegs`: the reference's reading of generated jpegs' quality
  (`oracle/fixtures/visual_jpegs/`), for "A has clearly better jpeg
  quality" and "is a progressive jpeg".
- `pairs`: the reference's verdicts (and the scores behind them) on pairs of
  the auto-resolution and similar-files fixture images, loaded as its image renderer loads
  them (`GenerateNumPyImage`).

Usage: QT_QPA_PLATFORM=offscreen python oracle/dump_visual_data.py
"""

import base64
import hashlib
import itertools
import json
import os
import sys
import zlib

HERE = os.path.dirname( os.path.abspath( __file__ ) )
REPO = os.path.join( HERE, '..' )

sys.path.insert( 0, REPO )

import numpy
import cv2

from hydrus.core import HydrusConstants as HC
from hydrus.core.files.images import HydrusImageHandling
from hydrus.client.files.images import ClientVisualData as V

OUT = os.path.join( HERE, 'fixtures', 'visual_data.json' )
IMAGE_DIRS = [ os.path.join( HERE, 'fixtures', 'auto_resolution' ), os.path.join( HERE, 'fixtures', 'similar_files' ) ]


def sha( a ):

    return hashlib.sha256( numpy.ascontiguousarray( a ).tobytes() ).hexdigest()


def f32( a ):

    return base64.b64encode( numpy.ascontiguousarray( a, dtype = numpy.float32 ).tobytes() ).decode()


def random_image( seed, w, h, cn ):
    """A gradient with hashed noise, in integer arithmetic the Rust tests
    repeat exactly."""

    mask = numpy.uint64( 0xffffffff )

    ( y, x ) = numpy.mgrid[ 0 : h, 0 : w ].astype( numpy.uint64 )

    out = numpy.zeros( ( h, w, cn ), dtype = numpy.uint8 )

    for c in range( cn ):

        v = ( ( x * numpy.uint64( 73856093 ) ) ^ ( y * numpy.uint64( 19349663 ) ) ^ numpy.uint64( ( c + 1 ) * 83492791 ) ^ numpy.uint64( seed * 2654435761 ) ) & mask
        v = ( v * numpy.uint64( 2246822519 ) ) & mask
        noise = ( v >> numpy.uint64( 24 ) ) & numpy.uint64( 255 )

        smooth = ( x * numpy.uint64( 255 ) // numpy.uint64( max( w - 1, 1 ) ) + y * numpy.uint64( 255 ) // numpy.uint64( max( h - 1, 1 ) ) ) // numpy.uint64( 2 )

        out[ :, :, c ] = ( ( smooth * numpy.uint64( 3 ) + noise ) // numpy.uint64( 4 ) ).astype( numpy.uint8 )


    return out


SIZES = [ ( 1, 1 ), ( 2, 3 ), ( 5, 4 ), ( 7, 9 ), ( 17, 9 ), ( 64, 64 ), ( 97, 61 ), ( 160, 160 ), ( 300, 211 ) ]


def stages():

    out = {}

    # 8-bit gaussian blur, one channel at a time as the reference does
    blur = []

    for ( i, ( w, h ) ) in enumerate( SIZES ):

        image = random_image( i, w, h, 1 )[ :, :, 0 ]

        blur.append( { 'seed' : i, 'w' : w, 'h' : h, 'sigma' : V.JPEG_ARTIFACT_GAUSSIAN_SIGMA_AT_100_ZOOM, 'sha' : sha( V.BlurChannelNumPy( image, V.JPEG_ARTIFACT_GAUSSIAN_SIGMA_AT_100_ZOOM ) ) } )


    out[ 'blur_u8' ] = blur

    # every colour through RGB->Lab
    cube = numpy.arange( 1 << 24, dtype = numpy.uint32 )
    rgb = numpy.stack( [ ( cube >> 16 ) & 255, ( cube >> 8 ) & 255, cube & 255 ], axis = -1 ).astype( numpy.uint8 ).reshape( 4096, 4096, 3 )

    out[ 'lab_cube_sha' ] = sha( cv2.cvtColor( rgb, cv2.COLOR_RGB2Lab ) )

    # INTER_AREA resizes the reference makes
    resize_u8 = []

    for ( i, ( w, h ) ) in enumerate( SIZES ):

        for cn in ( 1, 3 ):

            image = random_image( 100 + i, w, h, cn )

            if cn == 1:

                image = image[ :, :, 0 ]


            for target in ( ( 1024, 1024 ), ( 256, 256 ), ( 160, 107 ) ):

                resize_u8.append( { 'seed' : 100 + i, 'w' : w, 'h' : h, 'cn' : cn, 'to' : list( target ), 'sha' : sha( V.do_cv2_resize( image, target ) ) } )




    out[ 'resize_u8' ] = resize_u8

    # float work: the edge map's difference of gaussians, and its resize
    floats = []

    for ( i, ( w, h ) ) in enumerate( [ ( 5, 4 ), ( 40, 30 ), ( 97, 61 ), ( 300, 211 ) ] ):

        image = random_image( 200 + i, w, h, 3 ).astype( numpy.float32 )

        dog = image - V.BlurRGBNumPy( image, 10.0 )

        resized = V.do_cv2_resize( dog.astype( numpy.float32 ), V.EDGE_MAP_NORMALISED_RESOLUTION )

        blurred = V.BlurRGBNumPy( image, 10.0 )

        # the whole output's hash, and a sample to compare within a tolerance
        floats.append( {
            'seed' : 200 + i, 'w' : w, 'h' : h,
            'blur_sha' : sha( blurred ), 'blur_sample' : f32( blurred.reshape( -1 )[ : : 37 ] ),
            'resized_sha' : sha( resized ), 'resized_sample' : f32( resized.reshape( -1 )[ : : 97 ] )
        } )


    out[ 'float' ] = floats

    return out


def describe( visual, tiled ):

    return {
        'resolution' : list( visual.resolution ),
        'has_alpha' : visual.HasAlpha(),
        'lab' : [ f32( visual.lab_histograms.l_hist ), f32( visual.lab_histograms.a_hist ), f32( visual.lab_histograms.b_hist ) ],
        'edge_sha' : [ sha( tiled.edge_map.edge_map_r ), sha( tiled.edge_map.edge_map_g ), sha( tiled.edge_map.edge_map_b ) ],
        'tile_hist_sha' : sha( numpy.stack( [ numpy.stack( [ h.l_hist, h.a_hist, h.b_hist ] ) for h in tiled.histograms ] ) ),
    }


def pairs():

    paths = { f'{os.path.basename( d )}/{n}' : os.path.join( d, n ) for d in IMAGE_DIRS for n in os.listdir( d ) }

    names = sorted( paths )

    data = {}

    for name in names:

        path = paths[ name ]

        from hydrus.core.files import HydrusFileHandling

        mime = HydrusFileHandling.GetMime( path )

        numpy_image = HydrusImageHandling.GenerateNumPyImage( path, mime )

        data[ name ] = ( V.GenerateImageVisualDataNumPy( numpy_image ), V.GenerateImageVisualDataTiledNumPy( numpy_image ) )


    files = { name : describe( *data[ name ] ) for name in names }

    out = []

    for ( a, b ) in itertools.combinations( names, 2 ):

        # pairs within a family, and some across
        family = lambda n: n.split( '/' )[0] + '/' + n.split( '/' )[1].split( '_' )[0]
        if family( a ) != family( b ) and zlib.crc32( ( a + b ).encode() ) % 23 != 0:

            continue


        ( va, ta ) = data[ a ]
        ( vb, tb ) = data[ b ]

        ( simple_ok, simple_result, _ ) = V.FilesAreVisuallySimilarSimple( va, vb )

        ( interesting, lab_score ) = V.GetVisualDataWassersteinDistanceScore( va.lab_histograms, vb.lab_histograms )

        row = { 'a' : a, 'b' : b, 'simple' : [ simple_ok, simple_result ], 'simple_score' : lab_score }

        if simple_ok:

            ( regional_ok, regional_result, _ ) = V.FilesAreVisuallySimilarRegional( ta, tb )

            row[ 'regional' ] = [ regional_ok, regional_result ]
            row[ 'edge' ] = [ float( x ) for x in V.FilesAreVisuallySimilarRegionalEdgeMapRaw( ta.edge_map, tb.edge_map ) ]
            row[ 'lab' ] = [ float( x ) if not isinstance( x, bool ) else x for x in V.FilesAreVisuallySimilarRegionalLabHistogramsRaw( ta.histograms, tb.histograms ) ]


        out.append( row )


    return ( files, out )


def jpegs():
    """Generated jpegs in every subsampling, progressive or not, greyscale
    and CMYK, with the reference's reading of their quality."""

    from PIL import Image

    from hydrus.core.files.images import HydrusImageMetadata
    from hydrus.core.files.images import HydrusImageOpening

    directory = os.path.join( HERE, 'fixtures', 'visual_jpegs' )

    os.makedirs( directory, exist_ok = True )

    base = Image.fromarray( random_image( 7, 64, 48, 3 ) )

    variants = []

    for quality in ( 40, 75, 95 ):

        for subsampling in ( 0, 1, 2 ):

            variants.append( ( f'q{quality}_s{subsampling}.jpg', base, dict( quality = quality, subsampling = subsampling ) ) )



    variants.append( ( 'progressive.jpg', base, dict( quality = 80, progressive = True ) ) )
    variants.append( ( 'greyscale.jpg', base.convert( 'L' ), dict( quality = 80 ) ) )
    variants.append( ( 'cmyk.jpg', base.convert( 'CMYK' ), dict( quality = 80 ) ) )
    variants.append( ( 'optimised.jpg', base, dict( quality = 60, optimize = True ) ) )

    out = {}

    for ( name, image, options ) in variants:

        path = os.path.join( directory, name )

        image.save( path, 'JPEG', **options )

        raw = HydrusImageOpening.RawOpenPILImage( path )

        subsampling = HydrusImageMetadata.GetJpegSubsamplingRaw( raw )
        ( label, quality ) = HydrusImageMetadata.GetJPEGQuantizationQualityEstimate( raw )

        out[ name ] = { 'subsampling' : subsampling, 'quality' : quality, 'progressive' : 'progression' in raw.info }


    return out


def main():

    result = { 'opencv' : cv2.__version__, 'stages' : stages(), 'jpegs' : jpegs() }

    ( result[ 'files' ], result[ 'pairs' ] ) = pairs()

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "pairs" ] )} pairs' )


if __name__ == '__main__':

    main()
