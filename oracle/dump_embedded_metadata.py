#!/usr/bin/env python3
"""Record what the reference's "show detailed embedded file metadata" window
shows for each image of the media corpus and the metadata samples
(`oracle/fixtures/media`, `oracle/fixtures/metadata`).

For each file whose filetype can carry EXIF, XMP, IPTC, embedded text or
software/source (`HC.FILES_THAT_CAN_HAVE_*`), the reference's own worker
(`ShowFileEmbeddedMetadata`'s, its basics line left out) reads the file,
and its window's own conversions (`ReviewFileEmbeddedMetadata`) give what
each box shows:

* `exif`: the EXIF rows, as the list shows them (id, label, value), in
  the order it sorts them (by id), or null for no EXIF box;
* `xmp`, `iptc`: the XMP and IPTC boxes' text, or null for no box;
* `text`: the "human-readable text" box's, or null;
* `extra`: the "extra info" rows (each label and value);
* `has_icc`: whether the file has an ICC profile (as the reference's
  file record has it), which the colour rows read.

`generate` first writes images carrying EXIF of each kind Pillow reads
(rationals, Exif and GPS directories, text with NULs, floats, signed
values, undefined bytes) and the DPI forms of JPEG, PNG and TIFF, into
`oracle/fixtures/metadata/embedded_*`.

Usage: python oracle/dump_embedded_metadata.py generate
       python oracle/dump_embedded_metadata.py > oracle/fixtures/embedded_metadata.json
"""

import json
import os
import sys

ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )
sys.path.insert( 0, ROOT )

os.environ.setdefault( 'QT_QPA_PLATFORM', 'offscreen' )

from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusGlobals as HG
from hydrus.core.files import HydrusFileHandling
from hydrus.core.files.images import HydrusImageMetadata

from hydrus.client import ClientGlobals as CG
from hydrus.client.gui import ClientGUIAsync
from hydrus.client.gui.media import ClientGUIMediaModalActions as A
from hydrus.client.gui.panels import ClientGUIScrolledPanelsReview as R
from hydrus.client.media import ClientMediaResultPrettyInfo

DIRS = [ os.path.join( ROOT, 'oracle', 'fixtures', 'media' ), os.path.join( ROOT, 'oracle', 'fixtures', 'metadata' ) ]

LOOKED_AT = set().union(
    HC.FILES_THAT_CAN_HAVE_EXIF,
    HC.FILES_THAT_CAN_HAVE_XMP,
    HC.FILES_THAT_CAN_HAVE_IPTC,
    HC.FILES_THAT_CAN_HAVE_SOFTWARE_SOURCE,
    HC.FILES_THAT_CAN_HAVE_HUMAN_READABLE_EMBEDDED_METADATA,
)


class Files:

    path = None

    def GetFilePath( self, hash, mime ):

        return Files.path



class Controller:

    client_files_manager = Files()

    def __init__( self ):

        from hydrus.client import ClientOptions

        # (the options as a new client has them: byte sizes' figures)
        self.new_options = ClientOptions.ClientOptions()



class Media:

    # (a stand-in media result for one file)
    def __init__( self, mime, has_icc ):

        self._mime = mime
        self._has_icc = has_icc


    def GetMime( self ):

        return self._mime


    def GetHash( self ):

        return b'\\x00' * 32


    def GetLocationsManager( self ):

        return self


    def IsLocal( self ):

        return True


    def GetFileInfoManager( self ):

        return self


    @property
    def has_icc_profile( self ):

        return self._has_icc



captured = []

class Job:

    def __init__( self, win, work_callable, publish_callable ):

        captured.append( work_callable )


    def start( self ):

        pass



def has_icc( path ):

    from hydrus.core.files.images import HydrusImageOpening

    try:

        image = HydrusImageOpening.RawOpenPILImage( path )

        return 'icc_profile' in image.info and len( image.info[ 'icc_profile' ] ) > 0

    except Exception:

        return False



