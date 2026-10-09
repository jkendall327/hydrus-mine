#!/usr/bin/env python3
"""Record the reference's pages > history menu.

In the running client, on the `basic` fixture, five search pages ("one" to
"five") are opened, and a script of steps is run: show a page, choose an entry of the menu, close a
page, change "the number of entries shown" (`page_nav_history_max_entries`),
click "Clear History". After each step the entries of the history menu
(`page_nav_history_menu`, rebuilt by `_UpdateMenuPagesHistoryIfDirty`) are
recorded, as text with separators as "---", with whether the first is bold.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_page_history_menu.py
       (writes fixtures/page_history_menu.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'page_history_menu.json' )

NAMES = [ 'one', 'two', 'three', 'four', 'five' ]

STEPS = [
    [ 'show', 'one' ],
    [ 'show', 'two' ],
    [ 'show', 'three' ],
    [ 'show', 'one' ],
    [ 'show', 'five' ],
    [ 'show', 'three' ],
    [ 'choose', 3 ],
    [ 'choose', 1 ],
    [ 'choose', 4 ],
    [ 'max', 3 ],
    [ 'show', 'two' ],
    [ 'max', 20 ],
    [ 'close', 'three' ],
    [ 'close', 'two' ],
    [ 'show', 'four' ],
    [ 'clear' ],
    [ 'show', 'five' ],
    [ 'show', 'one' ],
]


def record( session ):

    from hydrus.client import ClientConstants as CC

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def entries():

        gui._pages_history_dirty = True

        gui._UpdateMenuPagesHistoryIfDirty()

        out = []

        for action in gui.page_nav_history_menu.actions():

            if action.isSeparator():

                out.append( '---' )

            else:

                out.append( { 'text' : action.text(), 'bold' : action.font().bold() } )


        return out


    def page_named( name ):

        for page in gui._notebook.GetPages():

            if page.GetName() == name:

                return page


        raise Exception( 'no page ' + name )


    def setup():

        notebook = gui._notebook

        while notebook.count() > 0:

            notebook.widget( 0 ).deleteLater()
            notebook.removeTab( 0 )


        location_context = controller.new_options.GetDefaultLocalLocationContext()

        for name in NAMES:

            notebook.NewPageQuery( location_context, page_name = name )


        gui.page_nav_history.CleanPages( {} )

        gui._pages_history_dirty = True


    qt( setup )

    time.sleep( 0.5 )

    # (the page that is up from the start has not "just changed")
    qt( lambda: gui.page_nav_history.CleanPages( {} ) )

    results = []

    for step in STEPS:

        def run():

            if step[ 0 ] == 'show':

                page = page_named( step[ 1 ] )

                gui._notebook.setCurrentWidget( page )

                gui.NotifyPageJustChanged()

            elif step[ 0 ] == 'close':

                page = page_named( step[ 1 ] )

                gui._notebook.setCurrentWidget( page )

                gui._notebook.CloseCurrentPage()

                gui.NotifyPageJustChanged()

            elif step[ 0 ] == 'choose':

                gui._pages_history_dirty = True

                gui._UpdateMenuPagesHistoryIfDirty()

                prefix = '{}: '.format( step[ 1 ] )

                for action in gui.page_nav_history_menu.actions():

                    if action.text().startswith( prefix ):

                        action.trigger()

                        break



            elif step[ 0 ] == 'max':

                controller.new_options.SetInteger( 'page_nav_history_max_entries', step[ 1 ] )

            elif step[ 0 ] == 'clear':

                gui._SetPagesHistoryEmpty()


        qt( run )

        time.sleep( 0.3 )

        results.append( { 'step' : step, 'entries' : qt( entries ), 'current' : qt( lambda: gui._notebook.GetCurrentMediaPage().GetName() ) } )


    return { 'names' : NAMES, 'steps' : results }


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
