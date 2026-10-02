#!/usr/bin/env python3
"""Record the reference's recent system predicates.

What a system predicate editor (`FleshOutPredicatePanel`) adds, from a
button or a panel's "ok", the reference keeps as a recent predicate of its
type (`PushRecentPredicates`): newest first, five of each type at most, one
added again moved to the front. Each editor's page shows, in a "recent"
box, those of the types it edits, less any the page has a button for, each
with a button to forget it (`RemoveRecentPredicate`).

In the running client, on the `basic` fixture, this records:

* `new_client`: a new client's recent predicates (none);
* `steps`: in turn, an editor opened from the search box's offer, a
  button clicked or a panel's "ok" pressed in it (`FleshOutPredicates`,
  as the search box calls it), or predicates pushed as an editor pushes
  them, or one forgotten; after each, every type's recent predicates (by
  the reference's type number), newest first;
* `editors`: then, each editor's pages with the recent predicates each
  shows (by text), and whether each can be forgotten;
* `stored`: the option as the reference stores it
  (`predicate_types_to_recent_predicates`), for the migration;
* `types`: each recorded predicate's type number, by its text.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_recent_predicates.py
       (writes fixtures/recent_predicates.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'recent_predicates.json' )

# ( what, ... ): ( 'button', editor, button label ) clicks a ready-made
# button of the editor opened from the offered predicate starting `editor`;
# ( 'ok', editor, page, panel ) presses a panel's "ok" at its defaults;
# ( 'push', [ predicates ] ) pushes parsed predicates together, as an
# editor does what it adds; ( 'forget', predicate ) forgets one
STEPS = [
    ( 'ok', 'system:dimensions', '', 0 ),
    ( 'ok', 'system:dimensions', '', 1 ),
    ( 'button', 'system:dimensions', '1080p' ),
    ( 'button', 'system:dimensions', 'system:ratio = 16:9' ),
    ( 'ok', 'system:dimensions', '', 2 ),
    ( 'ok', 'system:dimensions', '', 3 ),
    ( 'ok', 'system:filesize', '', 0 ),
    ( 'ok', 'system:time', 'import', 0 ),
    ( 'ok', 'system:time', 'modified', 0 ),
    ( 'ok', 'system:duration', '', 0 ),
    ( 'ok', 'system:number of tags', '', 0 ),
    ( 'ok', 'system:urls', 'known urls', 0 ),
    ( 'push', [ 'system:width = 100' ] ),
    ( 'push', [ 'system:width = 200' ] ),
    ( 'push', [ 'system:width = 300' ] ),
    ( 'push', [ 'system:width = 400' ] ),
    # (a sixth width pushes the oldest out)
    ( 'push', [ 'system:width = 500' ] ),
    # (one again goes to the front)
    ( 'push', [ 'system:width = 300' ] ),
    ( 'push', [ 'system:height = 720', 'system:width = 1280' ] ),
    ( 'push', [ 'system:filesize < 10MB', 'system:filesize > 1KB', 'system:filesize ~= 5MB' ] ),
    ( 'push', [ 'system:limit is 100', 'system:limit is 64' ] ),
    ( 'push', [ 'system:number of notes > 1', 'system:has note with name "comment"' ] ),
    ( 'push', [ 'system:num frames > 100', 'system:framerate = 30fps' ] ),
    ( 'forget', 'system:width = 200' ),
    ( 'forget', 'system:limit is 64' ),
    # (one not there: nothing)
    ( 'forget', 'system:width = 9999' ),
]


def record( session ):

    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.search import ClientGUISearch
    from hydrus.client.gui.search import ClientGUIPredicatesSingle
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchParseSystemPredicates
    from hydrus.client.search import ClientSearchTagContext

    controller = session.controller
    gui = controller.gui
    options = controller.new_options

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )
    file_search_context = ClientSearchFileSearchContext.FileSearchContext( location_context = location, tag_context = ClientSearchTagContext.TagContext() )

    offered = controller.Read( 'file_system_predicates', file_search_context )

    blanks = [ p for p in offered if p.GetValue() is None and p.GetType() in ClientSearchGUI_types( ClientGUISearch ) ]

    def blank( text ):

        [ predicate ] = [ p for p in blanks if p.ToString().startswith( text ) ]

        return predicate


    types = {}

    def note( predicates ):

        for p in predicates:

            types[ p.ToString() ] = p.GetType()


        return predicates


    def recent():

        stored = options._dictionary[ 'predicate_types_to_recent_predicates' ]

        return { str( t ) : [ p.ToString() for p in note( list( ps ) ) ] for ( t, ps ) in sorted( stored.items() ) if len( ps ) > 0 }


    def pages_of( panel ):

        if hasattr( panel, '_notebook' ):

            return [ ( panel._notebook.tabText( i ), panel._notebook.widget( i ) ) for i in range( panel._notebook.count() ) ]

        else:

            return [ ( '', panel ) ]



    def recent_buttons( page ):

        return [ w for w in page.findChildren( ClientGUIPredicatesSingle.StaticSystemPredicateButton ) if not w._remove_button.isHidden() ]


    # what the editor dialog does when shown: the step's action
    action = {}

    real_exec = ClientGUITopLevelWindowsPanels.DialogEdit.exec

    def exec_( dialog ):

        panel = dialog._panel

        panel.show()

        if action[ 'what' ] == 'button':

            [ button ] = [ w for w in panel.findChildren( ClientGUIPredicatesSingle.StaticSystemPredicateButton ) if w._predicates_button.text() == action[ 'label' ] ]

            button._DoPredicatesChoose()

        else:

            [ page ] = [ p for ( name, p ) in pages_of( panel ) if name == action[ 'page' ] ]

            ok_panels = page.findChildren( ClientGUISearch.FleshOutPredicatePanel._PredOKPanel )

            ok_panels[ action[ 'panel' ] ]._DoOK()


        note( panel.GetValue() )

        return QW.QDialog.DialogCode.Accepted


    ClientGUITopLevelWindowsPanels.DialogEdit.exec = exec_

    def parse( texts ):

        return note( ClientSearchParseSystemPredicates.ParseSystemPredicateStringsToPredicates( texts ) )


    def step( s ):

        what = s[ 0 ]

        if what in ( 'button', 'ok' ):

            if what == 'button':

                action.update( { 'what' : 'button', 'label' : s[ 2 ] } )

            else:

                action.update( { 'what' : 'ok', 'page' : s[ 2 ], 'panel' : s[ 3 ] } )


            added = ClientGUISearch.FleshOutPredicates( gui, [ blank( s[ 1 ] ) ] )

            done = { 'added' : [ p.ToString() for p in note( added ) ] }

        elif what == 'push':

            options.PushRecentPredicates( parse( s[ 1 ] ) )

            done = {}

        else:

            [ predicate ] = parse( [ s[ 1 ] ] )

            options.RemoveRecentPredicate( predicate )

            done = {}


        return dict( done, step = list( s ), recent = recent() )


    def editor( predicate ):

        panel = ClientGUISearch.FleshOutPredicatePanel( gui, predicate )

        panel.show()

        pages = []

        for ( name, page ) in pages_of( panel ):

            pages.append( {
                'name' : name,
                'recent' : [ ', '.join( p.ToString() for p in w._predicates ) for w in recent_buttons( page ) ],
            } )


        # (a recent one forgotten from the editor goes from the option)
        forgot = None

        for ( name, page ) in pages_of( panel ):

            buttons = recent_buttons( page )

            if len( buttons ) > 0 and forgot is None:

                forgot = { 'page' : name, 'predicate' : buttons[ -1 ]._predicates[ 0 ].ToString() }

                buttons[ -1 ]._DoPredicatesRemove()

                forgot[ 'shown' ] = [ ', '.join( p.ToString() for p in w._predicates ) for w in recent_buttons( page ) if w.isVisible() ]
                forgot[ 'recent' ] = recent()



        panel.deleteLater()

        return { 'editor' : predicate.ToString(), 'pages' : pages, 'forgot' : forgot }


    try:

        out = { 'new_client' : qt( recent ), 'steps' : [], 'editors' : [] }

        for s in STEPS:

            out[ 'steps' ].append( qt( lambda: step( s ) ) )


        stored = qt( lambda: json.loads( json.dumps( options._dictionary[ 'predicate_types_to_recent_predicates' ].GetSerialisableTuple() ) ) )

        # each editor as the recent predicates stand (the first to have
        # any forgets one, which the next editors then don't show)
        for predicate in blanks:

            out[ 'editors' ].append( qt( lambda: editor( predicate ) ) )


        out[ 'stored' ] = stored
        out[ 'types' ] = types

        return out

    finally:

        ClientGUITopLevelWindowsPanels.DialogEdit.exec = real_exec



def ClientSearchGUI_types( ClientGUISearch ):

    return ClientGUISearch.FLESH_OUT_SYSTEM_PRED_TYPES


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
