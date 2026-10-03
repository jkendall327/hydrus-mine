#!/usr/bin/env python3
"""Record the reference's "manage ratings" dialog (`DialogManageRatings`).

In the running client, on the `basic` fixture (a like/dislike service
"favourites", a numerical one "stars" and an inc/dec one "counter"), for
each case the real dialog is made over stand-in files rated as the case
says (each file's ratings by service name: a like is 1.0 and a dislike
0.0, stars a fraction, a count a number; a file not rated on a service
has no entry), and driven step by step:

* `["left", name]` / `["right", name]`: a left or right click on the
  service's control (a like's and an inc/dec's own click handlers; a
  numerical rating's right click clears it);
* `["stars", name, n]`: a numerical control clicked at its nth star (the
  rating `ConvertStarsToRating` makes of it);
* `["copy"]`: copy, recording what was copied and the notice;
* `["paste", pairs]`: paste these pairs, each `[name, rating]` with the
  service's key in place of its name (an unknown name is left as it is),
  as the reference's JSON; `["paste_raw", text]` pastes text as it is;
* `["ok"]`: apply, recording the content updates written (each service's
  name, the rating, and the files' hashes, sorted).

After each step `state` has each control, in the dialog's order: its
service's name, its state (ClientRatings' LIKE 0, DISLIKE 1, NULL 2, SET
3, MIXED 4) and its rating (a numerical or inc/dec control's; null for a
like). `said` has any notices and errors shown. `keys` maps each service's
name to its key, as hex.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_manage_ratings.py
       (writes fixtures/manage_ratings.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'manage_ratings.json' )

CASES = [
    { 'files' : [ [ 'aa', { 'favourites' : 1.0, 'stars' : 0.6, 'counter' : 3 } ] ], 'steps' : [
        [ 'copy' ],
        [ 'left', 'favourites' ],
        [ 'left', 'favourites' ],
        [ 'right', 'favourites' ],
        [ 'right', 'favourites' ],
        [ 'right', 'favourites' ],
        [ 'stars', 'stars', 4 ],
        [ 'left', 'counter' ],
        [ 'left', 'counter' ],
        [ 'right', 'counter' ],
        [ 'copy' ],
        [ 'ok' ],
    ] },
    { 'files' : [
        [ 'aa', { 'favourites' : 1.0, 'stars' : 0.6, 'counter' : 3 } ],
        [ 'bb', { 'favourites' : 0.0 } ],
        [ 'cc', { 'stars' : 0.6, 'counter' : 8 } ],
    ], 'steps' : [
        [ 'copy' ],
        [ 'left', 'counter' ],
        [ 'right', 'counter' ],
        [ 'paste', [ [ 'favourites', 1 ], [ 'stars', None ], [ 'nothing', 1 ], [ 'counter', 9 ] ] ],
        [ 'copy' ],
        [ 'paste_raw', 'this is not json at all, but it does go on for quite a while, longer than sixty-four' ],
        [ 'paste', [ [ 'favourites', 0.5 ], [ 'stars', 2 ], [ 'counter', 'x' ] ] ],
        [ 'paste', [ [ 'favourites', None ], [ 'stars', 0.4 ] ] ],
        [ 'copy' ],
        [ 'ok' ],
    ] },
    { 'files' : [ [ 'aa', {} ], [ 'bb', {} ] ], 'steps' : [
        [ 'copy' ],
        [ 'right', 'stars' ],
        [ 'right', 'counter' ],
        [ 'right', 'favourites' ],
        [ 'right', 'favourites' ],
        [ 'ok' ],
    ] },
    { 'files' : [ [ 'aa', { 'favourites' : 0.0, 'stars' : 1.0 } ], [ 'bb', { 'favourites' : 0.0, 'stars' : 1.0 } ] ], 'steps' : [
        [ 'right', 'stars' ],
        [ 'stars', 'stars', 1 ],
        [ 'left', 'favourites' ],
        [ 'ok' ],
    ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui import ClientGUIDialogsManage as M
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.media import ClientMediaManagers
        from hydrus.client.metadata import ClientRatings

        services = { s.GetName() : s for s in controller.services_manager.GetServices( HC.RATINGS_SERVICES ) }
        keys = { name : s.GetServiceKey().hex() for ( name, s ) in services.items() }
        names = { s.GetServiceKey() : name for ( name, s ) in services.items() }

        said = []
        clipboard = []
        written = []
        pasted = []

        real_pub = controller.pub

        def pub( topic, *args, **kwargs ):

            if topic == 'clipboard':

                clipboard.append( args[ 1 ] )

                return


            real_pub( topic, *args, **kwargs )


        controller.pub = pub
        controller.GetClipboardText = lambda: pasted[ -1 ]

        def write( name, package, *args, **kwargs ):

            for ( service_key, content_updates ) in package.IterateContentUpdates():

                for cu in content_updates:

                    ( rating, hashes ) = cu.GetRow()

                    written.append( [ names[ service_key ], rating, sorted( h.hex() for h in hashes ) ] )




        controller.Write = write

        ClientGUIFunctions.ShowMicroNotification = lambda win, text: said.append( { 'notice' : text } )
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: said.append( { 'critical' : title, 'message' : message } )

        class Media:

            # (a stand-in file: its hash and ratings)
            def __init__( self, hash, ratings ):

                self._hash = hash
                self._ratings = ClientMediaManagers.RatingsManager( { services[ name ].GetServiceKey() : rating for ( name, rating ) in ratings.items() } )


            def GetHashes( self ):

                return { self._hash }


            def GetRatingsManager( self ):

                return self._ratings



        from qtpy import QtCore as QC
        from qtpy import QtGui as QG

        def Event( button ):

            return QG.QMouseEvent( QC.QEvent.Type.MouseButtonPress, QC.QPointF( 1, 1 ), QC.QPointF( 1, 1 ), button, button, QC.Qt.KeyboardModifier.NoModifier )


        out = []

        for case in CASES:

            medias = [ Media( bytes.fromhex( h ), ratings ) for ( h, ratings ) in case[ 'files' ] ]

            dialog = M.DialogManageRatings( gui, medias )

            controls = {}

            for panel in dialog._panels:

                for ( service_key, control ) in panel._service_keys_to_controls.items():

                    controls[ names[ service_key ] ] = control



            def state():

                rows = []

                for ( name, control ) in controls.items():

                    rating = None if isinstance( control, M.ClientGUIRatings.RatingLike ) else control.GetRating()

                    rows.append( [ name, int( control.GetRatingState() ), rating ] )


                return rows


            steps = [ { 'state' : state() } ]

            for step in case[ 'steps' ]:

                del said[:]
                del clipboard[:]
                del written[:]

                kind = step[ 0 ]

                if kind in ( 'left', 'right' ):

                    control = controls[ step[ 1 ] ]

                    left = kind == 'left'

                    if isinstance( control, M.ClientGUIRatings.RatingIncDec ):

                        control.mousePressEvent( Event( QC.Qt.MouseButton.LeftButton if left else QC.Qt.MouseButton.RightButton ) )

                    elif left:

                        control.EventLeftDown( None )

                    else:

                        control.EventRightDown( None )


                elif kind == 'stars':

                    service = services[ step[ 1 ] ]

                    controls[ step[ 1 ] ]._SetRating( ClientRatings.ConvertStarsToRating( service.GetNumStars(), service.AllowZero(), step[ 2 ] ) )

                elif kind == 'copy':

                    dialog._Copy()

                elif kind in ( 'paste', 'paste_raw' ):

                    if kind == 'paste':

                        pasted.append( json.dumps( [ [ keys.get( name, name ), rating ] for ( name, rating ) in step[ 1 ] ] ) )

                    else:

                        pasted.append( step[ 1 ] )


                    dialog._Paste()

                elif kind == 'ok':

                    dialog.EventOK()


                steps.append( { 'do' : step, 'said' : list( said ), 'copied' : list( clipboard ), 'written' : list( written ), 'state' : state() } )


            dialog.deleteLater()

            out.append( { 'files' : case[ 'files' ], 'steps' : steps } )


        stars = services[ 'stars' ]

        return { 'keys' : keys, 'stars' : { 'num_stars' : stars.GetNumStars(), 'allow_zero' : stars.AllowZero() }, 'cases' : out }


    return controller.CallBlockingToQt( gui, f )


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( out, 'w' ) as f:

        json.dump( result, f, ensure_ascii = False )



def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ] )

        return


    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'manage_ratings.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()