def generate():

    import io
    import struct

    from PIL import Image, TiffImagePlugin
    from PIL.TiffImagePlugin import IFDRational

    out_dir = os.path.join( ROOT, 'oracle', 'fixtures', 'metadata' )

    def image():

        return Image.new( 'RGB', ( 24, 16 ), ( 200, 100, 50 ) )


    exif = Image.Exif()
    exif[ 271 ] = 'Camera\x00Maker\x00'
    exif[ 272 ] = 'Model 9'
    exif[ 282 ] = IFDRational( 300, 1 )
    exif[ 283 ] = IFDRational( 300, 1 )
    exif[ 296 ] = 2
    exif[ 305 ] = 'Editor 2.0'
    exif[ 274 ] = 1
    exif_ifd = exif.get_ifd( 0x8769 )
    exif_ifd[ 33434 ] = IFDRational( 1, 250 )
    exif_ifd[ 33437 ] = IFDRational( 28, 10 )
    exif_ifd[ 36867 ] = '2023:11:14 22:13:20'
    exif_ifd[ 37510 ] = b'ASCII\x00\x00\x00a comment'
    exif_ifd[ 37380 ] = IFDRational( -1, 3 )
    exif_ifd[ 41989 ] = 35
    gps = exif.get_ifd( 0x8825 )
    gps[ 0 ] = b'\x02\x02\x00\x00'
    gps[ 1 ] = 'N'
    gps[ 2 ] = ( IFDRational( 51, 1 ), IFDRational( 30, 1 ), IFDRational( 1234, 100 ) )
    gps[ 6 ] = IFDRational( 15, 2 )

    image().save( os.path.join( out_dir, 'embedded_exif_rich.jpg' ), 'JPEG', exif = exif )
    image().save( os.path.join( out_dir, 'embedded_exif_rich.png' ), 'PNG', exif = exif )
    image().save( os.path.join( out_dir, 'embedded_exif_rich.webp' ), 'WEBP', exif = exif )

    # DPI only in the EXIF, in centimetres
    dpcm = Image.Exif()
    dpcm[ 282 ] = IFDRational( 118, 1 )
    dpcm[ 283 ] = IFDRational( 118, 1 )
    dpcm[ 296 ] = 3

    def strip_jfif( data ):

        # (the APP0 after SOI cut out, so the DPI comes from the EXIF)
        assert data[ 2 : 4 ] == b'\xff\xe0'

        length = struct.unpack( '>H', data[ 4 : 6 ] )[ 0 ]

        return data[ : 2 ] + data[ 4 + length : ]


    for ( name, e ) in [ ( 'embedded_exif_dpcm.jpg', dpcm ), ( 'embedded_exif_no_unit.jpg', None ) ]:

        b = io.BytesIO()

        if e is None:

            e = Image.Exif()
            e[ 282 ] = IFDRational( 200, 1 )


        image().save( b, 'JPEG', exif = e )

        with open( os.path.join( out_dir, name ), 'wb' ) as f:

            f.write( strip_jfif( b.getvalue() ) )



    image().save( os.path.join( out_dir, 'embedded_jfif_dpcm.jpg' ), 'JPEG', dpi = ( 100, 100 ) )

    # JFIF in centimetres, and unitless
    for ( name, unit, density ) in [ ( 'embedded_jfif_cm.jpg', 2, ( 40, 40 ) ), ( 'embedded_jfif_aspect.jpg', 0, ( 3, 2 ) ) ]:

        b = io.BytesIO()

        image().save( b, 'JPEG' )

        data = bytearray( b.getvalue() )

        assert data[ 6 : 10 ] == b'JFIF'

        data[ 13 ] = unit
        data[ 14 : 18 ] = struct.pack( '>HH', *density )

        with open( os.path.join( out_dir, name ), 'wb' ) as f:

            f.write( bytes( data ) )



    image().save( os.path.join( out_dir, 'embedded_png_dpi.png' ), 'PNG', dpi = ( 96, 120 ) )
    image().save( os.path.join( out_dir, 'embedded_progressive.jpg' ), 'JPEG', progressive = True, subsampling = 0 )

    # TIFFs: resolution without a unit, in centimetres, and compressed with
    # an orientation
    for ( name, unit, compression ) in [ ( 'embedded_tiff_nounit.tiff', 1, None ), ( 'embedded_tiff_cm.tiff', 3, None ), ( 'embedded_tiff_lzw_orient.tiff', 2, 'tiff_lzw' ) ]:

        info = TiffImagePlugin.ImageFileDirectory_v2()
        info[ 282 ] = IFDRational( 72, 1 )
        info[ 283 ] = IFDRational( 96, 1 )
        info[ 296 ] = unit

        if compression is not None:

            info[ 274 ] = 6


        kwargs = { 'tiffinfo' : info }

        if compression is not None:

            kwargs[ 'compression' ] = compression


        image().save( os.path.join( out_dir, name ), 'TIFF', **kwargs )


    # a tag Pillow reads as one value, written with two; and an
    # orientation only in the XMP
    # (written by hand: Pillow's writer would cut them to one)
    tiff = b'II*\x00' + struct.pack( '<I', 8 ) + struct.pack( '<H', 2 )
    tiff += struct.pack( '<HHI', 274, 3, 2 ) + struct.pack( '<HH', 3, 4 )
    tiff += struct.pack( '<HHI', 296, 3, 2 ) + struct.pack( '<HH', 2, 3 )
    tiff += struct.pack( '<I', 0 )
    segment = b'Exif\x00\x00' + tiff
    b = io.BytesIO()
    image().save( b, 'JPEG' )
    data = b.getvalue()

    with open( os.path.join( out_dir, 'embedded_exif_multi.jpg' ), 'wb' ) as f:

        f.write( data[ : 2 ] + struct.pack( '>BBH', 0xFF, 0xE1, len( segment ) + 2 ) + segment + data[ 2 : ] )


    xmp = b'<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:tiff="http://ns.adobe.com/tiff/1.0/" tiff:Orientation="3"/></rdf:RDF></x:xmpmeta>'
    image().save( os.path.join( out_dir, 'embedded_xmp_orientation.jpg' ), 'JPEG', xmp = xmp )

    print( 'wrote the embedded_* samples' )


