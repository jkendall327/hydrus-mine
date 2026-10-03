#!/usr/bin/env python3
"""Record the reference's thumbnail page status bar text.

`MediaResultsPanel._GetPrettyStatusForStatusBar` (with
`GetMediasFiletypeSummaryString`, `_GetPrettyTotalSize`,
`_GetPrettyTotalDuration`) is run on a stand-in page holding the `basic`
fixture's files, at a fixed "now": pages of all the files, of the images,
of the largest group of one type, of the animations, of the videos, of
one file, and empty (with and without the "no files found for this
search" a search page gives it); each with nothing selected, several
selections, and on the page of all files each file selected alone (whose
status shows its info lines). Then once more with
`show_extended_single_file_info_in_status_bar` off.

Usage: python oracle/record_status_bar.py      (writes fixtures/status_bar.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'status_bar.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusTime
    from hydrus.client.gui.pages import ClientGUIMediaResultsPanel
    from hydrus.client.media import ClientMediaSingle

    controller = session.controller
    options = controller.new_options

    base = ClientGUIMediaResultsPanel.MediaResultsPanel

    class Page( object ):

        _GetPrettyStatusForStatusBar = base._GetPrettyStatusForStatusBar
        _GetNumSelected = base._GetNumSelected
        _GetPrettyTotalSize = base._GetPrettyTotalSize
        _GetPrettyTotalDuration = base._GetPrettyTotalDuration

        def __init__( self, media, override ):

            self._sorted_media = list( media )
            self._hashes = { m.GetHash() for m in media }
            self._selected_media = set()
            self._empty_page_status_override = override



    now = int( time.time() )

    real = ( HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat )

    HydrusTime.GetNow = lambda: now
    HydrusTime.GetNowMS = lambda: now * 1000
    HydrusTime.GetNowFloat = lambda: float( now )

    try:

        hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]

        media_results = controller.Read( 'media_results', hashes )

        media = [ ClientMediaSingle.MediaSingle( m ) for m in media_results ]

        by_mime = {}

        for m in media:

            by_mime.setdefault( m.GetMime(), [] ).append( m )


        largest = max( by_mime.values(), key = len )

        pages = {
            'all' : media,
            'images' : [ m for m in media if m.GetMime() in HC.IMAGES ],
            'one type' : largest,
            'animations' : [ m for m in media if m.GetMime() in HC.ANIMATIONS ],
            'videos' : [ m for m in media if m.GetMime() in HC.VIDEO ],
            'one file' : media[ : 1 ],
        }

        out_pages = []

        def status_of( page_media, selected, override = None ):

            page = Page( page_media, override )

            page._selected_media = set( selected )

            return page._GetPrettyStatusForStatusBar()


        def hex_of( ms ):

            return [ m.GetHash().hex() for m in ms ]


        for ( name, page_media ) in pages.items():

            if len( page_media ) == 0:

                continue


            inbox = [ m for m in page_media if m.HasInbox() ]
            archived = [ m for m in page_media if not m.HasInbox() ]

            selections = [ [], page_media[ : 3 ], inbox, archived, inbox[ : 2 ] + archived[ : 1 ], page_media ]

            if name == 'all':

                selections.extend( [ [ m ] for m in page_media ] )


            records = []

            for selected in selections:

                if len( selected ) == 0 and len( records ) > 0:

                    continue


                records.append( { 'selected' : hex_of( selected ), 'status' : status_of( page_media, selected ) } )


            out_pages.append( { 'name' : name, 'files' : hex_of( page_media ), 'override' : None, 'selections' : records } )


        for override in ( None, 'no files found for this search' ):

            out_pages.append( { 'name' : 'empty', 'files' : [], 'override' : override, 'selections' : [ { 'selected' : [], 'status' : status_of( [], [], override ) } ] } )


        # and the files on a page of their own, without the single file info
        options.SetBoolean( 'show_extended_single_file_info_in_status_bar', False )

        records = [ { 'selected' : hex_of( [ m ] ), 'status' : status_of( media, [ m ] ) } for m in media[ : 5 ] ]

        out_pages.append( { 'name' : 'all, no single file info', 'files' : hex_of( media ), 'override' : None, 'single_file_info' : False, 'selections' : records } )

    finally:

        ( HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat ) = real


    return { 'now' : now, 'pages' : out_pages }


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
