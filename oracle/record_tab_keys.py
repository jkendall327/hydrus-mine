#!/usr/bin/env python3
"""Record selecting tabs by a click and by Ctrl+PageUp / Ctrl+PageDown, and
the labels of the tab right-click menu's "pages" list.

In the running client, on the `basic` fixture, the main notebook is given a
search page "a", a notebook "n" holding search pages "b" and "c", and a search
page "d" (a, b and c holding some of the fixture's files, "d" none). Then a
script of steps, each real input to the real window:

* `["click", name]`: a real left click on that page's tab;
* `["key", "down" | "up"]`: a real Ctrl+PageDown / Ctrl+PageUp press (the
  window's shortcut for `PagesNotebook.MoveSelection`);
* `["wait"]`: three seconds are taken to have passed since the notebooks last
  moved a selection (the reference descends into the shown notebook first
  unless the notebook above moved in the last three seconds);
* `["menu", name]`: the tab's right-click menu opened (as
  `_ShowMenuForTabIndex`), its "pages" submenu read;
* `["choose", name, entry]`: the same menu's "pages" entry chosen.

After each step: the page shown, and the notebooks' tab names (the top
notebook, then each notebook inside the shown page) with the index each has
selected.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_tab_keys.py
       (writes fixtures/tab_keys.json)
"""

import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path( __file__ ).resolve().parent

sys.path.insert( 0, str( HERE ) )

