#!/usr/bin/env python3
"""Record the reference's system tray rules from the running client.

The offscreen platform has no tray host, so `SystemTrayAvailable` is scripted
(the real `ClientSystemTrayIcon` and the real `ClientGUI` methods then run
unchanged). On the `basic` fixture this records:

- `icon`: whether `_UpdateSystemTrayIcon` makes an icon, by availability,
  "always show" and whether the client is hidden to the tray;
- `close`: what the window's close request does by availability and the
  "close to tray" option (hide to tray, or ask to exit);
- `minimise`: what minimising the main window does, by availability, the
  option and the two bugfix switches;
- `activation`: what a click on the icon does;
- `flip`: the menu's show/hide entry from each state;
- `file_menu`: whether File > "minimise to system tray" is shown;
- `menu`: the icon's context menu, its show/hide label and its tooltip;
- `start`: whether the client boots hidden to the tray (a boot per case).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_system_tray.py
       (writes fixtures/system_tray.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'system_tray.json' )

BOOL_OPTIONS = (
    'always_show_system_tray_icon',
    'close_client_to_system_tray',
    'minimise_client_to_system_tray',
    'start_client_in_system_tray',
    'minimise_client_to_system_tray_bugfix_deferred_state_set',
    'minimise_client_to_system_tray_bugfix_restore_after_show',
    'advanced_mode',
)


def patch_available( state ):

    from hydrus.client.gui import ClientGUISystemTray

    ClientGUISystemTray.SystemTrayAvailable = lambda: state[ 'available' ]


def wait( n = 6 ):

    from qtpy import QtWidgets as QW

    for i in range( n ):

        QW.QApplication.processEvents()

        time.sleep( 0.05 )



def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtGui as QG
    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusConstants as HC

    state = { 'available' : True }

    patch_available( state )

    options = controller.new_options

    result = {}

    def reset():

        # back to a shown window, no icon, every switch off

        state[ 'available' ] = True

        for name in BOOL_OPTIONS:

            options.SetBoolean( name, False )


        if gui._currently_hidden_to_system_tray:

            gui._SystemTrayShow()


        gui.showNormal()

        gui._UpdateSystemTrayIcon()

        wait()


    def snapshot():

        return {
            'hidden' : gui._currently_hidden_to_system_tray,
            'icon' : gui._have_system_tray_icon,
            'minimised' : gui.isMinimized(),
        }


    exits = []

    gui.TryToExit = lambda *args, **kwargs: exits.append( True )

    def work():

        # icon rules

        cases = []

        for available in ( False, True ):

            for always in ( False, True ):

                for hidden in ( False, True ):

                    reset()

                    state[ 'available' ] = available
                    options.SetBoolean( 'always_show_system_tray_icon', always )

                    gui._currently_hidden_to_system_tray = hidden

                    gui._UpdateSystemTrayIcon()

                    wait()

                    cases.append( {
                        'available' : available,
                        'always_show' : always,
                        'hidden' : hidden,
                        'icon' : gui._have_system_tray_icon,
                        'icon_visible' : bool( gui._have_system_tray_icon and gui._system_tray_icon.isVisible() ),
                    } )

                    gui._currently_hidden_to_system_tray = False

        result[ 'icon' ] = cases

        # close

        cases = []

        for available in ( False, True ):

            for close in ( False, True ):

                reset()

                state[ 'available' ] = available
                options.SetBoolean( 'close_client_to_system_tray', close )

                del exits[:]

                gui.closeEvent( QG.QCloseEvent() )

                wait()

                now = snapshot()

                now.update( { 'available' : available, 'close_to_tray' : close, 'exit_asked' : len( exits ) > 0 } )

                cases.append( now )

        result[ 'close' ] = cases

        # minimise

        cases = []

        for available in ( False, True ):

            for minimise in ( False, True ):

                for bugfix in ( 'none', 'deferred_state_set', 'restore_after_show' ):

                    reset()

                    state[ 'available' ] = available
                    options.SetBoolean( 'minimise_client_to_system_tray', minimise )
                    options.SetBoolean( 'minimise_client_to_system_tray_bugfix_deferred_state_set', bugfix == 'deferred_state_set' )
                    options.SetBoolean( 'minimise_client_to_system_tray_bugfix_restore_after_show', bugfix == 'restore_after_show' )

                    gui.showMaximized()

                    wait()

                    gui.showMinimized()

                    wait( 10 )

                    now = snapshot()

                    state_after_hide = int( gui.windowState().value ) if hasattr( gui.windowState(), 'value' ) else int( gui.windowState() )

                    now.update( { 'available' : available, 'minimise_to_tray' : minimise, 'bugfix' : bugfix, 'window_state' : state_after_hide } )

                    if now[ 'hidden' ]:

                        gui._SystemTrayShow()

                        wait( 10 )

                        shown_state = gui.windowState()

                        now[ 'shown_again_minimised' ] = gui.isMinimized()
                        now[ 'shown_again_maximised' ] = gui.isMaximized()

                    cases.append( now )

        result[ 'minimise' ] = cases

        # a click on the icon

        cases = []

        for hidden in ( False, True ):

            for active in ( False, True ):

                for minimise in ( False, True ):

                    reset()

                    options.SetBoolean( 'minimise_client_to_system_tray', minimise )
                    options.SetBoolean( 'always_show_system_tray_icon', True )

                    gui._UpdateSystemTrayIcon()

                    if hidden:

                        gui._SystemTrayHide()

                    wait()

                    gui.isActiveWindow = lambda active = active: active

                    gui._SystemTrayActivation()

                    wait( 10 )

                    del gui.isActiveWindow

                    now = snapshot()

                    now.update( { 'was_hidden' : hidden, 'active' : active, 'minimise_to_tray' : minimise } )

                    cases.append( now )

        result[ 'activation' ] = cases

        # the menu's show/hide entry

        cases = []

        for hidden in ( False, True ):

            reset()

            options.SetBoolean( 'always_show_system_tray_icon', True )

            gui._UpdateSystemTrayIcon()

            if hidden:

                gui._SystemTrayHide()

            wait()

            gui._SystemTrayFlipShowHide()

            wait()

            now = snapshot()

            now.update( { 'was_hidden' : hidden } )

            cases.append( now )

        result[ 'flip' ] = cases

        # File > minimise to system tray

        cases = []

        for available in ( False, True ):

            for windows in ( False, True ):

                for advanced in ( False, True ):

                    reset()

                    state[ 'available' ] = available
                    options.SetBoolean( 'advanced_mode', advanced )

                    real_windows = HC.PLATFORM_WINDOWS

                    HC.PLATFORM_WINDOWS = windows

                    try:

                        gui._menu_updater_file.update()

                        wait( 30 )

                        visible = gui._menubar_file_minimise_to_system_tray.isVisible()

                        # and the shortcut action, which gates the same way
                        gui.HideToSystemTray()

                        wait()

                        hidden_by_action = gui._currently_hidden_to_system_tray

                    finally:

                        HC.PLATFORM_WINDOWS = real_windows


                    cases.append( {
                        'available' : available,
                        'windows' : windows,
                        'advanced_mode' : advanced,
                        'menu_item_visible' : visible,
                        'menu_item_text' : gui._menubar_file_minimise_to_system_tray.text(),
                        'menu_item_tooltip' : gui._menubar_file_minimise_to_system_tray.toolTip(),
                        'hid_by_action' : hidden_by_action,
                    } )

        result[ 'file_menu' ] = cases

        # the icon's menu and tooltip

        reset()

        options.SetBoolean( 'pause_all_new_network_traffic', False )
        options.SetBoolean( 'pause_subs_sync', False )
        options.SetBoolean( 'always_show_system_tray_icon', True )

        gui._UpdateSystemTrayIcon()

        wait()

        def describe( menu ):

            out = []

            for action in menu.actions():

                if action.isSeparator():

                    out.append( { 'separator' : True } )

                    continue

                item = { 'text' : action.text(), 'tooltip' : action.toolTip(), 'checkable' : action.isCheckable() }

                if action.isCheckable():

                    item[ 'checked' ] = action.isChecked()

                if action.menu() is not None:

                    item[ 'submenu' ] = describe( action.menu() )

                out.append( item )

            return out


        icon = gui._system_tray_icon

        menu = { 'shown' : describe( icon.contextMenu() ) }

        gui._SystemTrayHide()

        wait()

        menu[ 'hidden' ] = describe( gui._system_tray_icon.contextMenu() )

        gui._SystemTrayShow()

        wait()

        tooltips = []

        for network in ( False, True ):

            for subs in ( False, True ):

                options.SetBoolean( 'pause_all_new_network_traffic', network )
                options.SetBoolean( 'pause_subs_sync', subs )
                options.SetString( 'app_display_name', 'my hydrus' )

                gui._UpdateSystemTrayIcon()

                gui._system_tray_icon.RegenOptionsCheckboxes()

                tooltips.append( { 'network_paused' : network, 'subscriptions_paused' : subs, 'tooltip' : gui._system_tray_icon.toolTip() } )

        options.SetBoolean( 'pause_all_new_network_traffic', False )
        options.SetBoolean( 'pause_subs_sync', False )
        options.SetString( 'app_display_name', 'hydrus client' )

        menu[ 'tooltips' ] = tooltips

        # the checkboxes follow the options

        options.SetBoolean( 'close_client_to_system_tray', True )

        gui._system_tray_icon.RegenOptionsCheckboxes()

        menu[ 'after_close_option_on' ] = describe( gui._system_tray_icon.contextMenu() )

        result[ 'menu' ] = menu

        reset()

    controller.CallBlockingToQt( gui, work )

    return result


def child_set( db_dir, start ):

    import hydrus_driver

    # the option is saved with the rest of the client options

    def set_option( session ):

        session.controller.new_options.SetBoolean( 'start_client_in_system_tray', start )

        session.controller.Write( 'serialisable', session.controller.new_options )

    hydrus_driver.run_client( db_dir, set_option )


def child_look( db_dir, available, start, out ):

    import hydrus_driver

    patch_available( { 'available' : available } )

    def look( session ):

        gui = session.controller.gui

        done = {}

        def work():

            wait()

            done.update( {
                'available' : available,
                'start_option' : start,
                'hidden' : gui._currently_hidden_to_system_tray,
                'window_visible' : gui.isVisible(),
                'icon' : gui._have_system_tray_icon,
            } )

        session.controller.CallBlockingToQt( gui, work )

        return done

    result = hydrus_driver.run_client( db_dir, look )

    with open( out, 'w' ) as f:

        json.dump( result, f )


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( out, 'w' ) as f:

        json.dump( result, f, ensure_ascii = False )


def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ] )

        return

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child-set':

        child_set( sys.argv[ 2 ], sys.argv[ 3 ] == '1' )

        return

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child-look':

        child_look( sys.argv[ 2 ], sys.argv[ 3 ] == '1', sys.argv[ 4 ] == '1', sys.argv[ 5 ] )

        return

    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'system_tray.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

        starts = []

        for available in ( '0', '1' ):

            for start in ( '0', '1' ):

                import record_api

                db_dir = record_api.unpack_fixture( 'basic' )

                path = os.path.join( work, f'start_{available}{start}.json' )

                hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child-set', db_dir, start )
                hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child-look', db_dir, available, start, path )

                with open( path ) as f:

                    starts.append( json.load( f ) )

        result[ 'start' ] = starts

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )

    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
