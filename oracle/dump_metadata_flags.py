#!/usr/bin/env python3
"""Record the reference's XMP, IPTC and software/source flags.

    python oracle/dump_metadata_flags.py generate   # writes oracle/fixtures/metadata/*
    python oracle/dump_metadata_flags.py record     # writes oracle/fixtures/metadata_flags.json
    python oracle/dump_metadata_flags.py all        # both

`generate` writes small images carrying XMP packets, IPTC records and
software/source text in the places Pillow finds them (and some near misses).
`record` runs the import job's checks (ClientImportFiles.FileImportJob,
GenerateInfo) over those files and the media corpus, in the same order on
one opened image, as the import does.
"""
import io
import json
import os
import struct
import sys
import zlib

ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )
sys.path.insert( 0, ROOT )
sys.path.insert( 0, os.path.join( ROOT, 'oracle' ) )

from PIL import Image, PngImagePlugin

import dump_media

META_DIR = os.path.join( ROOT, 'oracle', 'fixtures', 'metadata' )
MEDIA_DIR = os.path.join( ROOT, 'oracle', 'fixtures', 'media' )
JSON_PATH = os.path.join( ROOT, 'oracle', 'fixtures', 'metadata_flags.json' )

XMP = (
    '<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>'
    '<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">'
    '<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/">'
    '<dc:creator>someone</dc:creator></rdf:Description></rdf:RDF></x:xmpmeta>'
    '<?xpacket end="w"?>'
).encode( 'utf-8' )


def image():
    return Image.fromarray( dump_media.photo( 48, 32, 7 ) )


def out( name ):
    return os.path.join( META_DIR, name )


def iptc_field( record, dataset, value: bytes ):
    if len( value ) < 0x8000:
        return bytes( [ 0x1C, record, dataset ] ) + struct.pack( '>H', len( value ) ) + value
    # extended size: 4 bytes of length
    return bytes( [ 0x1C, record, dataset, 0x80, 0x04 ] ) + struct.pack( '>I', len( value ) ) + value


def photoshop_block( iptc: bytes ):
    data = b'8BIM' + struct.pack( '>H', 0x0404 ) + b'\x00\x00' + struct.pack( '>I', len( iptc ) ) + iptc
    if len( iptc ) % 2:
        data += b'\x00'
    return b'Photoshop 3.0\x00' + data


def jpeg_with_segment( marker: int, payload: bytes ):
    b = io.BytesIO()
    image().save( b, 'JPEG', quality = 85 )
    data = b.getvalue()
    segment = struct.pack( '>BBH', 0xFF, marker, len( payload ) + 2 ) + payload
    # after SOI
    return data[ : 2 ] + segment + data[ 2 : ]


def png_with_chunks( chunks, after_idat = () ):
    b = io.BytesIO()
    image().save( b, 'PNG' )
    data = b.getvalue()

    def chunk( kind: bytes, body: bytes ):
        return struct.pack( '>I', len( body ) ) + kind + body + struct.pack( '>I', zlib.crc32( kind + body ) & 0xFFFFFFFF )

    # chunks go after IHDR (8 + 25 bytes), late ones before IEND
    head = data[ : 33 ]
    rest = data[ 33 : -12 ]
    tail = data[ -12 : ]
    return head + b''.join( chunk( k, v ) for ( k, v ) in chunks ) + rest + b''.join( chunk( k, v ) for ( k, v ) in after_idat ) + tail


def itxt( key: bytes, text: bytes, compressed = False ):
    if compressed:
        return b'iTXt', key + b'\x00\x01\x00' + b'\x00' + b'\x00' + zlib.compress( text )
    return b'iTXt', key + b'\x00\x00\x00' + b'\x00' + b'\x00' + text


def text( key: bytes, value: bytes ):
    return b'tEXt', key + b'\x00' + value