STEPS = [
    [ 'click', 'n' ],
    [ 'key', 'down' ],
    [ 'key', 'down' ],
    [ 'key', 'down' ],
    [ 'key', 'up' ],
    [ 'key', 'up' ],
    [ 'wait' ],
    [ 'click', 'n' ],
    [ 'key', 'up' ],
    [ 'key', 'up' ],
    [ 'key', 'up' ],
    [ 'wait' ],
    [ 'click', 'n' ],
    [ 'click', 'c' ],
    [ 'click', 'd' ],
    [ 'click', 'n' ],
    [ 'wait' ],
    [ 'key', 'down' ],
    [ 'wait' ],
    [ 'key', 'down' ],
    [ 'wait' ],
    [ 'click', 'a' ],
    [ 'key', 'up' ],
    [ 'menu', 'a' ],
    [ 'menu', 'n' ],
    [ 'menu', 'c' ],
    [ 'menu', 'd' ],
    [ 'choose', 'a', 'c - 1 files' ],
    [ 'choose', 'n', 'b - 2 files' ],
    [ 'choose', 'c', 'c - 1 files' ],
    [ 'choose', 'd', 'a - 2 files' ],
    [ 'choose', 'a', 'd' ],
]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW
    from qtpy import QtTest as QT

    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui import ClientGUICore
    from hydrus.client.gui.pages import ClientGUIPages
    from record_main_menu import tree

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    manifest = json.loads( ( HERE / 'fixtures/legacy_db/basic.manifest.json' ).read_text() )

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ][:4] ]

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    captured = []

    core = ClientGUICore.core()

    live = []

    def capture( window, menu ):

        captured.append( tree( menu ) )

        live.append( menu )

    core.PopupMenu = capture

    notebook = gui._notebook

    def setup():

        while notebook.count() > 0:

            notebook.widget( 0 ).deleteLater()
            notebook.removeTab( 0 )


        del notebook._closed_pages[:]

        notebook.NewPageQuery( location, initial_hashes = [ hashes[0], hashes[1] ], page_name = 'a' )

        inner = notebook.NewPagesNotebook( name = 'n', give_it_a_blank_page = False )

        inner.NewPageQuery( location, initial_hashes = [ hashes[1], hashes[2] ], page_name = 'b' )
        inner.NewPageQuery( location, initial_hashes = [ hashes[3] ], page_name = 'c' )

        inner.setCurrentIndex( 0 )

        notebook.NewPageQuery( location, page_name = 'd' )

        notebook.setCurrentIndex( 0 )

    qt( setup )

    time.sleep( 1 )

    def notebooks():

        out = []

        node = notebook

        while isinstance( node, ClientGUIPages.PagesNotebook ):

            out.append( node )

            node = node.currentWidget()

        return out


    def state():

        shown = notebook.GetCurrentMediaPage()

        return {
            'shown' : None if shown is None else shown.GetName(),
            'rows' : [ { 'tabs' : [ n.tabText( i ) for i in range( n.count() ) ], 'current' : n.currentIndex() } for n in notebooks() ],
        }


    def all_notebooks():

        return [ notebook ] + [ w for w in notebook.GetPages() if isinstance( w, ClientGUIPages.PagesNotebook ) ]


    def find_notebook_with_tab( name ):

        for n in notebooks() + all_notebooks():

            for i in range( n.count() ):

                if n.widget( i ).GetName() == name:

                    return ( n, i )


        raise RuntimeError( ( name, [ [ n.tabText( i ) for i in range( n.count() ) ] for n in all_notebooks() ] ) )


    steps = [ { 'state' : qt( state ) } ]

    for step in STEPS:

        kind = step[0]

        extra = {}

        def do():

            if kind == 'click':

                ( n, i ) = find_notebook_with_tab( step[1] )

                extra[ 'label' ] = n.tabText( i )

                bar = n.tabBar()

                QT.QTest.mouseClick( bar, QC.Qt.MouseButton.LeftButton, QC.Qt.KeyboardModifier.NoModifier, bar.tabRect( i ).center() )

            elif kind == 'key':

                gui.activateWindow()

                target = notebook.currentWidget()

                key = QC.Qt.Key.Key_PageDown if step[1] == 'down' else QC.Qt.Key.Key_PageUp

                QT.QTest.keyClick( target, key, QC.Qt.KeyboardModifier.ControlModifier )

            elif kind == 'wait':

                for n in all_notebooks():

                    n._time_of_last_move_selection_event -= 10

            elif kind == 'choose':

                ( n, i ) = find_notebook_with_tab( step[1] )

                extra[ 'label' ] = n.tabText( i )

                captured.clear()

                del live[:]

                n._ShowMenuForTabIndex( i )

                pages_action = next( a for a in live[0].actions() if a.text() == 'pages' )

                next( a for a in pages_action.menu().actions() if a.text() == step[2] ).trigger()

            elif kind == 'menu':

                ( n, i ) = find_notebook_with_tab( step[1] )

                extra[ 'label' ] = n.tabText( i )

                captured.clear()

                n._ShowMenuForTabIndex( i )

                pages = [ e for e in captured[0] if isinstance( e, dict ) and e.get( 'menu' ) == 'pages' ]

                extra[ 'pages' ] = pages[0][ 'entries' ] if pages else None

            QW.QApplication.processEvents()

        qt( do )

        time.sleep( 0.3 )

        record_step = { 'do' : step, 'state' : qt( state ) }

        record_step.update( extra )

        steps.append( record_step )


    layout = [
        { 'name' : 'a', 'hashes' : [ hashes[0].hex(), hashes[1].hex() ] },
        { 'name' : 'n', 'children' : [ { 'name' : 'b', 'hashes' : [ hashes[1].hex(), hashes[2].hex() ] }, { 'name' : 'c', 'hashes' : [ hashes[3].hex() ] } ] },
        { 'name' : 'd', 'hashes' : [] },
    ]

    return { 'layout' : layout, 'steps' : steps }


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        Path( sys.argv[ 2 ] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record ) ) )

        return


    with tempfile.TemporaryDirectory() as directory:

        path = Path( directory ) / 'result.json'

        hydrus_driver.run_in_subprocess( str( Path( __file__ ).resolve() ), '--child', str( path ) )

        result = json.loads( path.read_text() )


    ( HERE / 'fixtures' / 'tab_keys.json' ).write_text( json.dumps( result, indent = 2 ) + '\n' )

    print( 'wrote tab_keys.json' )


if __name__ == '__main__':

    main()
