#!/usr/bin/env python3
"""Generate a media corpus and record the reference implementation's view of it.

Two phases, both deterministic:

    python oracle/dump_media.py generate   # writes oracle/fixtures/media/*
    python oracle/dump_media.py record     # writes oracle/fixtures/media.json
    python oracle/dump_media.py all        # both

`generate` synthesises small test files with Pillow, numpy, ffmpeg and a few
hand-rolled container writers (OLE, PSD, SWF, CLIP, ...). The corpus is
committed, so regenerating it is only needed when adding cases; ffmpeg or
encoder upgrades may change the bytes.

`record` runs the unmodified reference code (hydrus/) over every corpus file,
mirroring the file import job (ClientImportFiles.FileImportJob.GenerateInfo):
mime detection, file info, hashes, thumbnail geometry, blurhash, perceptual
and pixel hashes, and the transparency/EXIF/ICC/metadata flags. It also
records a few pure functions (thumbnail resolution, OpenCV resizes) on
synthetic inputs so the Rust ports can be checked in isolation.

Needs the reference client's venv (oracle/requirements.txt), ffmpeg on PATH,
and QT_QPA_PLATFORM=offscreen (PDF and SVG support comes from Qt).
"""
import base64
import gzip
import hashlib
import io
import json
import lzma
import os
import plistlib
import sqlite3
import struct
import subprocess
import sys
import tempfile
import time
import uuid
import zipfile
import zlib

os.environ.setdefault( 'QT_QPA_PLATFORM', 'offscreen' )

ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )
sys.path.insert( 0, ROOT )

import numpy as np
from PIL import Image, ImageCms, PngImagePlugin

MEDIA_DIR = os.path.join( ROOT, 'oracle', 'fixtures', 'media' )
JSON_PATH = os.path.join( ROOT, 'oracle', 'fixtures', 'media.json' )

ZIP_DATE = ( 2020, 1, 1, 0, 0, 0 )
FIXED_ICC_DATE = struct.pack( '>6H', 2020, 1, 1, 0, 0, 0 )

# ---------------------------------------------------------------------------
# synthetic pixels

def photo( w, h, seed ):
    """A smooth, photo-ish RGB image with some structure, so perceptual hashes are not degenerate."""
    rng = np.random.default_rng( seed )
    y, x = np.mgrid[ 0 : h, 0 : w ].astype( np.float64 )
    out = np.zeros( ( h, w, 3 ) )
    for c in range( 3 ):
        acc = np.zeros( ( h, w ) )
        for _ in range( 3 ):
            fx = rng.uniform( 0.3, 3.0 ) / max( w, 1 )
            fy = rng.uniform( 0.3, 3.0 ) / max( h, 1 )
            phase = rng.uniform( 0, 2 * np.pi )
            acc += np.sin( 2 * np.pi * ( fx * x + fy * y ) + phase )
        out[ :, :, c ] = 128 + 40 * acc
    for _ in range( 6 ):
        cx, cy = rng.uniform( 0, w ), rng.uniform( 0, h )
        rx, ry = rng.uniform( 0.05, 0.3 ) * w + 1, rng.uniform( 0.05, 0.3 ) * h + 1
        colour = rng.uniform( 0, 255, 3 )
        mask = ( ( x - cx ) / rx ) ** 2 + ( ( y - cy ) / ry ) ** 2 <= 1
        out[ mask ] = 0.3 * out[ mask ] + 0.7 * colour
    out += rng.normal( 0, 4, out.shape )
    return np.clip( out, 0, 255 ).astype( np.uint8 )


def alpha_circle( w, h, seed, hard = False ):
    rng = np.random.default_rng( seed )
    y, x = np.mgrid[ 0 : h, 0 : w ].astype( np.float64 )
    cx, cy = w / 2 + rng.uniform( -w / 8, w / 8 ), h / 2 + rng.uniform( -h / 8, h / 8 )
    r = min( w, h ) * 0.4
    d = np.sqrt( ( x - cx ) ** 2 + ( y - cy ) ** 2 )
    if hard:
        a = np.where( d <= r, 255, 0 )
    else:
        a = np.clip( ( r - d ) * 8 + 128, 0, 255 )
    return a.astype( np.uint8 )


def rgba( w, h, seed, hard = False ):
    return np.dstack( [ photo( w, h, seed ), alpha_circle( w, h, seed, hard ) ] )


def pil( arr ):
    return Image.fromarray( arr )

# ---------------------------------------------------------------------------
# writing helpers

def out_path( name ):
    return os.path.join( MEDIA_DIR, name )


def write_bytes( name, data ):
    with open( out_path( name ), 'wb' ) as f:
        f.write( data )


def ffmpeg( name, *args ):
    cmd = [ 'ffmpeg', '-y', '-loglevel', 'error', '-nostdin' ] + list( args ) + [ '-map_metadata', '-1', '-fflags', '+bitexact', '-flags:v', '+bitexact', '-flags:a', '+bitexact', out_path( name ) ]
    subprocess.run( cmd, check = True )


def ffmpeg_raw( name, *args ):
    cmd = [ 'ffmpeg', '-y', '-loglevel', 'error', '-nostdin' ] + list( args ) + [ out_path( name ) ]
    subprocess.run( cmd, check = True )


def png_chunk( ctype, data ):
    return struct.pack( '>I', len( data ) ) + ctype + data + struct.pack( '>I', zlib.crc32( ctype + data ) & 0xffffffff )


def png_chunks( data ):
    chunks = []
    i = 8
    while i + 8 <= len( data ):
        ( n, ) = struct.unpack( '>I', data[ i : i + 4 ] )
        chunks.append( ( data[ i + 4 : i + 8 ], data[ i + 8 : i + 8 + n ] ) )
        i += 12 + n
    return chunks


def png_from_chunks( chunks ):
    return b'\x89PNG\r\n\x1a\n' + b''.join( png_chunk( t, d ) for ( t, d ) in chunks )


def png_bytes( im, **kwargs ):
    f = io.BytesIO()
    im.save( f, 'PNG', **kwargs )
    return f.getvalue()


def jpeg_bytes( im, **kwargs ):
    f = io.BytesIO()
    im.save( f, 'JPEG', **kwargs )
    return f.getvalue()


def fix_icc_date( icc ):
    return icc[ : 24 ] + FIXED_ICC_DATE + icc[ 36 : ]


def srgb_icc():
    return fix_icc_date( ImageCms.ImageCmsProfile( ImageCms.createProfile( 'sRGB' ) ).tobytes() )


def p3_icc():
    from hydrus.core.files.images import HydrusImageICCProfiles
    from hydrus.core.files.images import HydrusImageMathSlop as MathSlop
    white = ( 0.3127, 0.3290 )
    m = MathSlop.chromaticities_to_rgb_to_xyz( white, ( 0.680, 0.320 ), ( 0.265, 0.690 ), ( 0.150, 0.060 ) )
    m50 = MathSlop.adapt_rgb_to_xyz_matrix_to_D50( m, white )
    return fix_icc_date( HydrusImageICCProfiles.make_gamma_and_chromaticity_icc_profile( 2.2, m50 ) )


def zip_bytes( entries, first_stored = False ):
    """entries: list of (name, bytes). Deterministic timestamps."""
    f = io.BytesIO()
    with zipfile.ZipFile( f, 'w' ) as z:
        for ( i, ( name, data ) ) in enumerate( entries ):
            info = zipfile.ZipInfo( name, date_time = ZIP_DATE )
            info.compress_type = zipfile.ZIP_STORED if ( first_stored and i == 0 ) else zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr( info, data )
    return f.getvalue()

# ---------------------------------------------------------------------------
# hand-rolled containers

def zero_isobmff_times( data ):
    """Blank the creation/modification times libavif writes into mvhd/tkhd/mdhd."""
    data = bytearray( data )
    for kind in ( b'mvhd', b'tkhd', b'mdhd' ):
        at = data.find( kind )
        while at != -1:
            version = data[ at + 4 ]
            width = 8 if version == 1 else 4
            data[ at + 8 : at + 8 + 2 * width ] = bytes( 2 * width )
            at = data.find( kind, at + 4 )
    return bytes( data )


def interlaced_png( arr ):
    """8-bit RGB Adam7 PNG (neither Pillow nor OpenCV write interlaced PNGs)."""
    h, w, _ = arr.shape
    passes = [ ( 0, 0, 8, 8 ), ( 4, 0, 8, 8 ), ( 0, 4, 4, 8 ), ( 2, 0, 4, 4 ), ( 0, 2, 2, 4 ), ( 1, 0, 2, 2 ), ( 0, 1, 1, 2 ) ]
    raw = b''
    for ( x0, y0, dx, dy ) in passes:
        sub = arr[ y0 : : dy, x0 : : dx ]
        if sub.shape[ 0 ] == 0 or sub.shape[ 1 ] == 0:
            continue
        for row in sub:
            raw += b'\x00' + row.tobytes()
    ihdr = struct.pack( '>IIBBBBB', w, h, 8, 2, 0, 0, 1 )
    return png_from_chunks( [ ( b'IHDR', ihdr ), ( b'IDAT', zlib.compress( raw, 9 ) ), ( b'IEND', b'' ) ] )


