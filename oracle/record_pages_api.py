#!/usr/bin/env python3
"""Record what the Client API says of the live pages as the window changes.

In the running client, on the `basic` fixture, the main notebook is given a
search page "a" (two files), a notebook "nb" holding search pages "b" (three
files) and "c" (one file), and a search page "d" (none). Then steps, each a
change made in the window:

* `start`: as built;
* `show <name>`: that page made the one shown (its tabs selected);
* `select all` / `select none`: the shown page's files all selected, or none;
* `open <name>`: a new search page of that name opened (beside the one shown);
* `rename <name> <new>`: that page renamed through the tab menu's rename;
* `close <name>`: that page closed (`_ClosePage`).

After each step, `/manage_pages/get_pages` and, for each media page,
`/manage_pages/get_page_info?simple=false`, with the random page keys
replaced by the page names.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_pages_api.py
       (writes fixtures/pages_api.json)
"""

import json
import sys
import tempfile
import time
from pathlib import Path

HERE = Path( __file__ ).resolve().parent

sys.path.insert( 0, str( HERE ) )

PORT = 45878

STEPS = [
    [ 'start' ],
    [ 'show', 'c' ],
    [ 'select all' ],
    [ 'show', 'b' ],
    [ 'select all' ],
    [ 'select none' ],
    [ 'show', 'd' ],
    [ 'show', 'nb' ],
    [ 'show', 'a' ],
    [ 'select all' ],
    [ 'open', 'e' ],
    [ 'rename', 'd', 'dd' ],
    [ 'close', 'a' ],
    [ 'close', 'e' ],
]


def record( session ):

    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.pages import ClientGUIPages
    from hydrus.client.media import ClientMediaFileFilter

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    manifest = json.loads( ( HERE / 'fixtures/legacy_db/basic.manifest.json' ).read_text() )

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ][:4] ]

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    notebook = gui._notebook

    from hydrus.client.gui import ClientGUIDialogsQuick

    answers = []

    ClientGUIDialogsQuick.EnterText = lambda window, message, default = '', **kwargs: answers.pop( 0 )

    layout = [
        { 'name' : 'a', 'hashes' : [ hashes[0].hex(), hashes[1].hex() ] },
        { 'name' : 'nb', 'children' : [ { 'name' : 'b', 'hashes' : [ hashes[1].hex(), hashes[2].hex(), hashes[3].hex() ] }, { 'name' : 'c', 'hashes' : [ hashes[0].hex() ] } ] },
        { 'name' : 'd', 'hashes' : [] },
    ]

    def setup():

        while notebook.count() > 0:

            notebook.widget( 0 ).deleteLater()
            notebook.removeTab( 0 )


        del notebook._closed_pages[:]

        notebook.NewPageQuery( location, initial_hashes = [ hashes[0], hashes[1] ], page_name = 'a' )

        inner = notebook.NewPagesNotebook( name = 'nb', give_it_a_blank_page = False )

        inner.NewPageQuery( location, initial_hashes = [ hashes[1], hashes[2], hashes[3] ], page_name = 'b' )
        inner.NewPageQuery( location, initial_hashes = [ hashes[0] ], page_name = 'c' )

        inner.setCurrentIndex( 0 )

        notebook.NewPageQuery( location, page_name = 'd' )

        notebook.setCurrentIndex( 0 )

    qt( setup )

    time.sleep( 2 )

    def find( name ):

        def walk( node ):

            for i in range( node.count() ):

                widget = node.widget( i )

                if widget.GetName() == name:

                    return ( node, i, widget )


                if isinstance( widget, ClientGUIPages.PagesNotebook ):

                    found = walk( widget )

                    if found is not None:

                        return found


            return None

        return walk( notebook )


    def show( name ):

        # the notebooks from the top down to it, each selected
        ( node, i, widget ) = find( name )

        chain = []

        parent = node

        while parent is not notebook:

            chain.append( parent )

            parent = parent.parent().parent() if not isinstance( parent.parent(), ClientGUIPages.PagesNotebook ) else parent.parent()

        # (selecting from the top)
        for outer in notebook.GetPages():

            if isinstance( outer, ClientGUIPages.PagesNotebook ) and find_in( outer, name ):

                notebook.setCurrentWidget( outer )


        node.setCurrentIndex( i )

        QW.QApplication.processEvents()


    def find_in( outer, name ):

        for page in outer.GetPages():

            if page.GetName() == name:

                return True


        return False


    def select( everything ):

        page = notebook.GetCurrentMediaPage()

        panel = page.GetMediaResultsPanel()

        panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_ALL if everything else ClientMediaFileFilter.FILE_FILTER_NONE ) )

        QW.QApplication.processEvents()


    def names_for_keys( body ):

        mapping = {}

        def walk( page ):

            mapping[ page[ 'page_key' ] ] = page[ 'name' ]

            for child in page.get( 'pages', [] ):

                walk( child )

        walk( body[ 'pages' ] )

        return mapping


    def mask( value, mapping ):

        text = json.dumps( value )

        for key, name in mapping.items():

            text = text.replace( key, 'key of ' + name )

        return json.loads( text )


    def observe():

        pages = session.api.get( '/manage_pages/get_pages' )

        mapping = names_for_keys( pages )

        infos = {}

        for key, name in mapping.items():

            if name == 'top page notebook':

                continue

            infos[ name ] = session.api.get( '/manage_pages/get_page_info', page_key = key, simple = 'false' )

        return { 'pages' : mask( pages, mapping ), 'infos' : mask( infos, mapping ) }


    steps = []

    for step in STEPS:

        if step[0] == 'show':

            qt( lambda: show( step[1] ) )

        elif step[0] == 'open':

            qt( lambda: notebook.NewPageQuery( location, page_name = step[1] ) )

        elif step[0] == 'rename':

            def rename():

                ( node, i, widget ) = find( step[1] )

                answers.append( step[2] )

                node._RenamePage( i )

            qt( rename )

        elif step[0] == 'close':

            def close():

                ( node, i, widget ) = find( step[1] )

                node._ClosePage( i )

            qt( close )

        elif step[0] == 'select all':

            qt( lambda: select( True ) )

        elif step[0] == 'select none':

            qt( lambda: select( False ) )


        time.sleep( 0.5 )

        steps.append( { 'do' : step, 'api' : observe() } )


    return { 'layout' : layout, 'steps' : steps }


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        Path( sys.argv[ 2 ] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record, port = PORT ) ) )

        return


    with tempfile.TemporaryDirectory() as directory:

        path = Path( directory ) / 'result.json'

        hydrus_driver.run_in_subprocess( str( Path( __file__ ).resolve() ), '--child', str( path ) )

        result = json.loads( path.read_text() )


    ( HERE / 'fixtures' / 'pages_api.json' ).write_text( json.dumps( result, indent = 2 ) + '\n' )

    print( 'wrote pages_api.json' )


if __name__ == '__main__':

    main()
