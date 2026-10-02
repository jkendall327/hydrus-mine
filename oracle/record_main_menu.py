#!/usr/bin/env python3
"""Record the reference's main window menu bar.

In the running client the menu bar's menus (`FrameGUI._InitialiseMenubar`,
filled by its updaters) are recorded as they show: each entry's text (with
Qt's `&` mnemonics and `&&` escapes), separators as "---" (collapsed as Qt
shows them: none first, last, or after another), submenus with their
entries, checkable entries as `{"check": text, "checked": bool}`, and
disabled entries as `{"disabled": text}`; a disabled submenu or menu has
`"disabled": true`.

On the `basic` fixture: with the client's default options ("default");
with its one page closed, which fills the undo menu ("closed"); and in
advanced mode, which adds entries ("advanced"). On the `import_folder` and
`export_folder` fixtures, the file menu, which lists their folders; on the
`repositories` fixture, the whole bar, with its repositories' entries. Each
recording has the facts it was made with that hydrus-rs can't read from
the fixture (the driver boots the client with network traffic paused, as
it is in "all new network traffic").

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_main_menu.py
       (writes fixtures/main_menu.json)
"""

import json
import os
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'main_menu.json' )

# how long the menus' updaters are given to fill them
SETTLE = 5


def tree( menu ):

    out = []

    for action in menu.actions():

        if not action.isVisible():

            continue

        if action.isSeparator():

            if len( out ) > 0 and out[ -1 ] != '---':

                out.append( '---' )


        elif action.menu() is not None:

            entry = { 'menu' : action.text(), 'entries' : tree( action.menu() ) }

            if not action.isEnabled():

                entry[ 'disabled' ] = True


            out.append( entry )

        elif action.isCheckable():

            out.append( { 'check' : action.text(), 'checked' : action.isChecked() } )

        elif not action.isEnabled():

            out.append( { 'disabled' : action.text() } )

        else:

            out.append( action.text() )



    while len( out ) > 0 and out[ -1 ] == '---':

        out.pop()


    return out


def menubar( controller, gui, page_shown = True ):

    def f():

        if page_shown:

            # (as a page being shown does, which at boot may come after the
            # menus are first filled, or before)
            gui.NotifyPageJustChanged()

            # (whose menu update is queued, so it comes now)
            gui._SetPagesHistoryDirty()


        # (the pages menu's counts and history are filled as it is opened)
        gui._pages_menu.aboutToShow.emit()

        return tree( gui._menubar )


    return controller.CallBlockingToQt( gui, f )


def record_basic( session ):

    controller = session.controller
    gui = controller.gui

    time.sleep( SETTLE )

    result = { 'default' : menubar( controller, gui ) }

    def close():

        page = gui._notebook.GetCurrentMediaPage()

        name = page.GetNameForMenu()

        gui._notebook.CloseCurrentPage( polite = False )

        return name


    closed_name = controller.CallBlockingToQt( gui, close )

    time.sleep( SETTLE )

    result[ 'closed' ] = { 'page' : closed_name, 'menus' : menubar( controller, gui, page_shown = False ) }

    def advanced():

        controller.new_options.SetBoolean( 'advanced_mode', True )

        gui._InitialiseMenubar()


    controller.CallBlockingToQt( gui, advanced )

    time.sleep( SETTLE )

    result[ 'advanced' ] = menubar( controller, gui )

    return result


def record_folders( session ):

    controller = session.controller
    gui = controller.gui

    time.sleep( SETTLE )

    return [ menu for menu in menubar( controller, gui ) if menu[ 'menu' ] == '&file' ][ 0 ]


def record_repositories( session ):

    controller = session.controller
    gui = controller.gui

    time.sleep( SETTLE )

    return menubar( controller, gui )


def child( fixture, out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( fixture )

    hook = { 'basic' : record_basic, 'repositories' : record_repositories }.get( fixture, record_folders )

    result = hydrus_driver.run_client( db_dir, hook )

    with open( out, 'w' ) as f:

        json.dump( result, f, ensure_ascii = False )



def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ], sys.argv[ 3 ] )

        return


    import hydrus_driver

    result = { 'facts' : { 'pause_all_new_network_traffic' : True } }

    with tempfile.TemporaryDirectory() as work:

        for fixture in ( 'basic', 'import_folder', 'export_folder', 'repositories' ):

            path = os.path.join( work, fixture + '.json' )

            hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', fixture, path )

            with open( path ) as f:

                recorded = json.load( f )


            if fixture == 'basic':

                result.update( recorded )

            elif fixture == 'repositories':

                result[ 'repositories' ] = recorded

            else:

                result[ fixture + '_file_menu' ] = recorded




    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
