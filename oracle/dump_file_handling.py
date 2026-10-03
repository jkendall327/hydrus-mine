#!/usr/bin/env python3
"""Record what the reference's file handling options change.

    python oracle/dump_file_handling.py   # writes oracle/fixtures/file_handling.json

`file_has_transparency_strictness`: the import job's transparency check
(ClientFiles.HasTransparency) over the media corpus at each strictness
level. `allow_comic_book_archive_detection`: GetMime over the corpus's zips
with comic book detection on and off. Both are module globals in the
reference, set here as the client sets them at boot.
"""
import json
import os
import sys

ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )
sys.path.insert( 0, ROOT )
sys.path.insert( 0, os.path.join( ROOT, 'oracle' ) )

import dump_media

MEDIA_DIR = os.path.join( ROOT, 'oracle', 'fixtures', 'media' )
JSON_PATH = os.path.join( ROOT, 'oracle', 'fixtures', 'file_handling.json' )


def main():
    dump_media.setup_reference()
    from hydrus.core import HydrusConstants as HC
    from hydrus.core.files import HydrusFileHandling
    from hydrus.core.files.images import HydrusImageColours
    from hydrus.client.files import ClientFiles

    transparency = []
    zips = []
    for name in sorted( os.listdir( MEDIA_DIR ) ):
        path = os.path.join( MEDIA_DIR, name )
        try:
            mime = HydrusFileHandling.GetMime( path )
            ( size, mime, width, height, duration_ms, num_frames, has_audio, num_words ) = HydrusFileHandling.GetFileInfo( path, mime = mime )
        except Exception:
            continue
        if mime in ( HC.APPLICATION_ZIP, HC.APPLICATION_CBZ ):
            HydrusFileHandling.ALLOW_COMIC_BOOK_ARCHIVE_INSPECTION = False
            without = HydrusFileHandling.GetMime( path )
            HydrusFileHandling.ALLOW_COMIC_BOOK_ARCHIVE_INSPECTION = True
            zips.append( { 'file' : 'media/' + name, 'mime' : mime, 'mime_without_detection' : without } )
        levels = []
        for level in ( 0, 1, 2 ):
            HydrusImageColours.CURRENT_HAS_TRANSPARENCY_STRICTNESS_LEVEL = level
            try:
                has = ClientFiles.HasTransparency( path, mime, duration_ms = duration_ms, num_frames = num_frames, resolution = ( width, height ) )
            except Exception:
                has = False
            levels.append( bool( has ) )
        HydrusImageColours.CURRENT_HAS_TRANSPARENCY_STRICTNESS_LEVEL = HydrusImageColours.HAS_TRANSPARENCY_STRICTNESS_HUMAN
        # (only files the levels can tell apart are worth keeping)
        if any( levels ):
            transparency.append( { 'file' : 'media/' + name, 'mime' : mime, 'has_transparency_by_strictness' : levels } )
    with open( JSON_PATH, 'w' ) as f:
        json.dump( { 'transparency' : transparency, 'zips' : zips }, f, indent = 1, sort_keys = True )
        f.write( '\n' )


if __name__ == '__main__':
    main()