def generate():
    os.makedirs( META_DIR, exist_ok = True )

    def write( name, data ):
        with open( out( name ), 'wb' ) as f:
            f.write( data )

    adobe = b'http://ns.adobe.com/xap/1.0/\x00'
    write( 'jpeg_xmp.jpg', jpeg_with_segment( 0xE1, adobe + XMP ) )
    write( 'jpeg_xmp_padded.jpg', jpeg_with_segment( 0xE1, adobe + XMP + b'\x00\x00   ' ) )
    write( 'jpeg_xmp_not_utf8.jpg', jpeg_with_segment( 0xE1, adobe + b'<x>\xff\xfe</x>' ) )
    write( 'jpeg_xmp_empty.jpg', jpeg_with_segment( 0xE1, adobe ) )

    keywords = iptc_field( 2, 0, b'\x00\x04' ) + iptc_field( 2, 25, b'blue eyes' ) + iptc_field( 2, 25, b'metroid' )
    write( 'jpeg_iptc_keywords.jpg', jpeg_with_segment( 0xED, photoshop_block( keywords ) ) )
    write( 'jpeg_iptc_caption.jpg', jpeg_with_segment( 0xED, photoshop_block( iptc_field( 2, 120, 'ümlaut caption'.encode( 'utf-8' ) ) ) ) )
    write( 'jpeg_iptc_version_only.jpg', jpeg_with_segment( 0xED, photoshop_block( iptc_field( 2, 0, b'\x00\x04' ) ) ) )
    write( 'jpeg_iptc_blank.jpg', jpeg_with_segment( 0xED, photoshop_block( iptc_field( 2, 120, b'  \t ' ) ) ) )
    write( 'jpeg_iptc_empty_field.jpg', jpeg_with_segment( 0xED, photoshop_block( iptc_field( 2, 5, b'' ) ) ) )
    write( 'jpeg_iptc_invalid.jpg', jpeg_with_segment( 0xED, photoshop_block( b'\x1d\x02\x19\x00\x03abc' ) ) )
    write( 'jpeg_iptc_truncated.jpg', jpeg_with_segment( 0xED, photoshop_block( iptc_field( 2, 105, b'headline' ) + b'\x1c\x02' ) ) )
    write( 'jpeg_iptc_long.jpg', jpeg_with_segment( 0xED, photoshop_block( iptc_field( 2, 120, b'x' * 40000 ) ) ) )
    write( 'jpeg_iptc_after_unwanted.jpg', jpeg_with_segment( 0xED, photoshop_block( iptc_field( 1, 90, b'\x1b%G' ) + iptc_field( 2, 80, b'by someone' ) ) ) )
    write( 'jpeg_photoshop_no_iptc.jpg', jpeg_with_segment( 0xED, b'Photoshop 3.0\x00' + b'8BIM' + struct.pack( '>H', 0x0425 ) + b'\x00\x00' + struct.pack( '>I', 16 ) + b'\x00' * 16 ) )

    write( 'png_xmp.png', png_with_chunks( [ itxt( b'XML:com.adobe.xmp', XMP ) ] ) )
    write( 'png_xmp_compressed.png', png_with_chunks( [ itxt( b'XML:com.adobe.xmp', XMP, compressed = True ) ] ) )
    write( 'png_xmp_late.png', png_with_chunks( [], after_idat = [ itxt( b'XML:com.adobe.xmp', XMP ) ] ) )
    write( 'png_xmp_not_utf8.png', png_with_chunks( [ itxt( b'XML:com.adobe.xmp', b'<x>\xff</x>' ) ] ) )
    write( 'png_xmp_as_text.png', png_with_chunks( [ text( b'XML:com.adobe.xmp', XMP ) ] ) )
    write( 'png_comment_created_with.png', png_with_chunks( [ text( b'Comment', b'Created with GIMP' ) ] ) )
    write( 'png_comment_plain.png', png_with_chunks( [ text( b'Comment', b'a nice picture' ) ] ) )
    write( 'png_creator.png', png_with_chunks( [ text( b'Creator', b'some program' ) ] ) )
    write( 'png_source_blank.png', png_with_chunks( [ text( b'Source', b'   ' ) ] ) )
    write( 'png_software_late.png', png_with_chunks( [], after_idat = [ text( b'Software', b'late program' ) ] ) )
    write( 'png_software_itxt.png', png_with_chunks( [ itxt( b'software', 'prögram'.encode( 'utf-8' ) ) ] ) )
    write( 'png_lowercase_comment.png', png_with_chunks( [ text( b'comment', b'edited with something' ) ] ) )

    b = io.BytesIO()
    image().save( b, 'WEBP', xmp = XMP, quality = 80 )
    write( 'webp_xmp.webp', b.getvalue() )

    b = io.BytesIO()
    image().save( b, 'TIFF', tiffinfo = { 700 : XMP } )
    write( 'tiff_xmp.tiff', b.getvalue() )

    b = io.BytesIO()
    image().save( b, 'TIFF', tiffinfo = { 33723 : keywords } )
    write( 'tiff_iptc.tiff', b.getvalue() )

    b = io.BytesIO()
    image().convert( 'P' ).save( b, 'GIF', comment = b'Created with GIMP' )
    write( 'gif_comment_created_with.gif', b.getvalue() )

    b = io.BytesIO()
    image().save( b, 'JPEG', quality = 85, comment = b'Created with GIMP' )
    write( 'jpeg_comment_created_with.jpg', b.getvalue() )


