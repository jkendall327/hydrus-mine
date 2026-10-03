#!/usr/bin/env python3
"""Record the ratings the reference draws over its thumbnails.

In the running client, on the `basic` fixture (a like/dislike service
"favourites", a numerical one "stars" and an inc/dec one "counter"), some
files are rated, and for each of `CASES` (which services show in
thumbnails, and whether even when a file has no rating there) a page of the
fixture's files is opened and each thumbnail of the default (graphics
view) grid is painted (`_PaintThumbnailContent`) on a painter that notes:

* `ratings`: each rating drawn (`DrawLike`, `DrawNumerical`, `DrawIncDec`):
  its service's name, state, rating and where;
* `boxes`: each rectangle filled behind them (the options'
  `draw_thumbnail_rating_background`), and where;
* `icons`: each icon drawn, by name, and where (they go under the ratings);
* `texts`: each text drawn (a numerical rating's "stars/of", an inc/dec
  rating's number), with the rectangle it is drawn in and its font's
  pixel size.

Each case also sets the options the drawing reads (`OPTIONS`, defaults
where not given) and how the numerical service shows its "stars/of"
beside its stars (none, on the left, on the right), and records them.
Each thumbnail is named by its file's hash, with its size. `rated` has
what was rated (or unrated, as None), over the fixture's own ratings.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_thumbnail_ratings.py
       (writes fixtures/thumbnail_ratings.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'thumbnail_ratings.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

PIXMAPS = [ 'downloading', 'collection', 'inbox', 'trash', 'play', 'sound', 'notes' ]

# (service name, the file's place in the manifest, rating): a like is 1.0
# and a dislike 0.0, stars a fraction of the service's, a count a number
RATINGS = [
    ( 'favourites', 0, 1.0 ), ( 'favourites', 1, 0.0 ), ( 'favourites', 3, 1.0 ),
    ( 'stars', 0, 0.6 ), ( 'stars', 2, 1.0 ), ( 'stars', 3, 0.2 ),
    ( 'counter', 0, 3 ), ( 'counter', 2, 12345 ), ( 'counter', 4, 7 ),
    # unrated (the fixture rates each of these files)
    ( 'favourites', 2, None ), ( 'stars', 4, None ),
]

ALWAYS = { 'favourites' : ( True, True ), 'stars' : ( True, True ), 'counter' : ( True, True ) }

# the options as a new client has them
OPTIONS = { 'icon_size' : 12.0, 'incdec_height' : 12.0, 'background' : True, 'collapsed' : False }

# which services show in thumbnails: name -> ( show, even when unrated )
CASES = [
    ( 'all shown when rated', { 'favourites' : ( True, False ), 'stars' : ( True, False ), 'counter' : ( True, False ) } ),
    ( 'all shown always', { 'favourites' : ( True, True ), 'stars' : ( True, True ), 'counter' : ( True, True ) } ),
    ( 'stars only', { 'favourites' : ( False, False ), 'stars' : ( True, False ), 'counter' : ( False, False ) } ),
    ( 'like and counter always', { 'favourites' : ( True, True ), 'stars' : ( False, False ), 'counter' : ( True, True ) } ),
    ( 'no background', ALWAYS, { 'background' : False } ),
    ( 'bigger', ALWAYS, { 'icon_size' : 20.0, 'incdec_height' : 16.0 } ),
    ( 'smaller', ALWAYS, { 'icon_size' : 9.4, 'incdec_height' : 7.6 } ),
    ( 'collapsed', ALWAYS, { 'collapsed' : True } ),
    ( 'fraction on the left', ALWAYS, {}, 1 ),
    ( 'fraction on the right', ALWAYS, {}, 2 ),
    ( 'collapsed with the fraction on the right', ALWAYS, { 'collapsed' : True }, 2 ),
]

FILES = 6


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtGui as QG

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIRatings
    from hydrus.client.metadata import ClientContentUpdates

    controller = session.controller
    gui = controller.gui
    options = controller.new_options

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    services = { s.GetName() : s for s in controller.services_manager.GetServices( HC.RATINGS_SERVICES ) }
    names_by_key = { s.GetServiceKey().hex() : name for ( name, s ) in services.items() }

    pixmaps = CC.global_pixmaps()

    names = { getattr( pixmaps, name ).cacheKey() : name for name in PIXMAPS }

    drawn = []

    class Noting( QG.QPainter ):

        def drawPixmap( self, *args ):

            if len( args ) == 3 and isinstance( args[ 2 ], QG.QPixmap ):

                ( x, y, pixmap ) = args

                drawn.append( ( 'icon', names.get( pixmap.cacheKey(), 'other' ), int( x ), int( y ) ) )


            return super().drawPixmap( *args )


        def drawText( self, *args ):

            if len( args ) in ( 2, 3 ) and isinstance( args[ 0 ], ( QC.QRect, QC.QRectF ) ):

                rect = args[ 0 ]

                drawn.append( ( 'text', args[ -1 ], round( rect.x() ), round( rect.y() ), round( rect.width() ), round( rect.height() ), self.font().pixelSize() ) )


            return super().drawText( *args )


        def fillRect( self, *args ):

            if len( args ) == 5:

                ( x, y, w, h, _ ) = args

                drawn.append( ( 'box', int( x ), int( y ), int( w ), int( h ) ) )


            return super().fillRect( *args )



    real_ratings = { name : getattr( ClientGUIRatings, name ) for name in ( 'DrawLike', 'DrawNumerical', 'DrawIncDec' ) }

    def noting_rating( name ):

        def draw( painter, x, y, service_key, rating_state, *args, **kwargs ):

            rating = args[ 0 ] if name != 'DrawLike' and len( args ) > 0 else None

            drawn.append( ( 'rating', name, names_by_key.get( service_key.hex(), '?' ), int( rating_state ), rating, int( x ), int( y ) ) )

            return real_ratings[ name ]( painter, x, y, service_key, rating_state, *args, **kwargs )


        return draw


    for name in real_ratings:

        setattr( ClientGUIRatings, name, noting_rating( name ) )


    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ][ : FILES ]

    for ( name, index, rating ) in RATINGS:

        content_update = ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_RATINGS, HC.CONTENT_UPDATE_ADD, ( rating, [ hashes[ index ] ] ) )

        controller.WriteSynchronous( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate( services[ name ].GetServiceKey(), content_update ) )


    def open_page():

        location = ClientLocation.LocationContext.STATICCreateSimple( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY )

        page = qt( lambda: gui._notebook.NewPageQuery( location, initial_hashes = hashes ) )

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

            out.append( {
                'file' : media.GetHash().hex(),
                'size' : [ item.width, item.height ],
                'ratings' : [ { 'draw' : n, 'service' : s, 'state' : st, 'rating' : r, 'x' : x, 'y' : y } for ( kind, n, s, st, r, x, y ) in ( d for d in drawn if d[ 0 ] == 'rating' ) ],
                'boxes' : [ { 'x' : x, 'y' : y, 'width' : w, 'height' : h } for ( kind, x, y, w, h ) in ( d for d in drawn if d[ 0 ] == 'box' ) ],
                'icons' : [ { 'name' : n, 'x' : x, 'y' : y } for ( kind, n, x, y ) in ( d for d in drawn if d[ 0 ] == 'icon' ) ],
                'texts' : [ { 'text' : t, 'x' : x, 'y' : y, 'width' : w, 'height' : h, 'pixel_size' : px } for ( kind, t, x, y, w, h, px ) in ( d for d in drawn if d[ 0 ] == 'text' ) ],
            } )


        return out


    try:

        def set_options( given ):

            o = dict( OPTIONS, **given )

            options.SetFloat( 'draw_thumbnail_rating_icon_size_px', o[ 'icon_size' ] )
            options.SetFloat( 'thumbnail_rating_incdec_height_px', o[ 'incdec_height' ] )
            options.SetBoolean( 'draw_thumbnail_rating_background', o[ 'background' ] )
            options.SetBoolean( 'draw_thumbnail_numerical_ratings_collapsed_always', o[ 'collapsed' ] )


        def read_options():

            return {
                'icon_size' : options.GetFloat( 'draw_thumbnail_rating_icon_size_px' ),
                'incdec_height' : options.GetFloat( 'thumbnail_rating_incdec_height_px' ),
                'background' : options.GetBoolean( 'draw_thumbnail_rating_background' ),
                'collapsed' : options.GetBoolean( 'draw_thumbnail_numerical_ratings_collapsed_always' ),
            }


        out = {
            'thumbnail_border' : options.GetInteger( 'thumbnail_border' ),
            'new_client_options' : read_options(),
            'rated' : [ { 'service' : name, 'file' : hashes[ index ].hex(), 'rating' : rating } for ( name, index, rating ) in RATINGS ],
            'services' : { name : { 'num_stars' : s.GetNumStars() if hasattr( s, 'GetNumStars' ) else None, 'custom_pad' : s.GetCustomPad() if hasattr( s, 'GetCustomPad' ) else None } for ( name, s ) in services.items() },
            'cases' : [],
        }

        for ( case, flags, *rest ) in CASES:

            given = rest[ 0 ] if len( rest ) > 0 else {}
            beside = rest[ 1 ] if len( rest ) > 1 else 0

            for ( name, ( show, even_when_null ) ) in flags.items():

                services[ name ]._show_in_thumbnail = show
                services[ name ]._show_in_thumbnail_even_when_null = even_when_null


            services[ 'stars' ]._show_fraction_beside_stars = beside

            set_options( given )

            panel = open_page()

            out[ 'cases' ].append( {
                'case' : case,
                'flags' : { n : list( f ) for ( n, f ) in flags.items() },
                'options' : read_options(),
                'fraction_beside' : beside,
                'thumbnails' : qt( lambda: thumbnails( panel ) ),
            } )


        return out

    finally:

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
