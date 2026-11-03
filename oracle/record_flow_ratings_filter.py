#!/usr/bin/env python3
"""Flow: rate files, run the archive/delete filter over them, and search by
rating and inbox.

On the `basic` fixture (no site; the flow is all local), on a search page
of every file, with five still images F1-F5 (the fixture's first five
`jpeg_0*` files the page shows) sent to the inbox first:

1. "rate": F1 and F2 selected; manage > ratings (the real
   `DialogManageRatings`): "favourites" liked (a left click), "stars" set
   to 4/5, "counter" up twice (two left clicks); OK. Then F3: "favourites"
   disliked (a right click), "stars" 1/5; OK.
2. "filter": F1-F5 selected; the archive/delete filter (the real
   `CanvasMediaListFilterArchiveDelete` on the page's files): keep F1,
   delete F2, skip F3, keep F4, delete F5; the last decision closes the
   filter, which asks (`GetFinishArchiveDeleteFilteringAnswer`, the real
   panel shown) and is answered "commit".
3. "search": search pages for `system:inbox`, `system:archive`, and
   ratings: favourites liked, disliked, stars above 3/5, counter = 2 (as
   the reference parses them from their text).

After each step: what the windows showed (the ratings dialog's title, the
filter's order and its question with its labels and buttons; the ratings
dialog's controls are the leaf tests' business) and each of the five
files: its ratings, inbox and domains.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_ratings_filter.py
       (writes oracle/fixtures/flow_ratings_filter.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_ratings_filter.json' )

SEARCHES = [
    'system:inbox',
    'system:archive',
    'system:rating for favourites is like',
    'system:rating for favourites is dislike',
    'system:rating for stars > 3/5',
    'system:rating for counter = 2',
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtGui as QG
    from qtpy import QtWidgets as QW
    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUIDialogsManage as M
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.canvas import ClientGUICanvas as Canvas
    from hydrus.client.gui.panels import ClientGUIScrolledPanelsCommitFiltering as Finish
    from hydrus.client.media import ClientMediaFileFilter
    from hydrus.client.metadata import ClientRatings
    from hydrus.client.search import ClientSearchParseSystemPredicates
    from hydrus.client.search import ClientSearchPredicate as P

    controller = session.controller
    gui = controller.gui
    services_manager = controller.services_manager

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    services = { s.GetName() : s for s in services_manager.GetServices( HC.RATINGS_SERVICES ) }
    names = { s.GetServiceKey() : name for ( name, s ) in services.items() }
    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    # questions answered yes (sending to the inbox asks)
    asked = []

    def get_yes_no( win, message, *args, **kwargs ):

        asked.append( message )

        return QW.QDialog.DialogCode.Accepted


    ClientGUIDialogsQuick.GetYesNo = get_yes_no

    steps = []

    def pump_off_thread( condition, what, timeout = 60 ):

        deadline = time.time() + timeout

        while not condition():

            if time.time() > deadline:

                raise Exception( 'timed out: ' + what )


            time.sleep( 0.1 )



    def new_page( predicates ):

        page = qt( lambda: gui._notebook.NewPageQuery( location, initial_predicates = predicates, do_sort = False ) )

        last = [ None, 0 ]

        def loaded():

            panel = qt( page.GetMediaResultsPanel )

            if not hasattr( panel, '_sorted_media' ):

                return False


            n = len( panel._sorted_media )

            if n != last[0]:

                last[0] = n
                last[1] = time.time()

                return False


            return time.time() - last[1] > 1.5


        pump_off_thread( loaded, 'the page to load' )

        return page


    page = new_page( [ P.Predicate( P.PREDICATE_TYPE_SYSTEM_EVERYTHING ) ] )

    def media_panel():

        return page.GetMediaResultsPanel()


    listed = { m.GetHash().hex() for m in qt( lambda: media_panel().GetSortedMedia() ) }
    files = [ f[ 'hash' ] for f in manifest[ 'files' ] if f[ 'name' ].startswith( 'jpeg_0' ) and f[ 'hash' ] in listed ][ : 5 ]

    ( F1, F2, F3, F4, F5 ) = files

    def select( hashes ):

        panel = media_panel()

        panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_NONE ) )

        for ( i, h ) in enumerate( hashes ):

            media = next( m for m in panel.GetSortedMedia() if m.GetHash().hex() == h )

            panel._HitMedia( media, i > 0, False )



    def file_state( hash_hex ):

        media_result = session.read( 'media_result', bytes.fromhex( hash_hex ) )
        ratings = media_result.GetRatingsManager()
        locations = media_result.GetLocationsManager()

        return {
            'ratings' : { names[ k ] : ratings.GetRating( k ) for k in sorted( names, key = lambda k: names[ k ] ) },
            'inbox' : locations.inbox,
            'in' : sorted( services_manager.GetName( k ) for k in locations.GetCurrent() if services_manager.GetService( k ).GetServiceType() in ( HC.LOCAL_FILE_DOMAIN, HC.LOCAL_FILE_TRASH_DOMAIN ) ),
        }


    def step( name, seen, extra = None ):

        flows.wait_until_idle( controller )

        steps.append( {
            'step' : name,
            'dialog' : seen,
            'files' : { h : file_state( h ) for h in files },
            **( extra or {} ),
        } )


    # (the five sent to the inbox, so the filter has something to archive)
    qt( lambda: select( files ) )
    qt( lambda: media_panel()._Inbox() )

    flows.wait_until_idle( controller )

    asked.clear()

    # 1. rate -----------------------------------------------------------

    def Event( button ):

        return QG.QMouseEvent( QC.QEvent.Type.MouseButtonPress, QC.QPointF( 1, 1 ), QC.QPointF( 1, 1 ), button, button, QC.Qt.KeyboardModifier.NoModifier )


    def rate( hashes, actions ):
        """Manage ratings on `hashes`: `actions` `( service, what )` done on its
        controls, then OK. What the dialog showed."""

        seen = {}

        real_exec = M.DialogManageRatings.exec

        def execute( dialog ):

            def act():

                controls = {}

                for panel in dialog._panels:

                    for ( service_key, control ) in panel._service_keys_to_controls.items():

                        controls[ names[ service_key ] ] = control



                seen[ 'title' ] = dialog.windowTitle()

                for ( name, what ) in actions:

                    control = controls[ name ]

                    if what == 'left':

                        if isinstance( control, M.ClientGUIRatings.RatingIncDec ):

                            control.mousePressEvent( Event( QC.Qt.MouseButton.LeftButton ) )

                        else:

                            control.EventLeftDown( None )


                    elif what == 'right':

                        if isinstance( control, M.ClientGUIRatings.RatingIncDec ):

                            control.mousePressEvent( Event( QC.Qt.MouseButton.RightButton ) )

                        else:

                            control.EventRightDown( None )


                    else:

                        service = services[ name ]

                        control._SetRating( ClientRatings.ConvertStarsToRating( service.GetNumStars(), service.AllowZero(), what ) )



                dialog.EventOK()


            QC.QTimer.singleShot( 0, act )

            return real_exec( dialog )


        M.DialogManageRatings.exec = execute

        try:

            qt( lambda: select( hashes ) )
            qt( lambda: media_panel()._ManageRatings() )

        finally:

            M.DialogManageRatings.exec = real_exec


        return seen


    seen = {
        'F1 F2' : rate( [ F1, F2 ], [ ( 'favourites', 'left' ), ( 'stars', 4 ), ( 'counter', 'left' ), ( 'counter', 'left' ) ] ),
        'F3' : rate( [ F3 ], [ ( 'favourites', 'right' ), ( 'stars', 1 ) ] ),
    }

    step( 'rate', seen )

    # 2. filter ---------------------------------------------------------

    seen = {}

    real_finish = Finish.GetFinishArchiveDeleteFilteringAnswer

    def finish( win, kept_label, deletion_options ):

        dlg = ClientGUITopLevelWindowsPanels.DialogCustomButtonQuestion( win, 'filtering done?' )
        panel = Finish.QuestionArchiveDeleteFinishFilteringPanel( dlg, kept_label, deletion_options )

        dlg.SetPanel( panel )
        dlg.show()

        QW.QApplication.processEvents()

        buttons = panel.findChildren( QW.QPushButton )

        labels = [ x.text() for x in panel.findChildren( QW.QLabel ) if x.text() and x.text() != '-or-' ]

        # (the question, its commit buttons; "forget" and "back to filtering"
        # are always there)
        seen[ 'question' ] = {
            'question' : labels[0],
            'commit buttons' : [ b.text() for b in buttons if b.text() not in ( 'forget', 'back to filtering' ) ],
        }

        next( b for b in buttons if b.text() == 'commit' ).click()

        location_context = panel.GetLocationContext()

        dlg.hide()
        dlg.deleteLater()

        return ( QW.QDialog.DialogCode.Accepted, location_context, False )


    Finish.GetFinishArchiveDeleteFilteringAnswer = finish

    try:

        qt( lambda: select( files ) )
        qt( lambda: media_panel()._ArchiveDeleteFilter() )

        def canvas():

            # (its frame has no parent: a window of its own)
            found = [ c for w in QW.QApplication.topLevelWidgets() for c in w.findChildren( Canvas.CanvasMediaListFilterArchiveDelete ) if c.isVisible() ]

            return found[0] if len( found ) > 0 else None


        pump_off_thread( lambda: qt( canvas ) is not None, 'the filter' )

        the_canvas = qt( canvas )

        order = qt( lambda: [ m.GetHash().hex() for m in the_canvas._media_list.GetSortedMedia() ] )

        seen[ 'order' ] = [ files.index( h ) + 1 for h in order ]

        decisions = { F1 : 'keep', F2 : 'delete', F3 : 'skip', F4 : 'keep', F5 : 'delete' }

        for h in order:

            def decide():

                assert the_canvas._current_media.GetHash().hex() == h

                getattr( the_canvas, '_' + decisions[ h ].capitalize() )()


            qt( decide )

            time.sleep( 0.2 )


    finally:

        Finish.GetFinishArchiveDeleteFilteringAnswer = real_finish


    pump_off_thread( lambda: qt( canvas ) is None, 'the filter to close' )

    step( 'filter', seen )

    # 3. search ---------------------------------------------------------

    searches = {}

    for text in SEARCHES:

        [ predicate ] = ClientSearchParseSystemPredicates.ParseSystemPredicateStringsToPredicates( [ text ] )

        searched = new_page( [ predicate ] )

        found = { m.GetHash().hex() for m in qt( lambda: searched.GetMediaResultsPanel().GetSortedMedia() ) }

        searches[ text ] = { 'predicate' : predicate.ToString(), 'files' : [ i + 1 for ( i, h ) in enumerate( files ) if h in found ] }


    step( 'search', {}, { 'searches' : searches } )

    return { 'files' : files, 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )

    flows.write_fixture( out, result )


def main():

    if len( sys.argv ) > 2 and sys.argv[1] == '--child':

        child( sys.argv[2] )

        return


    import hydrus_driver

    hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', OUT )


if __name__ == '__main__':

    main()
