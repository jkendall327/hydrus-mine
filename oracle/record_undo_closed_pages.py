#!/usr/bin/env python3
"""Record the reference's undo > closed pages menu.

In the running client, on the `basic` fixture, five search pages ("one" to
"five") are opened and a script of steps is run: close a page (the page is
made current and `CloseCurrentPage` closes it), restore the most recent
closed page (`_UnclosePage`), restore the page an entry of the closed pages
menu stands for, and "clear all…" (the yes/no question is answered by the
script, and its text recorded). After each step the entries of the closed
pages submenu (separators as "---", whether the submenu is visible), the
tabs in order and which is current are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_undo_closed_pages.py
       (writes fixtures/undo_closed_pages.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'undo_closed_pages.json' )

NAMES = [ 'one', 'two', 'three', 'four', 'five' ]

STEPS = [
    [ 'close', 'three' ],
    [ 'close', 'five' ],
    [ 'close', 'one' ],
    [ 'entry', 2 ],
    [ 'undo' ],
    [ 'close', 'four' ],
    [ 'close', 'two' ],
    [ 'entry', 1 ],
    [ 'clear', False ],
    [ 'clear', True ],
    [ 'undo' ],
]


def record( session ):

    from qtpy import QtWidgets as QW

    from hydrus.client.gui import ClientGUIDialogsQuick

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    questions = []

    answer = { 'yes' : True }

    def get_yes_no( parent, message, *args, **kwargs ):

        questions.append( message )

        return QW.QDialog.DialogCode.Accepted if answer[ 'yes' ] else QW.QDialog.DialogCode.Rejected


    real_get_yes_no = ClientGUIDialogsQuick.GetYesNo

    ClientGUIDialogsQuick.GetYesNo = get_yes_no

    def closed_menu():

        gui._menu_updater_undo.update()

        return None


    def entries():

        out = []

        for action in gui._menubar_undo_closed_pages_submenu.actions():

            out.append( '---' if action.isSeparator() else action.text() )


        return out


    def tabs():

        return [ p.GetName() for p in gui._notebook.GetPages() ]


    def current():

        page = gui._notebook.GetCurrentMediaPage()

        return None if page is None else page.GetName()


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


        del gui._closed_pages[:]

        location_context = controller.new_options.GetDefaultLocalLocationContext()

        for name in NAMES:

            notebook.NewPageQuery( location_context, page_name = name )


    qt( setup )

    time.sleep( 0.5 )

    results = []

    for step in STEPS:

        del questions[:]

        def run():

            if step[ 0 ] == 'close':

                gui._notebook.setCurrentWidget( page_named( step[ 1 ] ) )

                gui._notebook.CloseCurrentPage()

            elif step[ 0 ] == 'undo':

                gui._UnclosePage()

            elif step[ 0 ] == 'entry':

                # (the menu lists the most recently closed first: entry 1 is the top one)
                gui._UnclosePage( len( gui._closed_pages ) - step[ 1 ] )

            elif step[ 0 ] == 'clear':

                answer[ 'yes' ] = step[ 1 ]

                gui.AskToDeleteAllClosedPages()


        qt( run )

        qt( closed_menu )

        # (the menu fills in a moment later)
        time.sleep( 1.0 )

        results.append( {
            'step' : step,
            'questions' : list( questions ),
            'closed' : qt( entries ),
            'tabs' : qt( tabs ),
            'current' : qt( current ),
            'closed_visible' : qt( lambda: gui._menubar_undo_closed_pages_submenu.menuAction().isVisible() ),
            'closed_enabled' : qt( lambda: gui._menubar_undo_closed_pages_submenu.isEnabled() ),
            'undo_menu_enabled' : qt( lambda: gui._menubar_undo_submenu.menuAction().isEnabled() ),
            'closed_count' : qt( lambda: len( gui._closed_pages ) ),
        } )


    ClientGUIDialogsQuick.GetYesNo = real_get_yes_no

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
