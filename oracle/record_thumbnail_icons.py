#!/usr/bin/env python3
"""Record what the reference draws over its thumbnails.

In the running client, on the `basic` fixture, a page of every file (its
trashed ones too) is opened, and then the same files collected by series, and each thumbnail of the default (graphics view) grid is painted
(`_PaintThumbnailContent`) on a painter that notes what is drawn on it:

* `icons`: each icon drawn, by its name in the reference's pixmaps
  (`inbox`, `trash`, `notes`, `sound`, `play`, `collection`,
  `file_repository`...), with where its top left corner went;
* `texts`: each text drawn (a collection's number of files, a tag
  banner) and where;
* `ratings`: each rating drawn (`DrawLike`, `DrawNumerical`,
  `DrawIncDec`), its service, state, rating and where.

Each thumbnail is named by its file's hash (a collection's by its files'
hashes, sorted), with its size; `thumbnail_border` is the options'.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_thumbnail_icons.py
       (writes fixtures/thumbnail_icons.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'thumbnail_icons.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

PIXMAPS = [
    'downloading', 'file_repository', 'file_repository_pending', 'file_repository_petitioned',
    'ipfs', 'ipfs_pending', 'ipfs_petitioned', 'collection', 'inbox', 'trash', 'play', 'sound', 'notes',
]


def record( session ):

    from qtpy import QtGui as QG

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIFunctions
    from hydrus.client.gui import ClientGUIRatings
    from hydrus.client.media import ClientMediaCollect

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    pixmaps = CC.global_pixmaps()

    names = { getattr( pixmaps, name ).cacheKey() : name for name in PIXMAPS }

    drawn = []

    class Noting( QG.QPainter ):

        def drawPixmap( self, *args ):

            if len( args ) == 3 and isinstance( args[ 2 ], QG.QPixmap ):

                ( x, y, pixmap ) = args

                drawn.append( ( 'icon', names.get( pixmap.cacheKey(), 'other' ), int( x ), int( y ) ) )


            return super().drawPixmap( *args )



    real_text = ClientGUIFunctions.DrawText

    def noting_text( painter, x, y, text ):

        drawn.append( ( 'text', text, int( x ), int( y ) ) )

        return real_text( painter, x, y, text )


    real_ratings = { name : getattr( ClientGUIRatings, name ) for name in ( 'DrawLike', 'DrawNumerical', 'DrawIncDec' ) }

    def noting_rating( name ):

        def draw( painter, x, y, service_key, rating_state, *args, **kwargs ):

            rating = args[ 0 ] if name != 'DrawLike' and len( args ) > 0 else None

            drawn.append( ( 'rating', name, service_key.hex(), int( rating_state ), rating, int( x ), int( y ) ) )

            return real_ratings[ name ]( painter, x, y, service_key, rating_state, *args, **kwargs )


        return draw


    ClientGUIFunctions.DrawText = noting_text

    for name in real_ratings:

        setattr( ClientGUIRatings, name, noting_rating( name ) )


    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]

    def open_page( service_key, collect = None ):

        location = ClientLocation.LocationContext.STATICCreateSimple( service_key )

        page = qt( lambda: gui._notebook.NewPageQuery( location, initial_hashes = hashes, initial_collect = collect ) )

        for _ in range( 600 ):

            panel = qt( lambda: page.GetMediaResultsPanel() )

            if hasattr( panel, '_media_to_thumbnails' ) and len( panel._sorted_media ) > 0 and len( panel._media_to_thumbnails ) == len( panel._sorted_media ):

                return panel


            time.sleep( 0.05 )


        raise Exception( 'the page never loaded' )


    def thumbnails( panel ):

        out = []

        for media in list( panel._sorted_media ):

            item = panel._media_to_thumbnails[ media ]

            image = QG.QImage( item.width, item.height, QG.QImage.Format.Format_ARGB32_Premultiplied )

            image.setDevicePixelRatio( 1.0 )

            painter = Noting( image )

            drawn.clear()

            item._PaintThumbnailContent( painter, item.media, item._view )

            painter.end()

            if media.IsCollection():

                files = sorted( m.GetHash().hex() for m in media.GetFlatMedia() )

            else:

                files = [ media.GetHash().hex() ]


            out.append( {
                'files' : files,
                'size' : [ item.width, item.height ],
                'icons' : [ { 'name' : n, 'x' : x, 'y' : y } for ( kind, n, x, y ) in drawn if kind == 'icon' ],
                'texts' : [ { 'text' : t, 'x' : x, 'y' : y } for ( kind, t, x, y ) in drawn if kind == 'text' ],
                'ratings' : [ { 'draw' : n, 'service' : s, 'state' : st, 'rating' : r, 'x' : x, 'y' : y } for ( kind, n, s, st, r, x, y ) in ( d for d in drawn if d[ 0 ] == 'rating' ) ],
            } )


        return out


    try:

        out = {
            'thumbnail_border' : controller.new_options.GetInteger( 'thumbnail_border' ),
        }

        # (each page opened here, waiting for it to load, then painted in
        # Qt's thread)
        collect = ClientMediaCollect.MediaCollect( namespaces = [ 'series' ] )

        for ( name, service_key, page_collect ) in (
            ( 'files', CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, None ),
            ( 'collected_by_series', CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, collect ),
        ):

            panel = open_page( service_key, page_collect )

            out[ name ] = qt( lambda: thumbnails( panel ) )

        return out

    finally:

        ClientGUIFunctions.DrawText = real_text

        for ( name, f ) in real_ratings.items():

            setattr( ClientGUIRatings, name, f )




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
