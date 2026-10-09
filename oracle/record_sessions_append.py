#!/usr/bin/env python3
"""Record the reference's pages > sessions > append.

In the running client, on the `basic` fixture, a session "work" is saved
with the pages "a" and a page of pages "nested" holding "b" and "c". For each
of the four "new page goes" settings, with the open pages being a search
page "start" and a page of pages "outer" (holding "inner"), current being
"start" and, again, "inner", the top notebook's `AppendGUISessionFreshest`
(what the menu calls) is run, twice in a row. After each, the whole tree of
pages (each page's name, a page of pages' pages) and which tabs are current
are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_sessions_append.py
       (writes fixtures/sessions_append.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'sessions_append.json' )


def record( session ):

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


    location_context = controller.new_options.GetDefaultLocalLocationContext()

    def save_session():

        seed = ClientGUIPages.PagesNotebook( gui, 'seed' )

        seed.NewPageQuery( location_context, page_name = 'a' )

        nested = seed.NewPagesNotebook( name = 'nested', give_it_a_blank_page = False )

        nested.NewPageQuery( location_context, page_name = 'b' )
        nested.NewPageQuery( location_context, page_name = 'c' )

        controller.SaveGUISession( seed.GetCurrentGUISession( 'work', False, True ) )


    qt( save_session )

    time.sleep( 1.0 )

    def setup( inside ):

        notebook = gui._notebook

        while notebook.count() > 0:

            notebook.widget( 0 ).deleteLater()
            notebook.removeTab( 0 )


        notebook.NewPageQuery( location_context, page_name = 'start' )

        outer = notebook.NewPagesNotebook( name = 'outer', give_it_a_blank_page = False )

        outer.NewPageQuery( location_context, page_name = 'inner' )

        notebook.setCurrentIndex( 1 if inside else 0 )

    cases = []

    for mode in range( 4 ):

        controller.new_options.SetInteger( 'default_new_page_goes', mode )

        for inside in ( False, True ):

            qt( lambda: setup( inside ) )

            time.sleep( 0.2 )

            before = qt( lambda: tree( gui._notebook ) )
            before_current = qt( current_path )

            steps = []

            for _ in range( 2 ):

                qt( lambda: gui._notebook.AppendGUISessionFreshest( 'work' ) )

                time.sleep( 1.0 )

                steps.append( { 'tree' : qt( lambda: tree( gui._notebook ) ), 'current' : qt( current_path ) } )


            cases.append( { 'mode' : mode, 'inside' : inside, 'before' : before, 'before_current' : before_current, 'steps' : steps } )


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