def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == 'generate':

        generate()

        return


    CG.client_controller = Controller()
    ClientGUIAsync.AsyncQtJob = Job
    ClientMediaResultPrettyInfo.GetPrettyMediaResultInfoLines = lambda media_result: []
    ClientMediaResultPrettyInfo.ConvertInfoLinesToTextBlock = lambda lines: ''

    out = {}

    for directory in DIRS:

        for name in sorted( os.listdir( directory ) ):

            path = os.path.join( directory, name )

            try:

                mime = HydrusFileHandling.GetMime( path )

            except Exception:

                continue


            if mime not in LOOKED_AT or mime == HC.APPLICATION_PDF:

                continue


            Files.path = path

            del captured[:]

            icc = has_icc( path )

            A.ShowFileEmbeddedMetadata( None, Media( mime, icc ) )

            ( mime, top_line_text, exif_dict, xmp_dict, iptc_dict, file_text, extra_rows ) = captured[ 0 ]()

            exif = None

            if exif_dict is not None:

                datas = []

                for ( exif_id, value ) in exif_dict.items():

                    datas.extend( value.items() if isinstance( value, dict ) else [ ( exif_id, value ) ] )


                from hydrus.core import HydrusLists

                datas.sort( key = lambda row: HydrusLists.ConvertTupleOfDatasToCasefolded( R.ReviewFileEmbeddedMetadata._ConvertEXIFToSortTuple( None, row ) ) )

                exif = [ list( R.ReviewFileEmbeddedMetadata._ConvertEXIFToDataTuple( None, row ) ) for row in datas ]


            def rendered( d, empty ):

                if d is None:

                    return None


                try:

                    text = HydrusImageMetadata.render_dict( d, 0 )

                    return empty if text is None else text

                except Exception as e:

                    return f'Could not render: {e}'



            out[ os.path.relpath( path, os.path.join( ROOT, 'oracle', 'fixtures' ) ) ] = {
                'mime' : HC.mime_string_lookup[ mime ],
                'has_icc' : icc,
                'exif' : exif,
                'xmp' : rendered( xmp_dict, 'XMP data appears to be empty!' ),
                'iptc' : rendered( iptc_dict, 'IPTC data appears to be empty!' ),
                'text' : file_text,
                'extra' : [ list( row ) for row in extra_rows ],
            }


    json.dump( out, sys.stdout, indent = 1, ensure_ascii = False )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
