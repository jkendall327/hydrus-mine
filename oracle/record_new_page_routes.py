#!/usr/bin/env python3
"""Record where the reference's Ctrl+T and empty-space double click put a new page.

In the running client, on the `basic` fixture, the main notebook is set up
as "first" (a search page), "second" (a page of pages holding the search
pages "a", "b" and "c") and "third" (a search page), and the new page
chooser is made to choose "page of pages" (or be cancelled). For each of the
four "new page goes" settings, with the current tab being the page of pages
(its "b" tab current, so the new page has a deepest notebook to go in) and
with the current tab being "first" (no deeper notebook), these are done:

- Ctrl+T (`SIMPLE_NEW_PAGE`: `ChooseNewPageForDeepestNotebook`),
- a double click in the empty space of the top row of tabs, and of the row
  of the page of pages (`EventNewPageFromScreenPosition`, which chooses the
  page in the notebook of the row clicked: recorded by choosing in that
  notebook, as the click would).

The notebook tree after each (names, pages of pages inside) and which tabs
are current are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_new_page_routes.py
       (writes fixtures/new_page_routes.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'new_page_routes.json' )


def record( session ):

    from qtpy import QtWidgets as QW

    from hydrus.client import ClientApplicationCommand as CAC
    from hydrus.client.gui.pages import ClientGUINewPageChooser
    from hydrus.client.gui.pages import ClientGUIPages

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def tree( notebook ):

        out = []

        for page in notebook.GetPages():

            if isinstance( page, ClientGUIPages.PagesNotebook ):

                out.append( { 'name' : page.GetName(), 'pages' : tree( page ) } )

            else:

                out.append( { 'name' : page.GetName() } )


        return out


    def current_path():

        path = []

        notebook = gui._notebook

        while isinstance( notebook, ClientGUIPages.PagesNotebook ):

            path.append( notebook.currentIndex() )

            notebook = notebook.currentWidget()


        return path


    real_chooser = ClientGUINewPageChooser.DialogPageChooser

    class Chosen:

        cancel = False

        def __init__( self, *args ): pass
        def __enter__( self ): return self
        def __exit__( self, *args ): pass
        def exec( self ): return QW.QDialog.DialogCode.Rejected if Chosen.cancel else QW.QDialog.DialogCode.Accepted
        def GetValue( self ): return ( 'pages', None )


    ClientGUINewPageChooser.DialogPageChooser = Chosen

    def setup( inside ):

        notebook = gui._notebook

        while notebook.count() > 0:

            notebook.widget( 0 ).deleteLater()
            notebook.removeTab( 0 )


        location_context = controller.new_options.GetDefaultLocalLocationContext()

        notebook.NewPageQuery( location_context, page_name = 'first' )

        second = notebook.NewPagesNotebook( name = 'second', give_it_a_blank_page = False )

        for name in ( 'a', 'b', 'c' ):

            second.NewPageQuery( location_context, page_name = name )


        second.setCurrentIndex( 1 )

        notebook.NewPageQuery( location_context, page_name = 'third' )

        notebook.setCurrentIndex( 1 if inside else 0 )


    cases = []

    for mode in range( 4 ):

        controller.new_options.SetInteger( 'default_new_page_goes', mode )

        for inside in ( True, False ):

            for ( how, cancel ) in (
                ( 'ctrl+t', False ),
                ( 'ctrl+t', True ),
                ( 'double click, top row', False ),
                ( 'double click, top row', True ),
                ( 'double click, second row', False ),
            ):

                if how == 'double click, second row' and not inside:

                    continue


                Chosen.cancel = cancel

                qt( lambda: setup( inside ) )

                time.sleep( 0.2 )

                before = qt( lambda: tree( gui._notebook ) )
                before_current = qt( current_path )

                def act():

                    if how == 'ctrl+t':

                        gui.ProcessApplicationCommand( CAC.ApplicationCommand.STATICCreateSimpleCommand( CAC.SIMPLE_NEW_PAGE ) )

                    elif how == 'double click, top row':

                        gui._notebook._ChooseNewPage()

                    else:

                        gui._notebook.currentWidget()._ChooseNewPage()


                qt( act )

                time.sleep( 0.3 )

                cases.append( {
                    'mode' : mode,
                    'inside' : inside,
                    'how' : how,
                    'cancel' : cancel,
                    'before' : before,
                    'before_current' : before_current,
                    'after' : qt( lambda: tree( gui._notebook ) ),
                    'current' : qt( current_path ),
                } )


    ClientGUINewPageChooser.DialogPageChooser = real_chooser

    return { 'cases' : cases }


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