def swf( version, width, height, fps, frames, compression ):
    nbits = 16
    values = [ 0, width * 20, 0, height * 20 ]
    bits = format( nbits, '05b' ) + ''.join( format( v, '0{}b'.format( nbits ) ) for v in values )
    bits += '0' * ( -len( bits ) % 8 )
    rect = int( bits, 2 ).to_bytes( len( bits ) // 8, 'big' )
    body = rect + struct.pack( '<HH', fps << 8, frames ) + b'\x40\x00' + b'\x00\x00'  # ShowFrame, End
    total = 8 + len( body )
    if compression == 'F':
        return b'FWS' + bytes( [ version ] ) + struct.pack( '<I', total ) + body
    if compression == 'C':
        return b'CWS' + bytes( [ version ] ) + struct.pack( '<I', total ) + zlib.compress( body )
    alone = lzma.compress( body, format = lzma.FORMAT_ALONE )
    props, data = alone[ : 5 ], alone[ 13 : ]
    return b'ZWS' + bytes( [ version ] ) + struct.pack( '<I', total ) + struct.pack( '<I', len( data ) ) + props + data


def psd( arr, icc = None ):
    h, w, c = arr.shape
    out = b'8BPS' + struct.pack( '>H', 1 ) + b'\x00' * 6 + struct.pack( '>HIIHH', c, h, w, 8, 3 )
    out += struct.pack( '>I', 0 )  # colour mode data
    resources = b''
    def resource( rid, data ):
        r = b'8BIM' + struct.pack( '>H', rid ) + b'\x00\x00' + struct.pack( '>I', len( data ) ) + data
        if len( data ) % 2:
            r += b'\x00'
        return r
    resources += resource( 1005, struct.pack( '>IHHIHH', 72 << 16, 1, 1, 72 << 16, 1, 1 ) )
    if icc is not None:
        resources += resource( 1039, icc )
    out += struct.pack( '>I', len( resources ) ) + resources
    out += struct.pack( '>I', 0 )  # layer and mask info
    out += struct.pack( '>H', 0 )  # raw image data
    for ch in range( c ):
        out += arr[ :, :, ch ].tobytes()
    return out


def clip( w, h, unit, dpi, timeline, preview ):
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join( d, 'c.sqlite' )
        db = sqlite3.connect( p )
        db.execute( 'CREATE TABLE Canvas ( CanvasWidth REAL, CanvasHeight REAL, CanvasUnit INTEGER, CanvasResolution REAL );' )
        db.execute( 'INSERT INTO Canvas VALUES ( ?, ?, ?, ? );', ( w, h, unit, dpi ) )
        db.execute( 'CREATE TABLE CanvasPreview ( ImageData BLOB );' )
        db.execute( 'INSERT INTO CanvasPreview VALUES ( ? );', ( sqlite3.Binary( preview ), ) )
        if timeline is not None:
            db.execute( 'CREATE TABLE TimeLine ( StartFrame REAL, FrameRate REAL, EndFrame REAL );' )
            db.execute( 'INSERT INTO TimeLine VALUES ( ?, ?, ? );', timeline )
        db.commit()
        db.close()
        with open( p, 'rb' ) as f:
            sqlite_bytes = f.read()
    return b'CSFCHUNK' + b'\x00' * 8 + b'CHNKHead' + b'\x00' * 40 + sqlite_bytes


def pdn( w, h, thumb_png ):
    xml = '<pdnImage width="{}" height="{}" layerCount="1" savedWithVersion="4.0.0"><custom><thumb png="{}" /></custom></pdnImage>'.format( w, h, base64.b64encode( thumb_png ).decode( 'ascii' ) ).encode( 'utf-8' )
    return b'PDN3' + struct.pack( '<I', len( xml ) )[ : 3 ] + xml + b'\x00\x01' + b'\x00' * 64


def ole( root_clsid = None, app_name = None, num_words = None, pad_streams = True ):
    """Minimal CFB v3 file: root storage + \\x05SummaryInformation + WordDocument, all in regular sectors."""
    FREE, END, FATSECT, NOSTREAM = 0xFFFFFFFF, 0xFFFFFFFE, 0xFFFFFFFD, 0xFFFFFFFF
    props = []
    props.append( ( 1, struct.pack( '<IH', 2, 1252 ) + b'\x00\x00' ) )
    if num_words is not None:
        props.append( ( 15, struct.pack( '<Ii', 3, num_words ) ) )
    if app_name is not None:
        s = app_name.encode( 'latin-1' ) + b'\x00'
        s += b'\x00' * ( -len( s ) % 4 )
        props.append( ( 18, struct.pack( '<II', 30, len( app_name ) + 1 ) + s ) )
    header_len = 8 + 8 * len( props )
    offsets = []
    body = b''
    for ( pid, data ) in props:
        offsets.append( header_len + len( body ) )
        body += data
    section = struct.pack( '<II', header_len + len( body ), len( props ) ) + b''.join( struct.pack( '<II', pid, off ) for ( ( pid, _ ), off ) in zip( props, offsets ) ) + body
    fmtid = uuid.UUID( 'F29F85E0-4FF9-1068-AB91-08002B27B3D9' ).bytes_le
    summary = struct.pack( '<HHI', 0xFFFE, 0, 0x00020006 ) + b'\x00' * 16 + struct.pack( '<I', 1 ) + fmtid + struct.pack( '<I', 48 ) + section
    summary = summary.ljust( 4096, b'\x00' )
    word = b'\xec\xa5\xc1\x00'.ljust( 4096, b'\x00' )
    sectors_per_stream = 8
    fat = [ FATSECT, END ]
    for start in ( 2, 2 + sectors_per_stream ):
        for i in range( sectors_per_stream ):
            fat.append( start + i + 1 if i < sectors_per_stream - 1 else END )
    fat += [ FREE ] * ( 128 - len( fat ) )
    def entry( name, etype, left, right, child, clsid, start, size ):
        n = ( name + '\x00' ).encode( 'utf-16-le' ) if name else b''
        return n.ljust( 64, b'\x00' ) + struct.pack( '<HBBIII', len( n ), etype, 1, left, right, child ) + clsid + struct.pack( '<I', 0 ) + b'\x00' * 16 + struct.pack( '<III', start, size, 0 )
    clsid = uuid.UUID( root_clsid ).bytes_le if root_clsid else b'\x00' * 16
    directory = entry( 'Root Entry', 5, NOSTREAM, NOSTREAM, 1, clsid, END, 0 )
    directory += entry( '\x05SummaryInformation', 2, 2, NOSTREAM, NOSTREAM, b'\x00' * 16, 2, 4096 )
    directory += entry( 'WordDocument', 2, NOSTREAM, NOSTREAM, NOSTREAM, b'\x00' * 16, 2 + sectors_per_stream, 4096 )
    directory += entry( '', 0, NOSTREAM, NOSTREAM, NOSTREAM, b'\x00' * 16, 0, 0 )
    header = b'\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1' + b'\x00' * 16 + struct.pack( '<HHHHH', 0x3E, 3, 0xFFFE, 9, 6 ) + b'\x00' * 6
    header += struct.pack( '<IIIIIIIII', 0, 1, 1, 0, 4096, END, 0, END, 0 )
    header += struct.pack( '<I', 0 ) + struct.pack( '<I', FREE ) * 108
    return header + struct.pack( '<128I', *fat ) + directory + summary + word


def procreate_archive( size_string, orientation, class_name = 'SilicaDocument' ):
    objects = [
        '$null',
        { '$class' : plistlib.UID( 3 ), 'size' : plistlib.UID( 2 ), 'orientation' : orientation, 'name' : 'test' },
        size_string,
        { '$classname' : class_name, '$classes' : [ class_name, 'NSObject' ] },
    ]
    doc = { '$archiver' : 'NSKeyedArchiver', '$version' : 100000, '$top' : { 'root' : plistlib.UID( 1 ) }, '$objects' : objects }
    return plistlib.dumps( doc, fmt = plistlib.FMT_BINARY, sort_keys = True )


def epub( cover_style, cover_bytes, cover_name = 'cover.jpg' ):
    container = '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>'
    if cover_style == 'epub3':
        manifest = f'<item id="c" href="images/{cover_name}" media-type="image/jpeg" properties="cover-image"/>'
        meta = ''
    elif cover_style == 'epub2':
        manifest = f'<item id="cover-img" href="images/{cover_name}" media-type="image/jpeg"/>'
        meta = '<meta name="cover" content="cover-img"/>'
    else:
        manifest = '<item id="text" href="text.xhtml" media-type="application/xhtml+xml"/>'
        meta = ''
    opf = f'<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata>{meta}</metadata><manifest>{manifest}</manifest><spine><itemref idref="text"/></spine></package>'
    entries = [ ( 'mimetype', b'application/epub+zip' ), ( 'META-INF/container.xml', container.encode() ), ( 'OEBPS/content.opf', opf.encode() ), ( 'OEBPS/text.xhtml', b'<html><body>hi</body></html>' ) ]
    if cover_bytes is not None:
        entries.append( ( f'OEBPS/images/{cover_name}', cover_bytes ) )
    return zip_bytes( entries, first_stored = True )


def office( kind, words = None, default_style = False, thumbnail = None ):
    main = { 'docx' : ( '/word/document.xml', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml' ),
             'xlsx' : ( '/xl/workbook.xml', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml' ),
             'pptx' : ( '/ppt/presentation.xml', 'application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml' ) }[ kind ]
    if default_style:
        types = f'<Default Extension="xml" ContentType="{main[ 1 ]}"/>'
    else:
        types = f'<Default Extension="xml" ContentType="application/xml"/><Override PartName="{main[ 0 ]}" ContentType="{main[ 1 ]}"/>'
    ct = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{types}</Types>'
    entries = [ ( '[Content_Types].xml', ct.encode() ) ]
    if words is not None:
        app = f'<?xml version="1.0" encoding="UTF-8"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Words>{words}</Words></Properties>'
        entries.append( ( 'docProps/app.xml', app.encode() ) )
    if kind == 'pptx':
        pres = '<?xml version="1.0"?><p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldSz cx="9144000" cy="5143500"/></p:presentation>'
        entries.append( ( 'ppt/presentation.xml', pres.encode() ) )
    else:
        entries.append( ( main[ 0 ][ 1 : ], b'<?xml version="1.0"?><doc/>' ) )
    if thumbnail is not None:
        entries.append( ( 'docProps/thumbnail.jpeg', thumbnail ) )
    return zip_bytes( entries )


def set_zip_encrypted_flag( data ):
    """Flip general purpose bit 0 on every local and central header (content stays readable)."""
    b = bytearray( data )
    i = 0
    while True:
        i = b.find( b'PK\x03\x04', i )
        if i < 0:
            break
        b[ i + 6 ] |= 1
        i += 4
    i = 0
    while True:
        i = b.find( b'PK\x01\x02', i )
        if i < 0:
            break
        b[ i + 8 ] |= 1
        i += 4
    return bytes( b )


PDF_PAD = bytes.fromhex( '28BF4E5E4E758A4164004E56FFFA01082E2E00B6D0683E802F0CA9FE6453697A' )


def rc4( key, data ):
    s = list( range( 256 ) )
    j = 0
    for i in range( 256 ):
        j = ( j + s[ i ] + key[ i % len( key ) ] ) % 256
        s[ i ], s[ j ] = s[ j ], s[ i ]
    out = bytearray()
    i = j = 0
    for byte in data:
        i = ( i + 1 ) % 256
        j = ( j + s[ i ] ) % 256
        s[ i ], s[ j ] = s[ j ], s[ i ]
        out.append( byte ^ s[ ( s[ i ] + s[ j ] ) % 256 ] )
    return bytes( out )


def pdf_literal( text ):
    return b'(' + text.replace( b'\\', b'\\\\' ).replace( b'(', b'\\(' ).replace( b')', b'\\)' ) + b')'


def pdf_document( objects, info = None, user_password = None, owner_password = b'owner' ):
    """A classic-xref PDF. `objects` are 1-indexed bodies: bytes, or (dict_bytes, stream_data)
    for streams (Length is added). Strings to encrypt are given as ('str', bytes). With
    `user_password` set, everything is RC4-40 encrypted (standard security handler, R2)."""
    file_id = hashlib.md5( b'hydrus-rs oracle' ).digest()
    key = None
    extra = []
    if user_password is not None:
        permissions = -4
        o_key = hashlib.md5( ( owner_password + PDF_PAD )[ : 32 ] ).digest()[ : 5 ]
        o_entry = rc4( o_key, ( user_password + PDF_PAD )[ : 32 ] )
        key = hashlib.md5( ( user_password + PDF_PAD )[ : 32 ] + o_entry + struct.pack( '<i', permissions ) + file_id ).digest()[ : 5 ]
        u_entry = rc4( key, PDF_PAD )
        extra.append( b'<< /Filter /Standard /V 1 /R 2 /O <' + o_entry.hex().encode() + b'> /U <' + u_entry.hex().encode() + b'> /P ' + str( permissions ).encode() + b' >>' )
    all_objects = list( objects ) + ( [ info ] if info is not None else [] ) + extra
    info_num = len( objects ) + 1 if info is not None else None
    encrypt_num = len( all_objects ) if extra else None

    def crypt( num, data ):
        if key is None or num == encrypt_num:
            return data
        obj_key = hashlib.md5( key + struct.pack( '<I', num )[ : 3 ] + b'\x00\x00' ).digest()[ : 10 ]
        return rc4( obj_key, data )

    def render( num, value ):
        if isinstance( value, tuple ) and value[ 0 ] == 'str':
            return b'<' + crypt( num, value[ 1 ] ).hex().encode() + b'>'
        if isinstance( value, list ):
            return b''.join( render( num, v ) for v in value )
        return value

    out = b'%PDF-1.4\n%\xe2\xe3\xcf\xd3\n'
    offsets = []
    for ( i, body ) in enumerate( all_objects ):
        num = i + 1
        offsets.append( len( out ) )
        if isinstance( body, tuple ) and body[ 0 ] != 'str':
            ( d, data ) = body
            data = crypt( num, data )
            body = d[ : -2 ] + b' /Length ' + str( len( data ) ).encode() + b' >>\nstream\n' + data + b'\nendstream'
        else:
            body = render( num, body )
        out += f'{num} 0 obj\n'.encode() + body + b'\nendobj\n'
    xref = len( out )
    out += f'xref\n0 {len( all_objects ) + 1}\n0000000000 65535 f \n'.encode()
    for off in offsets:
        out += f'{off:010d} 00000 n \n'.encode()
    trailer = f'<< /Size {len( all_objects ) + 1} /Root 1 0 R'.encode()
    if info_num:
        trailer += f' /Info {info_num} 0 R'.encode()
    if encrypt_num:
        trailer += f' /Encrypt {encrypt_num} 0 R'.encode()
    trailer += b' /ID [<' + file_id.hex().encode() + b'> <' + file_id.hex().encode() + b'>] >>'
    out += b'trailer\n' + trailer + f'\nstartxref\n{xref}\n%%EOF\n'.encode()
    return out


HELVETICA = b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'
TIMES = b'<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman /Encoding /WinAnsiEncoding >>'


def text_pdf( words ):
    """A one-page PDF with a line of Helvetica text, 400x300pt."""
    content = 'BT /F1 18 Tf 20 150 Td ({}) Tj ET'.format( words ).encode( 'latin-1' )
    objs = [
        b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
        b'<< /Length ' + str( len( content ) ).encode() + b' >>\nstream\n' + content + b'\nendstream',
        b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
        b'<< /Title (Oracle Test) /Author (hydrus-rs) >>',
    ]
    out = b'%PDF-1.4\n'
    offsets = []
    for ( i, o ) in enumerate( objs ):
        offsets.append( len( out ) )
        out += f'{i + 1} 0 obj\n'.encode() + o + b'\nendobj\n'
    xref = len( out )
    out += f'xref\n0 {len( objs ) + 1}\n0000000000 65535 f \n'.encode()
    for off in offsets:
        out += f'{off:010d} 00000 n \n'.encode()
    out += f'trailer\n<< /Size {len( objs ) + 1} /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode()
    return out


def multi_page_pdf():
    """Two pages: inherited MediaBox, a CropBox, /Rotate, compressed content, several
    lines, TJ kerning and word gaps made by positioning alone."""
    page1 = b'\n'.join( [
        b'BT /F1 14 Tf 16 TL 60 700 Td (The quick brown fox) Tj T* (jumps over the lazy dog.) Tj',
        b'0 -32 Td [(Kern)-40(ed te)-30(xt)-600(and a gap)] TJ',
        b'0 -20 Td (positioned) Tj 90 0 Td (words) Tj 45 0 Td (here) Tj',
        b"(and a quote op) ' ET",
        b'BT /F2 11 Tf 1 0 0 1 60 560 Tm (Times: caf\xe9, na\xefve, \x93quoted\x94 \x97 dash) Tj ET',
        b'BT /F2 11 Tf 1 0 0 1 60 540 Tm (hyphen-ated_under score 3.14 50% x/y) Tj ET',
    ] )
    page2 = b'BT /F1 20 Tf 100 100 Td (Second page!) Tj ET\nBT /F1 20 Tf 100 60 Td (  spaced   out  ) Tj ET'
    resources = b'/Resources << /Font << /F1 5 0 R /F2 6 0 R >> >>'
    objs = [
        b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 612 792] /Rotate 90 >>',
        b'<< /Type /Page /Parent 2 0 R /CropBox [36 36 576.5 700] /Contents 7 0 R ' + resources + b' >>',
        b'<< /Type /Page /Parent 2 0 R /Rotate 0 /Contents 8 0 R ' + resources + b' >>',
        HELVETICA,
        TIMES,
        ( b'<< /Filter /FlateDecode >>', zlib.compress( page1 ) ),
        ( b'<< >>', page2 ),
    ]
    info = [ b'<< /Title ', ( 'str', b'\xfe\xff' + 'Ünïcødé title'.encode( 'utf-16-be' ) ), b' /Producer (not counted) >>' ]
    return pdf_document( objs, info = info )


def blank_pdf( info = None, user_password = None, media_box = b'[0 0 595 842]', rotate = b'' ):
    content = b'BT /F1 12 Tf 50 50 Td (' + ( b'secret words' if user_password is not None else b'' ) + b') Tj ET'
    objs = [
        b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
        b'<< /Type /Page /Parent 2 0 R /MediaBox ' + media_box + rotate + b' /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
        ( b'<< >>', content ),
        HELVETICA,
    ]
    return pdf_document( objs, info = info, user_password = user_password )


# ---------------------------------------------------------------------------
# the corpus

def gen_jpeg():
    base = pil( photo( 311, 237, 1 ) )
    base.save( out_path( 'jpeg_420.jpg' ), quality = 85 )
    pil( photo( 640, 480, 2 ) ).save( out_path( 'jpeg_444_q95.jpg' ), quality = 95, subsampling = 0 )
    pil( photo( 200, 300, 3 ) ).save( out_path( 'jpeg_422.jpg' ), quality = 80, subsampling = 1 )
    pil( photo( 500, 375, 4 ) ).save( out_path( 'jpeg_progressive.jpg' ), quality = 75, progressive = True )
    pil( photo( 256, 256, 5 ) ).convert( 'L' ).save( out_path( 'jpeg_gray.jpg' ), quality = 85 )
    pil( photo( 1600, 1200, 6 ) ).save( out_path( 'jpeg_large.jpg' ), quality = 70 )
    pil( photo( 1, 1, 7 ) ).save( out_path( 'jpeg_1x1.jpg' ) )
    pil( photo( 40, 900, 8 ) ).save( out_path( 'jpeg_tall.jpg' ), quality = 80 )
    pil( photo( 1500, 60, 9 ) ).save( out_path( 'jpeg_wide.jpg' ), quality = 80 )
    pil( photo( 17, 13, 10 ) ).save( out_path( 'jpeg_tiny.jpg' ), quality = 90 )
    pil( np.full( ( 120, 160, 3 ), ( 90, 140, 200 ), np.uint8 ) ).save( out_path( 'jpeg_flat.jpg' ), quality = 90 )
    half = photo( 150, 100, 11 )
    pil( np.concatenate( [ half, half[ :, : : -1 ] ], axis = 1 ) ).save( out_path( 'jpeg_mirror.jpg' ), quality = 90 )
    for orientation in range( 2, 9 ):
        exif = Image.Exif()
        exif[ 274 ] = orientation
        pil( photo( 120, 80, 20 + orientation ) ).save( out_path( f'jpeg_orient{orientation}.jpg' ), quality = 85, exif = exif )
    exif = Image.Exif()
    exif[ 0x0131 ] = 'Oracle Paint 1.0'
    exif[ 0x010F ] = 'Camera Maker'
    pil( photo( 160, 120, 30 ) ).save( out_path( 'jpeg_exif_software.jpg' ), quality = 85, exif = exif )
    pil( photo( 320, 240, 31 ) ).save( out_path( 'jpeg_icc_p3.jpg' ), quality = 90, icc_profile = p3_icc() )
    pil( photo( 180, 120, 32 ) ).save( out_path( 'jpeg_icc_srgb.jpg' ), quality = 90, icc_profile = srgb_icc() )
    pil( photo( 120, 90, 33 ) ).convert( 'CMYK' ).save( out_path( 'jpeg_cmyk.jpg' ), quality = 90 )
    pil( photo( 140, 100, 34 ) ).save( out_path( 'jpeg_comment.jpg' ), quality = 85, comment = b'a jpeg comment' )


def gen_png():
    pil( photo( 300, 200, 40 ) ).save( out_path( 'png_rgb.png' ) )
    pil( rgba( 256, 192, 41 ) ).save( out_path( 'png_rgba.png' ) )
    a = rgba( 200, 150, 42 )
    a[ :, :, 3 ] = 255
    pil( a ).save( out_path( 'png_rgba_opaque.png' ) )
    a = rgba( 200, 150, 43 )
    a[ :, :, 3 ] = 255
    a[ : 3, : 10, 3 ] = 254
    pil( a ).save( out_path( 'png_rgba_nearly_opaque.png' ) )
    a = rgba( 100, 100, 44 )
    a[ :, :, 3 ] = 0
    pil( a ).save( out_path( 'png_rgba_clear.png' ) )
    la = np.dstack( [ photo( 180, 120, 45 )[ :, :, 0 ], alpha_circle( 180, 120, 45 ) ] )
    Image.fromarray( la ).save( out_path( 'png_la.png' ) )
    pil( photo( 190, 110, 46 )[ :, :, 1 ] ).save( out_path( 'png_l.png' ) )
    pil( photo( 210, 160, 47 ) ).quantize( 16 ).save( out_path( 'png_p.png' ) )
    p = pil( photo( 150, 150, 48 ) ).quantize( 8 )
    p.save( out_path( 'png_p_trns.png' ), transparency = 3 )
    pil( photo( 130, 90, 49 ) ).convert( '1' ).save( out_path( 'png_1bit.png' ) )
    import cv2
    rgb16 = ( photo( 120, 80, 50 ).astype( np.uint16 ) * 257 + np.random.default_rng( 50 ).integers( 0, 257, ( 80, 120, 3 ) ) ).astype( np.uint16 )
    cv2.imwrite( out_path( 'png_rgb16.png' ), rgb16[ :, :, : : -1 ] )
    rgba16 = np.dstack( [ rgb16, ( alpha_circle( 120, 80, 51 ).astype( np.uint16 ) * 257 ) ] )
    cv2.imwrite( out_path( 'png_rgba16.png' ), rgba16[ :, :, [ 2, 1, 0, 3 ] ] )
    gray16 = ( photo( 100, 70, 52 )[ :, :, 0 ].astype( np.uint16 ) * 257 ).astype( np.uint16 )
    Image.fromarray( gray16 ).save( out_path( 'png_gray16.png' ) )
    pil( photo( 240, 160, 53 ) ).save( out_path( 'png_icc_p3.png' ), icc_profile = p3_icc() )
    info = PngImagePlugin.PngInfo()
    info.add( b'gAMA', struct.pack( '>I', 45455 ) )
    info.add( b'cHRM', struct.pack( '>8I', 31270, 32900, 68000, 32000, 26500, 69000, 15000, 6000 ) )
    pil( photo( 160, 120, 54 ) ).save( out_path( 'png_gama_chrm.png' ), pnginfo = info )
    info = PngImagePlugin.PngInfo()
    info.add( b'cHRM', struct.pack( '>8I', 31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000 ) )
    pil( rgba( 120, 100, 55 ) ).save( out_path( 'png_chrm_rgba.png' ), pnginfo = info )
    info = PngImagePlugin.PngInfo()
    info.add( b'sRGB', b'\x00' )
    info.add( b'gAMA', struct.pack( '>I', 45455 ) )
    info.add( b'cHRM', struct.pack( '>8I', 31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000 ) )
    pil( photo( 150, 100, 56 ) ).save( out_path( 'png_srgb_chunk.png' ), pnginfo = info )
    info = PngImagePlugin.PngInfo()
    info.add_text( 'Comment', 'a human readable comment' )
    info.add_text( 'Title', 'Oracle' )
    pil( photo( 140, 100, 57 ) ).save( out_path( 'png_text.png' ), pnginfo = info )
    info = PngImagePlugin.PngInfo()
    info.add_text( 'Software', 'Oracle Paint 1.0' )
    pil( photo( 140, 100, 58 ) ).save( out_path( 'png_software.png' ), pnginfo = info )
    info = PngImagePlugin.PngInfo()
    info.add_text( 'parameters', 'masterpiece, best quality\nSteps: 20, Sampler: Euler a', zip = True )
    pil( photo( 128, 128, 59 ) ).save( out_path( 'png_ztxt.png' ), pnginfo = info )
    write_bytes( 'png_interlaced.png', interlaced_png( photo( 97, 61, 60 ) ) )
    pil( photo( 1, 1, 61 ) ).save( out_path( 'png_1x1.png' ) )
    pil( photo( 2000, 8, 62 ) ).save( out_path( 'png_wide.png' ) )
    pil( photo( 6, 700, 63 ) ).save( out_path( 'png_tall.png' ) )
    exif = Image.Exif()
    exif[ 274 ] = 6
    pil( photo( 120, 80, 64 ) ).save( out_path( 'png_exif.png' ), exif = exif )
    pil( np.full( ( 90, 90, 3 ), 200, np.uint8 ) ).save( out_path( 'png_flat.png' ) )
    # text chunk after IDAT, only visible after a full load
    data = png_bytes( pil( photo( 64, 64, 65 ) ) )
    chunks = png_chunks( data )
    chunks.insert( -1, ( b'tEXt', b'Description\x00late text chunk' ) )
    write_bytes( 'png_late_text.png', png_from_chunks( chunks ) )


def gen_apng():
    frames = [ pil( photo( 160, 120, 70 + i ) ) for i in range( 3 ) ]
    frames[ 0 ].save( out_path( 'apng_3frames.png' ), save_all = True, append_images = frames[ 1 : ], duration = [ 100, 200, 50 ], loop = 0 )
    frames = [ pil( rgba( 100, 100, 80 + i ) ) for i in range( 4 ) ]
    frames[ 0 ].save( out_path( 'apng_rgba.png' ), save_all = True, append_images = frames[ 1 : ], duration = 40, loop = 2 )
    # an apng whose acTL says one frame: a static png
    data = png_bytes( pil( photo( 90, 70, 85 ) ) )
    chunks = png_chunks( data )
    fctl = struct.pack( '>IIIIIHHBB', 0, 90, 70, 0, 0, 1, 10, 0, 0 )
    chunks = chunks[ : 1 ] + [ ( b'acTL', struct.pack( '>II', 1, 0 ) ), ( b'fcTL', fctl ) ] + chunks[ 1 : ]
    write_bytes( 'apng_single_frame.png', png_from_chunks( chunks ) )
    # a real apng whose acTL sits beyond the first 256 bytes: the reference calls it a png
    frames = [ pil( photo( 80, 60, 86 + i ) ) for i in range( 2 ) ]
    buf = io.BytesIO()
    frames[ 0 ].save( buf, 'PNG', save_all = True, append_images = frames[ 1 : ], duration = 100 )
    chunks = png_chunks( buf.getvalue() )
    chunks = chunks[ : 1 ] + [ ( b'tEXt', b'Comment\x00' + b'x' * 300 ) ] + chunks[ 1 : ]
    write_bytes( 'apng_late_actl.png', png_from_chunks( chunks ) )
    # zero delay denominator (0.1s fallback) and tiny delays
    frames = [ pil( photo( 64, 64, 88 + i ) ) for i in range( 3 ) ]
    buf = io.BytesIO()
    frames[ 0 ].save( buf, 'PNG', save_all = True, append_images = frames[ 1 : ], duration = 100 )
    chunks = png_chunks( buf.getvalue() )
    new_chunks = []
    n = 0
    for ( t, d ) in chunks:
        if t == b'fcTL':
            d = bytearray( d )
            if n == 0:
                d[ 20 : 24 ] = struct.pack( '>HH', 7, 0 )
            elif n == 1:
                d[ 20 : 24 ] = struct.pack( '>HH', 0, 1000 )
            n += 1
            d = bytes( d )
        new_chunks.append( ( t, d ) )
    write_bytes( 'apng_odd_delays.png', png_from_chunks( new_chunks ) )


def gen_gif():
    pil( photo( 200, 150, 90 ) ).quantize( 64 ).save( out_path( 'gif_static.gif' ) )
    p = pil( photo( 120, 100, 91 ) ).quantize( 32 )
    p.save( out_path( 'gif_static_transparent.gif' ), transparency = 5 )
    frames = [ pil( photo( 100, 80, 92 + i ) ).quantize( 32 ) for i in range( 4 ) ]
    frames[ 0 ].save( out_path( 'gif_anim.gif' ), save_all = True, append_images = frames[ 1 : ], duration = [ 100, 0, 10, 250 ], loop = 0, optimize = False, disposal = 1 )
    frames = [ pil( rgba( 90, 90, 96 + i, hard = True ) ).convert( 'RGB' ).quantize( 16 ) for i in range( 3 ) ]
    frames[ 0 ].save( out_path( 'gif_anim_transparent.gif' ), save_all = True, append_images = frames[ 1 : ], duration = 70, transparency = 0, disposal = 2, optimize = False )
    frames = [ pil( photo( 60, 60, 99 + i ) ).quantize( 16 ) for i in range( 3 ) ]
    frames[ 0 ].save( out_path( 'gif_anim_noduration.gif' ), save_all = True, append_images = frames[ 1 : ], optimize = False )
    grey = pil( photo( 80, 64, 103 )[ :, :, 0 ] )
    grey.save( out_path( 'gif_grey.gif' ) )
    # a single frame smaller than the logical screen
    small = pil( photo( 40, 30, 104 ) ).quantize( 16 )
    buf = io.BytesIO()
    small.save( buf, 'GIF' )
    data = bytearray( buf.getvalue() )
    data[ 6 : 10 ] = struct.pack( '<HH', 64, 48 )
    i = data.index( b',', 13 + 3 * 16 )
    data[ i + 1 : i + 5 ] = struct.pack( '<HH', 10, 7 )
    write_bytes( 'gif_small_frame.gif', bytes( data ) )


def gen_webp():
    pil( photo( 300, 200, 110 ) ).save( out_path( 'webp_lossy.webp' ), quality = 80 )
    pil( photo( 200, 150, 111 ) ).save( out_path( 'webp_lossless.webp' ), lossless = True )
    pil( rgba( 180, 140, 112 ) ).save( out_path( 'webp_alpha.webp' ), quality = 80 )
    pil( rgba( 128, 96, 113 ) ).save( out_path( 'webp_alpha_lossless.webp' ), lossless = True )
    frames = [ pil( photo( 120, 90, 114 + i ) ) for i in range( 3 ) ]
    frames[ 0 ].save( out_path( 'webp_anim.webp' ), save_all = True, append_images = frames[ 1 : ], duration = [ 80, 120, 0 ], loop = 3, quality = 70 )
    frames = [ pil( rgba( 100, 100, 118 + i ) ) for i in range( 2 ) ]
    frames[ 0 ].save( out_path( 'webp_anim_alpha.webp' ), save_all = True, append_images = frames[ 1 : ], duration = 100, lossless = True )
    exif = Image.Exif()
    exif[ 274 ] = 6
    pil( photo( 160, 100, 120 ) ).save( out_path( 'webp_exif_orient6.webp' ), quality = 80, exif = exif )


def gen_misc_images():
    pil( photo( 150, 100, 130 ) ).save( out_path( 'bmp_24.bmp' ) )
    pil( photo( 97, 63, 131 ) ).quantize( 64 ).save( out_path( 'bmp_8bit.bmp' ) )
    pil( photo( 64, 48, 132 ) ).convert( '1' ).save( out_path( 'bmp_1bit.bmp' ) )
    pil( rgba( 80, 60, 133 ) ).save( out_path( 'bmp_32_rgba.bmp' ) )
    pil( photo( 70, 50, 134 )[ :, :, 0 ] ).save( out_path( 'bmp_gray.bmp' ) )
    pil( rgba( 64, 64, 135 ) ).save( out_path( 'ico_png.ico' ), sizes = [ ( 16, 16 ), ( 32, 32 ), ( 64, 64 ) ] )
    pil( rgba( 48, 48, 136 ) ).save( out_path( 'ico_bmp.ico' ), sizes = [ ( 16, 16 ), ( 48, 48 ) ], bitmap_format = 'bmp' )
    pil( photo( 160, 120, 137 ) ).save( out_path( 'tiff_raw.tiff' ) )
    pil( rgba( 120, 90, 138 ) ).save( out_path( 'tiff_lzw_rgba.tiff' ), compression = 'tiff_lzw' )
    pil( photo( 100, 80, 139 )[ :, :, 2 ] ).save( out_path( 'tiff_gray_deflate.tiff' ), compression = 'tiff_adobe_deflate' )
    exif = Image.Exif()
    exif[ 274 ] = 6
    pil( photo( 90, 60, 140 ) ).save( out_path( 'tiff_orient6.tiff' ), exif = exif )
    pil( photo( 80, 60, 141 ) ).quantize( 16 ).save( out_path( 'tiff_palette.tiff' ) )
    pil( photo( 150, 90, 142 ) ).save( out_path( 'qoi_rgb.qoi' ) )
    pil( rgba( 100, 80, 143 ) ).save( out_path( 'qoi_rgba.qoi' ) )
    pil( photo( 160, 120, 144 ) ).save( out_path( 'avif_still.avif' ), quality = 60 )
    pil( rgba( 96, 80, 145 ) ).save( out_path( 'avif_alpha.avif' ), quality = 60 )
    frames = [ pil( photo( 64, 48, 146 + i ) ) for i in range( 4 ) ]
    buf = io.BytesIO()
    frames[ 0 ].save( buf, 'AVIF', save_all = True, append_images = frames[ 1 : ], duration = 100, quality = 50 )
    write_bytes( 'avif_sequence.avifs', zero_isobmff_times( buf.getvalue() ) )
    import pillow_heif
    pillow_heif.register_heif_opener()
    pil( photo( 160, 120, 150 ) ).save( out_path( 'heic_still.heic' ), quality = 60 )
    pil( rgba( 96, 64, 151 ) ).save( out_path( 'heic_alpha.heic' ), quality = 60 )
    ffmpeg( 'jxl_still.jxl', '-f', 'lavfi', '-i', 'testsrc2=s=64x48', '-frames:v', '1', '-c:v', 'libjxl' )


def gen_video():
    v = [ '-f', 'lavfi', '-i', 'testsrc2=s=64x48:r=10:d=1' ]
    a = [ '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=22050:d=1' ]
    silent = [ '-f', 'lavfi', '-i', 'anullsrc=r=22050:cl=mono', '-t', '1' ]
    x264 = [ '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p' ]
    ffmpeg( 'mp4_h264_aac.mp4', *v, *a, *x264, '-c:a', 'aac', '-b:a', '32k', '-shortest' )
    ffmpeg( 'mp4_h264.mp4', *v, *x264 )
    ffmpeg( 'mp4_silent_audio.mp4', *v, *silent, *x264, '-c:a', 'aac', '-b:a', '16k', '-shortest' )
    ffmpeg( 'mp4_rotated.mp4', '-display_rotation', '90', '-f', 'lavfi', '-i', 'testsrc2=s=64x48:r=10:d=1', *x264 )
    ffmpeg( 'mp4_sar.mp4', *v, *x264, '-vf', 'setsar=2/1' )
    ffmpeg( 'mp4_25fps_odd.mp4', '-f', 'lavfi', '-i', 'testsrc2=s=48x32:r=25:d=0.52', *x264 )
    ffmpeg( 'm4v_h264.m4v', *v, *x264, '-f', 'mp4', '-brand', 'M4V ' )
    ffmpeg( 'webm_vp9_opus.webm', *v, '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000:d=1', '-c:v', 'libvpx-vp9', '-b:v', '50k', '-deadline', 'realtime', '-c:a', 'libopus', '-b:a', '16k', '-shortest' )
    ffmpeg( 'webm_vp8.webm', *v, '-c:v', 'libvpx', '-b:v', '50k', '-deadline', 'realtime' )
    ffmpeg( 'mkv_h264.mkv', *v, *x264 )
    ffmpeg( 'mkv_vp9_aac.mkv', *v, *a, '-c:v', 'libvpx-vp9', '-b:v', '50k', '-deadline', 'realtime', '-c:a', 'aac', '-b:a', '32k', '-shortest' )
    ffmpeg( 'avi_mpeg4.avi', *v, '-c:v', 'mpeg4', '-q:v', '10' )
    ffmpeg( 'flv_h264.flv', *v, *x264 )
    ffmpeg( 'mov_h264.mov', *v, *x264 )
    ffmpeg( 'mpeg_mpeg1.mpg', '-f', 'lavfi', '-i', 'testsrc2=s=64x48:r=25:d=1', '-c:v', 'mpeg1video', '-q:v', '10' )
    ffmpeg( 'ts_mpeg2.ts', '-f', 'lavfi', '-i', 'testsrc2=s=64x48:r=25:d=1', '-c:v', 'mpeg2video', '-q:v', '10' )
    ffmpeg( 'ogv_theora.ogv', *v, '-c:v', 'libtheora', '-q:v', '3' )
    ffmpeg( 'wmv_wmv2.wmv', *v, *a, '-c:v', 'wmv2', '-q:v', '10', '-c:a', 'wmav2', '-b:a', '32k', '-shortest' )
    ffmpeg( 'rm_rv10.rm', '-f', 'lavfi', '-i', 'testsrc2=s=64x48:r=10:d=1', '-c:v', 'rv10', '-q:v', '10' )


def gen_audio():
    a = [ '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=22050:d=1.3' ]
    ffmpeg( 'mp3.mp3', *a, '-c:a', 'libmp3lame', '-b:a', '32k' )
    cover = out_path( '_cover_tmp.png' )
    pil( photo( 60, 60, 160 ) ).save( cover )
    ffmpeg_raw( 'mp3_cover.mp3', *a, '-i', cover, '-map', '0:a', '-map', '1:v', '-c:a', 'libmp3lame', '-b:a', '32k', '-c:v', 'png', '-disposition:v', 'attached_pic', '-map_metadata', '-1' )
    ffmpeg_raw( 'flac_cover.flac', *a, '-i', cover, '-map', '0:a', '-map', '1:v', '-c:a', 'flac', '-c:v', 'png', '-disposition:v', 'attached_pic', '-map_metadata', '-1' )
    os.remove( cover )
    ffmpeg( 'ogg_vorbis.ogg', *a, '-c:a', 'libvorbis', '-q:a', '0' )
    ffmpeg( 'ogg_opus.ogg', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000:d=1.3', '-c:a', 'libopus', '-b:a', '16k' )
    ffmpeg( 'flac.flac', *a, '-c:a', 'flac' )
    ffmpeg( 'wav.wav', *a, '-c:a', 'pcm_s16le' )
    ffmpeg( 'm4a_aac.m4a', *a, '-c:a', 'aac', '-b:a', '32k', '-f', 'ipod' )
    ffmpeg( 'mp4_audio.mp4', *a, '-c:a', 'aac', '-b:a', '32k', '-f', 'mp4' )
    ffmpeg( 'wma.wma', *a, '-c:a', 'wmav2', '-b:a', '32k' )
    ffmpeg( 'wavpack.wv', *a, '-c:a', 'wavpack' )
    ffmpeg( 'tta.tta', *a, '-c:a', 'tta' )
    ffmpeg( 'mka_flac.mka', *a, '-c:a', 'flac' )
    ffmpeg( 'mka_vorbis.mka', *a, '-c:a', 'libvorbis', '-q:a', '0', '-f', 'matroska' )
    ffmpeg( 'ra_ac3.ra', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=44100:d=1.3', '-c:a', 'ac3', '-b:a', '64k', '-f', 'rm' )


def gen_archives():
    jpg = lambda w, h, s: jpeg_bytes( pil( photo( w, h, s ) ), quality = 80 )
    pngb = lambda w, h, s: png_bytes( pil( photo( w, h, s ) ) )
    write_bytes( 'zip_plain.zip', zip_bytes( [ ( 'readme.txt', b'hello' ), ( 'program.exe', b'MZ' + b'\x00' * 100 ) ] ) )
    write_bytes( 'zip_one_image.zip', zip_bytes( [ ( 'image001.jpg', jpg( 50, 40, 170 ) ) ] ) )
    write_bytes( 'cbz_flat.cbz', zip_bytes( [ ( f'page_{i:03}.jpg', jpg( 90, 120, 171 + i ) ) for i in range( 1, 6 ) ] + [ ( 'ComicInfo.xml', b'<ComicInfo/>' ) ] ) )
    write_bytes( 'cbz_folder.zip', zip_bytes( [ ( f'Chapter 1/p{i}.png', pngb( 70, 100, 180 + i ) ) for i in ( 10, 9, 11, 2, 1 ) ] + [ ( '__MACOSX/Chapter 1/._p1.png', b'junk' ) ] ) )
    write_bytes( 'zip_mixed_images.zip', zip_bytes( [ ( 'cat.jpg', jpg( 40, 40, 190 ) ), ( 'dog photo.jpg', jpg( 40, 40, 191 ) ), ( 'img_2.png', pngb( 30, 30, 192 ) ), ( 'screenshot-2020.png', pngb( 30, 30, 193 ) ), ( 'x.jpg', jpg( 20, 20, 194 ) ) ] ) )
    frames = [ ( f'{i:06}.jpg', jpg( 80, 60, 200 + i ) ) for i in range( 5 ) ]
    write_bytes( 'ugoira_plain.zip', zip_bytes( frames ) )
    anim = json.dumps( { 'frames' : [ { 'file' : name, 'delay' : 60 + 10 * i } for ( i, ( name, _ ) ) in enumerate( frames ) ] } ).encode()
    write_bytes( 'ugoira_json.zip', zip_bytes( frames + [ ( 'animation.json', anim ) ] ) )
    anim_list = json.dumps( [ { 'file' : name, 'delay' : 40 } for ( name, _ ) in frames[ : 3 ] ] ).encode()
    write_bytes( 'ugoira_list_json.zip', zip_bytes( frames[ : 3 ] + [ ( 'animation.json', anim_list ) ] ) )
    exif = Image.Exif()
    exif[ 274 ] = 6
    rotated = [ ( f'{i:06}.jpg', jpeg_bytes( pil( photo( 80, 60, 210 + i ) ), exif = exif ) ) for i in range( 3 ) ]
    write_bytes( 'ugoira_rotated.zip', zip_bytes( rotated ) )
    write_bytes( 'zip_encrypted.zip', set_zip_encrypted_flag( zip_bytes( [ ( 'secret.txt', b'secret' * 10 ) ] ) ) )
    cover = jpg( 120, 180, 220 )
    write_bytes( 'epub3.epub', epub( 'epub3', cover ) )
    write_bytes( 'epub2.epub', epub( 'epub2', jpg( 100, 150, 221 ) ) )
    write_bytes( 'epub_nocover.epub', epub( 'none', None ) )
    merged = png_bytes( pil( rgba( 320, 240, 230 ) ) )
    maindoc = b'<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE DOC PUBLIC \'-//KDE//DTD krita 2.0//EN\' \'http://www.calligra.org/DTD/krita-2.0.dtd\'><DOC xmlns="http://www.calligra.org/DTD/krita" syntaxVersion="2" editor="Krita"><IMAGE width="320" height="240" mime="application/x-kra" name="Unnamed"/></DOC>'
    write_bytes( 'krita.kra', zip_bytes( [ ( 'mimetype', b'application/x-krita' ), ( 'maindoc.xml', maindoc ), ( 'mergedimage.png', merged ), ( 'preview.png', png_bytes( pil( rgba( 64, 48, 230 ) ) ) ) ], first_stored = True ) )
    write_bytes( 'krita_mimetype_late.kra', zip_bytes( [ ( 'maindoc.xml', maindoc ), ( 'mimetype', b'application/x-krita' ), ( 'mergedimage.png', merged ) ] ) )
    stack = b'<?xml version="1.0" encoding="UTF-8"?><image version="0.0.5" w="300" h="200"><stack><layer name="bg" src="data/bg.png"/></stack></image>'
    ora_merged = png_bytes( pil( photo( 300, 200, 231 ) ) )
    write_bytes( 'openraster.ora', zip_bytes( [ ( 'mimetype', b'image/openraster' ), ( 'stack.xml', stack ), ( 'data/bg.png', ora_merged ), ( 'mergedimage.png', ora_merged ), ( 'Thumbnails/thumbnail.png', png_bytes( pil( photo( 64, 43, 231 ) ) ) ) ], first_stored = True ) )
    thumb = png_bytes( pil( photo( 60, 80, 232 ) ) )
    write_bytes( 'procreate.procreate', zip_bytes( [ ( 'Document.archive', procreate_archive( '{2894, 4093}', 1 ) ), ( 'QuickLook/Thumbnail.png', thumb ) ] ) )
    write_bytes( 'procreate_rotated.procreate', zip_bytes( [ ( 'Document.archive', procreate_archive( '{1000, 600}', 3 ) ), ( 'QuickLook/Thumbnail.png', thumb ) ] ) )
    write_bytes( 'procreate_notsilica.zip', zip_bytes( [ ( 'Document.archive', procreate_archive( '{10, 10}', 1, 'Other' ) ), ( 'QuickLook/Thumbnail.png', thumb ) ] ) )
    write_bytes( 'docx.docx', office( 'docx', words = 1234 ) )
    write_bytes( 'docx_default.docx', office( 'docx', words = 7, default_style = True ) )
    write_bytes( 'docx_nowords.docx', office( 'docx' ) )
    write_bytes( 'xlsx.xlsx', office( 'xlsx', words = 3 ) )
    write_bytes( 'pptx.pptx', office( 'pptx', words = 42, thumbnail = jpg( 256, 144, 233 ) ) )
    write_bytes( 'pptx_nothumb.pptx', office( 'pptx', words = 5 ) )
    write_bytes( 'gzip.gz', gzip.compress( b'hello gzip ' * 50, mtime = 0 ) )
    write_bytes( 'sevenzip.7z', b'7z\xBC\xAF\x27\x1C\x00\x04' + bytes( range( 256 ) ) )
    write_bytes( 'rar4.rar', b'Rar!\x1A\x07\x00' + bytes( range( 200 ) ) )
    write_bytes( 'rar5.rar', b'Rar!\x1A\x07\x01\x00' + bytes( range( 200 ) ) )


def gen_documents():
    write_bytes( 'ole_doc.doc', ole( root_clsid = '00020906-0000-0000-C000-000000000046', app_name = 'Microsoft Office Word', num_words = 321 ) )
    write_bytes( 'ole_xls_by_app.xls', ole( app_name = 'Microsoft Excel', num_words = 0 ) )
    write_bytes( 'ole_ppt.ppt', ole( root_clsid = '64818D10-4F9B-11CF-86EA-00AA00B929E8', num_words = 12 ) )
    write_bytes( 'ole_word_by_app.doc', ole( app_name = 'Microsoft Word 8.0', num_words = 99 ) )
    write_bytes( 'ole_unknown.bin', ole( app_name = 'Something Else' ) )
    write_bytes( 'doc_legacy.doc', b'\xDB\xA5\x2D\x00' + bytes( range( 256 ) ) * 4 )
    write_bytes( 'ppt_legacy.ppt', b'\xED\xDE\xAD\x0B' + bytes( range( 256 ) ) * 4 )
    buf = io.BytesIO()
    fixed_time = time.gmtime( 1577836800 )
    pil( photo( 200, 150, 240 ) ).save( buf, 'PDF', resolution = 72, creationDate = fixed_time, modDate = fixed_time )
    write_bytes( 'pdf_image.pdf', buf.getvalue() )
    write_bytes( 'pdf_text.pdf', text_pdf( 'Hello there, this is a small test document!' ) )
    write_bytes( 'pdf_multipage.pdf', multi_page_pdf() )
    write_bytes( 'pdf_keywords_only.pdf', blank_pdf( info = [ b'<< /Keywords ', ( 'str', b'tag, another' ), b' >>' ] ) )
    write_bytes( 'pdf_empty_title.pdf', blank_pdf( info = b'<< /Title () /Creator (a program) >>', media_box = b'[0 0 300.25 200.75]', rotate = b' /Rotate -90' ) )
    write_bytes( 'pdf_encrypted_owner.pdf', blank_pdf( info = [ b'<< /Author ', ( 'str', b'someone' ), b' >>' ], user_password = b'' ) )
    write_bytes( 'pdf_encrypted_user.pdf', blank_pdf( info = [ b'<< /Author ', ( 'str', b'someone' ), b' >>' ], user_password = b'secret' ) )
    write_bytes( 'rtf.rtf', b'{\\rtf1\\ansi\\deff0 {\\fonttbl {\\f0 Times;}} Hello RTF}' )
    write_bytes( 'djvu.djvu', b'AT&TFORM\x00\x00\x00\x40DJVUINFO\x00\x00\x00\x0a' + bytes( range( 80 ) ) )
    write_bytes( 'exe.exe', b'MZ\x90\x00\x03\x00\x00\x00\x04\x00' + bytes( range( 200 ) ) )
    write_bytes( 'svg_sized.svg', b'<?xml version="1.0"?>\n<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120"><rect x="10" y="10" width="100" height="50" fill="red"/></svg>' )
    write_bytes( 'svg_viewbox.svg', b'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64.5 32.2"><circle cx="20" cy="16" r="10" fill="blue"/></svg>' )
    write_bytes( 'html.html', b'<!DOCTYPE html><html><head><title>x</title></head><body>hello</body></html>' )
    write_bytes( 'json.json', b'{"hello": [1, 2, 3], "world": null}' )
    write_bytes( 'json_as_data.dat', b'[1, 2, {"a": "b"}]' )
    write_bytes( 'text.txt', b'just some plain text\n' * 5 )
    write_bytes( 'random.bin', bytes( np.random.default_rng( 250 ).integers( 0, 256, 3000, dtype = np.uint8 ) ) )
    write_bytes( 'hydrus_encrypted.zip.encrypted', b'hydrus encrypted zip' + bytes( range( 100 ) ) )
    write_bytes( 'empty.bin', b'' )


def gen_projects():
    write_bytes( 'flash_fws.swf', swf( 10, 550, 400, 24, 48, 'F' ) )
    write_bytes( 'flash_cws.swf', swf( 10, 320, 240, 30, 15, 'C' ) )
    write_bytes( 'flash_zws.swf', swf( 13, 640, 360, 12, 1, 'Z' ) )
    write_bytes( 'psd_rgb.psd', psd( photo( 120, 90, 260 ) ) )
    write_bytes( 'psd_icc.psd', psd( photo( 100, 70, 261 ), icc = srgb_icc() ) )
    write_bytes( 'psd_rgba.psd', psd( rgba( 80, 60, 262 ) ) )
    preview = png_bytes( pil( photo( 100, 75, 263 ) ) )
    write_bytes( 'clip_pixels.clip', clip( 1200.0, 900.0, 0, 350.0, None, preview ) )
    write_bytes( 'clip_mm_anim.clip', clip( 210.0, 297.0, 2, 350.0, ( 0.0, 24.0, 60.0 ), preview ) )
    write_bytes( 'clip_inches_zero_fps.clip', clip( 5.5, 3.25, 3, 300.0, ( 1.0, 0.0, 12.0 ), preview ) )
    write_bytes( 'paintnet.pdn', pdn( 800, 600, png_bytes( pil( photo( 64, 48, 264 ) ) ) ) )
    write_bytes( 'sai2.sai2', b'SAI-CANVAS-TYPE0' + bytes( range( 200 ) ) )
    write_bytes( 'xcf.xcf', b'gimp xcf v011\x00' + struct.pack( '>III', 640, 480, 0 ) + bytes( range( 100 ) ) )


def generate():
    os.makedirs( MEDIA_DIR, exist_ok = True )
    for name in os.listdir( MEDIA_DIR ):
        os.remove( out_path( name ) )
    for gen in ( gen_jpeg, gen_png, gen_apng, gen_gif, gen_webp, gen_misc_images, gen_video, gen_audio, gen_archives, gen_documents, gen_projects ):
        gen()
    total = sum( os.path.getsize( out_path( n ) ) for n in os.listdir( MEDIA_DIR ) )
    print( f'{len( os.listdir( MEDIA_DIR ) )} files, {total} bytes', file = sys.stderr )


# ---------------------------------------------------------------------------
# recording the reference

def setup_reference():
    """Import the reference with Qt-backed PDF/SVG support, as the running client has."""
    from qtpy import QtWidgets
    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication( [] )
    from hydrus.client import ClientGlobals as CG
    class ControllerShim:
        def CallBlockingToQtTLW( self, func, *args, **kwargs ):
            return func( *args, **kwargs )
    CG.client_controller = ControllerShim()
    from hydrus.core import HydrusGlobals as HG
    import threading
    class CoreControllerShim:
        def CallToThread( self, func, *args, **kwargs ):
            threading.Thread( target = func, args = args, kwargs = kwargs, daemon = True ).start()
        def pub( self, *args, **kwargs ):
            pass
        def GetDBDir( self ):
            # no user overrides of the static dir: the stock icons are used
            return os.path.join( tempfile.gettempdir(), 'hydrus-oracle-no-db' )
        def GetHydrusTempDir( self ):
            return tempfile.gettempdir()
    HG.controller = CoreControllerShim()
    from hydrus.client import ClientPDFHandling, ClientSVGHandling  # noqa: F401 (they patch hydrus.core)
    from hydrus.core.files import HydrusFileHandling
    HydrusFileHandling.InitialiseMimesToDefaultThumbnailPaths()
    return app


def sha256_hex( data ):
    return hashlib.sha256( data ).hexdigest()


def error_name( e ):
    return type( e ).__name__


def record_file( name ):
    from hydrus.core import HydrusConstants as HC
    from hydrus.core.files import HydrusFileHandling, HydrusPSDHandling
    from hydrus.core.files.images import HydrusImageHandling, HydrusBlurhash, HydrusImageOpening, HydrusImageMetadata, HydrusImageColours
    from hydrus.client.files.images import ClientImagePerceptualHashes
    from hydrus.client.files import ClientFiles
    path = out_path( name )
    rec = { 'file' : name }
    try:
        mime = HydrusFileHandling.GetMime( path )
    except Exception as e:
        rec[ 'mime_error' ] = error_name( e )
        return rec
    rec[ 'mime' ] = mime
    with open( path, 'rb' ) as f:
        data = f.read()
    ( md5, sha1, sha512 ) = HydrusFileHandling.GetExtraHashesFromPath( path )
    rec[ 'hashes' ] = { 'sha256' : HydrusFileHandling.GetHashFromPath( path ).hex(), 'md5' : md5.hex(), 'sha1' : sha1.hex(), 'sha512' : sha512.hex() }
    try:
        ( size, mime, width, height, duration_ms, num_frames, has_audio, num_words ) = HydrusFileHandling.GetFileInfo( path, mime = mime )
    except Exception as e:
        rec[ 'info_error' ] = error_name( e )
        return rec
    rec[ 'info' ] = { 'size' : size, 'mime' : mime, 'width' : width, 'height' : height, 'duration_ms' : duration_ms, 'num_frames' : num_frames, 'has_audio' : has_audio, 'num_words' : num_words }
    if mime in HC.MIMES_WITH_THUMBNAILS:
        target = HydrusImageHandling.GetThumbnailResolution( ( width, height ), ( 150, 125 ), HydrusImageHandling.THUMBNAIL_SCALE_DOWN_ONLY, 100 )
        thumb = HydrusFileHandling.GenerateThumbnailNumPy( path, target, mime, duration_ms, num_frames, percentage_in = 35 )
        default = HydrusFileHandling.GenerateDefaultThumbnail( mime, target )
        depth = 3 if len( thumb.shape ) == 2 else thumb.shape[ 2 ]
        t = { 'target' : list( target ), 'width' : thumb.shape[ 1 ], 'height' : thumb.shape[ 0 ], 'channels' : depth, 'format' : 'png' if depth == 4 else 'jpeg', 'pixels_sha256' : sha256_hex( thumb.tobytes() ), 'is_default' : bool( thumb.shape == default.shape and ( thumb == default ).all() ) }
        try:
            t[ 'blurhash' ] = HydrusBlurhash.GetBlurhashFromNumPy( thumb )
        except Exception as e:
            t[ 'blurhash' ] = None
        rec[ 'thumbnail' ] = t
    if mime in HC.FILES_THAT_HAVE_PERCEPTUAL_HASH:
        rec[ 'perceptual_hashes' ] = sorted( p.hex() for p in ClientImagePerceptualHashes.GenerateShapePerceptualHashes( path, mime ) )
        try:
            numpy_image = HydrusImageHandling.GenerateNumPyImage( path, mime )
            rec[ 'decoded' ] = { 'shape' : list( numpy_image.shape ), 'sha256' : sha256_hex( numpy_image.tobytes() ), 'blurhash' : HydrusBlurhash.GetBlurhashFromNumPy( numpy_image ) }
        except Exception as e:
            rec[ 'decoded' ] = { 'error' : error_name( e ) }
    if mime in HC.FILES_THAT_CAN_HAVE_PIXEL_HASH and duration_ms is None:
        try:
            rec[ 'pixel_hash' ] = HydrusImageHandling.GetImagePixelHash( path, mime ).hex()
        except Exception as e:
            rec[ 'pixel_hash' ] = None
    rec[ 'has_transparency' ] = ClientFiles.HasTransparency( path, mime, duration_ms = duration_ms, num_frames = num_frames, resolution = ( width, height ) )
    has_exif = False
    has_hr = False
    has_icc = False
    raw = None
    if mime in HC.FILES_THAT_CAN_HAVE_EXIF:
        try:
            raw = HydrusImageOpening.RawOpenPILImage( path )
            has_exif = HydrusImageMetadata.HasEXIF( raw )
        except Exception as e:
            pass
    if mime in HC.FILES_THAT_CAN_HAVE_HUMAN_READABLE_EMBEDDED_METADATA:
        try:
            has_hr = ClientFiles.HasHumanReadableEmbeddedMetadata( path, mime, possible_raw_pil_image = raw )
        except Exception as e:
            pass
    if mime in HC.FILES_THAT_CAN_HAVE_ICC_PROFILE:
        try:
            if mime == HC.APPLICATION_PSD:
                has_icc = HydrusPSDHandling.PSDHasICCProfile( path )
            else:
                if raw is None:
                    raw = HydrusImageOpening.RawOpenPILImage( path )
                has_icc = HydrusImageMetadata.HasICCProfile( raw )
        except Exception as e:
            pass
    if raw is not None:
        raw.close()
    rec[ 'has_exif' ] = has_exif
    rec[ 'has_human_readable_embedded_metadata' ] = bool( has_hr )
    rec[ 'has_icc_profile' ] = has_icc
    return rec


def record_thumbnail_resolutions():
    from hydrus.core.files.images import HydrusImageHandling
    rng = np.random.default_rng( 688 )
    cases = []
    dims = [ 1, 2, 3, 7, 16, 99, 100, 124, 125, 126, 149, 150, 151, 199, 200, 300, 400, 640, 1000, 1080, 1920, 4000, 12000, 65535 ]
    for _ in range( 600 ):
        w = int( rng.choice( dims ) ) if rng.random() < 0.5 else int( rng.integers( 1, 5000 ) )
        h = int( rng.choice( dims ) ) if rng.random() < 0.5 else int( rng.integers( 1, 5000 ) )
        bw = int( rng.choice( [ 150, 125, 200, 300, 1, 10, 400 ] ) )
        bh = int( rng.choice( [ 125, 150, 200, 300, 1, 10, 400 ] ) )
        scale = int( rng.integers( 0, 3 ) )
        dpr = int( rng.choice( [ 100, 100, 125, 150, 200, 33 ] ) )
        cases.append( ( w, h, bw, bh, scale, dpr ) )
    for ( w, h ) in [ ( None, None ), ( 0, 0 ), ( 0, 100 ), ( 100, None ) ]:
        for scale in range( 3 ):
            cases.append( ( w, h, 150, 125, scale, 100 ) )
    out = []
    for ( w, h, bw, bh, scale, dpr ) in cases:
        try:
            result = list( HydrusImageHandling.GetThumbnailResolution( ( w, h ), ( bw, bh ), scale, dpr ) )
        except ZeroDivisionError:
            result = None
        out.append( { 'width' : w, 'height' : h, 'bounding' : [ bw, bh ], 'scale_type' : scale, 'dpr_percent' : dpr, 'result' : result } )
    return out


def record_cv_ops():
    """cv2 resizes/greyscale on decoded lossless corpus images, for bit-exact checks of the Rust ports."""
    import cv2
    from hydrus.core.files.images import HydrusImageHandling
    from hydrus.client.files.images import ClientImagePerceptualHashes
    sources = {
        'png_rgb.png' : HydrusImageHandling.GenerateNumPyImage( out_path( 'png_rgb.png' ), 2 ),
        'png_rgba.png' : HydrusImageHandling.GenerateNumPyImage( out_path( 'png_rgba.png' ), 2 ),
        'png_wide.png' : HydrusImageHandling.GenerateNumPyImage( out_path( 'png_wide.png' ), 2 ),
        'png_tall.png' : HydrusImageHandling.GenerateNumPyImage( out_path( 'png_tall.png' ), 2 ),
    }
    sizes = [ ( 150, 100 ), ( 100, 66 ), ( 75, 50 ), ( 60, 40 ), ( 32, 32 ), ( 100, 100 ), ( 400, 300 ), ( 310, 150 ), ( 290, 250 ), ( 1, 1 ), ( 7, 3 ), ( 256, 192 ), ( 128, 96 ), ( 85, 64 ), ( 64, 48 ), ( 17, 700 ), ( 2000, 4 ), ( 1000, 8 ), ( 33, 33 ) ]
    out = []
    for ( name, src ) in sources.items():
        variants = { 'native' : src, 'grey' : cv2.cvtColor( src[ :, :, : 3 ].copy(), cv2.COLOR_RGB2GRAY ) }
        for ( variant, img ) in variants.items():
            out.append( { 'source' : name, 'variant' : variant, 'op' : 'identity', 'shape' : list( img.shape ), 'sha256' : sha256_hex( img.tobytes() ) } )
            for ( w, h ) in sizes:
                for ( interp_name, interp ) in ( ( 'area', cv2.INTER_AREA ), ( 'linear', cv2.INTER_LINEAR ) ):
                    r = cv2.resize( img, ( w, h ), interpolation = interp )
                    out.append( { 'source' : name, 'variant' : variant, 'op' : interp_name, 'size' : [ w, h ], 'sha256' : sha256_hex( r.tobytes() ) } )
            out.append( { 'source' : name, 'variant' : variant, 'op' : 'phash', 'phash' : ClientImagePerceptualHashes.GenerateShapePerceptualHashNumPy( img ).hex() } )
    return out


def record():
    app = setup_reference()
    names = sorted( n for n in os.listdir( MEDIA_DIR ) )
    files = []
    for name in names:
        files.append( record_file( name ) )
    out = {
        'thumbnail_bounding' : [ 150, 125 ],
        'thumbnail_scale_type' : 0,
        'thumbnail_dpr_percent' : 100,
        'video_thumbnail_percentage_in' : 35,
        'files' : files,
        'thumbnail_resolutions' : record_thumbnail_resolutions(),
        'cv_ops' : record_cv_ops(),
    }
    with open( JSON_PATH, 'w' ) as f:
        json.dump( out, f, indent = 1, sort_keys = True )
        f.write( '\n' )
    del app


if __name__ == '__main__':
    phase = sys.argv[ 1 ] if len( sys.argv ) > 1 else 'all'
    if phase in ( 'generate', 'all' ):
        generate()
    if phase in ( 'record', 'all' ):
        record()
