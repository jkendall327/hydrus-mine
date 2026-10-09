#!/usr/bin/env python3
"""Record the reference's new page chooser (`DialogPageChooser`) as a tree.

In the running client, on the `basic` fixture, a chooser is made and its nine
buttons' texts read for the home screen and for each menu it leads to (file
search, download, special); then, for every button of every menu that makes
a page, a fresh chooser is made, the buttons hit (the menu's, then the
page's) and what it chose recorded: whether a page or a page of pages, and
for a page the kind (its page manager's type), its name, and (for a file
search) the file domain.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_page_chooser_tree.py
       (writes fixtures/page_chooser_tree.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'page_chooser_tree.json' )


def record( session ):

    from hydrus.client.gui.pages import ClientGUINewPageChooser

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def texts( dialog ):

        out = {}

        for n in range( 1, 10 ):

            button = getattr( dialog, '_button_{}'.format( n ) )

            out[ str( n ) ] = button.text() if button.isVisibleTo( dialog ) else ''


        return out


    def make():

        dialog = ClientGUINewPageChooser.DialogPageChooser( gui, controller )

        return dialog


    def tree():

        dialog = make()

        home = texts( dialog )

        menus = {}
        leaves = []

        for n in range( 1, 10 ):

            if home[ str( n ) ] == '':

                continue


            dialog = make()

            dialog._HitButton( n )

            menu = texts( dialog )

            menus[ home[ str( n ) ] ] = { 'button' : n, 'buttons' : menu }

            for m in range( 1, 10 ):

                if menu[ str( m ) ] == '':

                    continue


                dialog = make()

                dialog._HitButton( n )

                dialog._HitButton( m )

                result = dialog._result

                entry = { 'menu' : home[ str( n ) ], 'button' : m, 'text' : menu[ str( m ) ] }

                if result is None:

                    entry[ 'result' ] = None

                elif result[ 0 ] == 'pages':

                    entry[ 'result' ] = 'pages'

                else:

                    manager = result[ 1 ]

                    entry[ 'result' ] = 'page'
                    entry[ 'page_type' ] = manager.GetType()
                    entry[ 'name' ] = manager.GetPageName()

                    location_context = manager.GetLocationContext()

                    entry[ 'location' ] = sorted( controller.services_manager.GetName( k ) for k in location_context.current_service_keys )


                leaves.append( entry )


        return { 'home' : home, 'menus' : menus, 'leaves' : leaves }


    return qt( tree )


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
