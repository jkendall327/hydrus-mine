#!/usr/bin/env python3
"""Record the reference's thumbnail right-click menu.

In the running client, pages of the `basic` fixture's files are opened
(on "my files", and on "all local files", which shows the trash too), and
for several selections (none, one file in the inbox, one archived, one
in the trash, several, all) the grid's own `GetMenu` is built and its
tree recorded: each entry's text, separators as "---", and submenus with
their entries, once the share menu's other hashes have filled in; at a
fixed "now", for the selection's info lines.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_thumbnail_menu.py
       (writes fixtures/thumbnail_menu.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'thumbnail_menu.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )


def tree( menu ):

    out = []

    for action in menu.actions():

        if action.isSeparator():

            out.append( '---' )

        elif action.menu() is not None:

            out.append( { 'menu' : action.text(), 'entries' : tree( action.menu() ) } )

        else:

            out.append( action.text() )


    return out


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation

    from hydrus.core import HydrusTime

    controller = session.controller
    gui = controller.gui

    # (at a fixed now, for the info lines' times)
    now = int( time.time() )

    real = ( HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat )

    HydrusTime.GetNow = lambda: now
    HydrusTime.GetNowMS = lambda: now * 1000
    HydrusTime.GetNowFloat = lambda: float( now )

    try:

        return { 'now' : now, 'pages' : record_pages( controller, gui ) }

    finally:

        ( HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat ) = real



def record_pages( controller, gui ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def open_page( service_key, page_hashes ):

        location = ClientLocation.LocationContext.STATICCreateSimple( service_key )

        page = qt( lambda: gui._notebook.NewPageQuery( location, initial_hashes = page_hashes ) )

        for _ in range( 600 ):

            panel = qt( lambda: page.GetMediaResultsPanel() )

            if hasattr( panel, 'GetMenu' ) and len( panel._sorted_media ) > 0:

                return panel


            time.sleep( 0.05 )


        panel = qt( lambda: page.GetMediaResultsPanel() )

        raise Exception( 'the page never loaded' )


    pages = []

    media_results = controller.Read( 'media_results', hashes )

    inbox_hashes = [ m.GetHash() for m in media_results if m.GetInbox() ]
    my_files_only = [ m.GetHash() for m in media_results if set( m.GetLocationsManager().GetCurrent() ).intersection( { CC.LOCAL_FILE_SERVICE_KEY } ) and not CC.TRASH_SERVICE_KEY in m.GetLocationsManager().GetCurrent() and len( [ k for k in m.GetLocationsManager().GetCurrent() if controller.services_manager.GetServiceType( k ) == HC.LOCAL_FILE_DOMAIN ] ) == 1 ]

    for ( name, service_key, page_hashes ) in (
        ( 'my files', CC.LOCAL_FILE_SERVICE_KEY, hashes ),
        ( 'all local files', CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, hashes ),
        ( 'inbox files', CC.LOCAL_FILE_SERVICE_KEY, inbox_hashes ),
        ( 'my files only', CC.LOCAL_FILE_SERVICE_KEY, my_files_only ),
    ):

        panel = open_page( service_key, page_hashes )

        def sorted_media():

            return list( panel._sorted_media )


        media = qt( sorted_media )

        inbox = [ m for m in media if m.HasInbox() ]
        archived = [ m for m in media if not m.HasInbox() ]
        trashed = [ m for m in media if CC.TRASH_SERVICE_KEY in m.GetLocationsManager().GetCurrent() ]

        selections = [
            ( 'none', [] ),
            ( 'one in the inbox', inbox[ : 1 ] ),
            ( 'one archived', archived[ : 1 ] ),
            ( 'several', inbox[ : 2 ] + archived[ : 2 ] ),
            ( 'all', media ),
        ]

        if len( trashed ) > 0:

            selections.append( ( 'one in the trash', trashed[ : 1 ] ) )
            selections.append( ( 'trash and not', trashed[ : 1 ] + inbox[ : 1 ] ) )


        menus = []

        for ( label, selected ) in selections:

            def build():

                panel._Select( ClientMediaFileFilterNone() )

                for ( i, m ) in enumerate( selected ):

                    panel._HitMedia( m, i > 0, False )


                return panel.GetMenu()


            menu = qt( build )

            # (the share menu's md5, sha1 and sha512 fill in a moment later)
            time.sleep( 0.5 )

            menus.append( {
                'selection' : label,
                'selected' : [ m.GetHash().hex() for m in selected ],
                'menu' : qt( lambda: tree( menu ) ),
            } )


        pages.append( { 'page' : name, 'files' : [ m.GetHash().hex() for m in media ], 'menus' : menus } )


    return pages


def ClientMediaFileFilterNone():

    from hydrus.client.media import ClientMediaFileFilter

    return ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_NONE )


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