def record_file( path ):
    """GenerateInfo's flag checks, on one raw image opened once."""
    from hydrus.core import HydrusConstants as HC
    from hydrus.core.files import HydrusFileHandling
    from hydrus.core.files.images import HydrusImageOpening, HydrusImageMetadata
    from hydrus.client.files.images import ClientImageMetadata

    rec = { 'file' : os.path.relpath( path, os.path.join( ROOT, 'oracle', 'fixtures' ) ) }
    try:
        mime = HydrusFileHandling.GetMime( path )
    except Exception as e:
        rec[ 'mime_error' ] = type( e ).__name__
        return rec
    rec[ 'mime' ] = mime

    has_xmp = False
    has_iptc = False
    has_software_source = False
    raw = None

    def opened():
        nonlocal raw
        if raw is None:
            raw = HydrusImageOpening.RawOpenPILImage( path )
        return raw

    # (the EXIF check comes first, and can load the image)
    if mime in HC.FILES_THAT_CAN_HAVE_EXIF:
        try:
            HydrusImageMetadata.HasEXIF( opened() )
        except Exception:
            pass
    if mime in HC.FILES_THAT_CAN_HAVE_XMP:
        try:
            has_xmp = ClientImageMetadata.HasXMP( opened() )
        except Exception:
            pass
    if mime in HC.FILES_THAT_CAN_HAVE_IPTC:
        try:
            has_iptc = ClientImageMetadata.HasIPTC( opened() )
        except Exception:
            pass
    if mime in HC.FILES_THAT_CAN_HAVE_SOFTWARE_SOURCE:
        try:
            has_software_source = HydrusImageMetadata.HasSoftwareSource( opened() )
        except Exception:
            pass
    if raw is not None:
        raw.close()

    rec[ 'has_xmp' ] = bool( has_xmp )
    rec[ 'has_iptc' ] = bool( has_iptc )
    rec[ 'has_software_source' ] = bool( has_software_source )
    return rec


def record():
    dump_media.setup_reference()
    paths = [ os.path.join( MEDIA_DIR, n ) for n in sorted( os.listdir( MEDIA_DIR ) ) ]
    paths += [ os.path.join( META_DIR, n ) for n in sorted( os.listdir( META_DIR ) ) ]
    files = [ record_file( p ) for p in paths ]
    with open( JSON_PATH, 'w' ) as f:
        json.dump( { 'files' : files }, f, indent = 1, sort_keys = True )
        f.write( '\n' )
    flagged = sum( 1 for r in files if r.get( 'has_xmp' ) or r.get( 'has_iptc' ) or r.get( 'has_software_source' ) )
    print( f'recorded {len( files )} files, {flagged} with a flag' )


if __name__ == '__main__':
    what = sys.argv[ 1 ] if len( sys.argv ) > 1 else 'all'
    if what in ( 'generate', 'all' ):
        generate()
    if what in ( 'record', 'all' ):
        record()
