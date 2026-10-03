#!/usr/bin/env python3
"""Write hydrus-media's table of Pillow's EXIF tags
(`crates/hydrus-media/src/imaging/exif_tags.rs`): the names the reference's
embedded metadata window labels EXIF rows with (`ExifTags.TAGS`, then
`ExifTags.GPSTAGS`), and the tags Pillow reads as a single value (a
`TiffTags` length of 1), for the main IFD and the Exif, GPS and interop
groups.

Usage: python oracle/gen_exif_tags.py
"""

import os

import PIL
from PIL import ExifTags, TiffTags

ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )
OUT = os.path.join( ROOT, 'crates', 'hydrus-media', 'src', 'imaging', 'exif_tags.rs' )


def names( d ):

    return '\n'.join( f'    ({k}, "{v}"),' for ( k, v ) in sorted( d.items() ) )


def singles( d ):

    return ', '.join( str( k ) for ( k, info ) in sorted( d.items() ) if info.length == 1 )


def main():

    text = f'''//! Pillow's EXIF tags: the names the embedded metadata window labels rows
//! with (`ExifTags.TAGS`, then `ExifTags.GPSTAGS`), and the tags Pillow
//! reads as one value (`TiffTags` length 1, by group). Written by
//! `oracle/gen_exif_tags.py` from Pillow {PIL.__version__}; don't edit.

pub(crate) const TAGS: &[(u16, &str)] = &[
{names( ExifTags.TAGS )}
];

pub(crate) const GPS_TAGS: &[(u16, &str)] = &[
{names( ExifTags.GPSTAGS )}
];

/// The main IFD's tags read as one value.
pub(crate) const SINGLE: &[u16] = &[{singles( TiffTags.TAGS_V2 )}];

/// The Exif IFD's (34665).
pub(crate) const SINGLE_EXIF: &[u16] = &[{singles( TiffTags.TAGS_V2_GROUPS[ 34665 ] )}];

/// The GPS IFD's (34853).
pub(crate) const SINGLE_GPS: &[u16] = &[{singles( TiffTags.TAGS_V2_GROUPS[ 34853 ] )}];
'''

    with open( OUT, 'w' ) as f:

        f.write( text )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
