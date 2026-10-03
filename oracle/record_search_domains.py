#!/usr/bin/env python3
"""Record the reference search page's file and tag domain buttons.

In the running client, on the `basic` fixture (two local file domains, two
local tag services), a search page's autocomplete has a file domain button
("my files") and a tag domain button ("all known tags"), each opening a
menu of the domains to search. For advanced mode off and on, this records:

* `location_menu`, `tag_menu`: each menu as the button builds it, from a
  page on "my files" and "all known tags": each entry's text and whether
  it is checked, separators as "---"
* `location_choices`, `tag_choices`: each entry chosen in turn from that
  start, and both buttons' labels after (choosing "all known files with
  tags" with "all known tags" moves the tags to the first local tag
  service; choosing "all known tags" on "all known files" moves the files
  to the default local domain)
* `multiple`: the "multiple/deleted locations" panel's checkbox list,
  opened on "my files"

and, whatever the mode, `labels`: the file domain button's label for some
mixed domains, and `surplus`: what the panel keeps of some checked sets
(checking a domain that covers others unchecks those); and
`include_buttons`: the include current/pending tags buttons' labels, on
and off.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_search_domains.py
       (writes fixtures/search_domains.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'search_domains.json' )


def entries( menu ):

    out = []

    for action in menu.actions():

        if action.isSeparator():

            out.append( '---' )

        else:

            out.append( { 'text' : action.text(), 'checked' : action.isChecked() } )


    return out


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUICore as CGC
    from hydrus.client.gui.search import ClientGUILocation
    from hydrus.client.search import ClientSearchTagContext

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    # menus are captured rather than shown
    captured = []

    core = CGC.core()
    core.PopupMenu = lambda window, menu: captured.append( menu )

    def my_files():

        return ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )


    def open_page():

        page = qt( lambda: gui._notebook.NewPageQuery( my_files() ) )

        for _ in range( 600 ):

            sidebar = qt( lambda: page.GetSidebar() )

            if sidebar is not None and hasattr( sidebar, '_tag_autocomplete' ):

                return sidebar._tag_autocomplete


            time.sleep( 0.05 )


        raise Exception( 'the page never loaded' )


    def labels( ac ):

        return {
            'location' : ac._location_context_button.text(),
            'tags' : ac._tag_context_button.text(),
        }


    def reset( ac ):

        ac._SetLocationContext( my_files() )
        ac._SetTagContext( ClientSearchTagContext.TagContext() )


    def menu_of( ac, button ):

        return qt( lambda: menu_of_now( ac, button ) )


    def choices( ac, button, skip = () ):

        def run():

            reset( ac )

            menu = menu_of_now( ac, button )

            texts = [ a.text() for a in menu.actions() if not a.isSeparator() ]

            out = []

            for text in texts:

                if text in skip:

                    continue


                reset( ac )

                menu = menu_of_now( ac, button )

                action = [ a for a in menu.actions() if a.text() == text ][0]

                action.trigger()

                out.append( dict( labels( ac ), chosen = text ) )


            return out


        return qt( run )


    def menu_of_now( ac, button ):

        captured.clear()

        button._EditLocation() if button is ac._location_context_button else button._Edit()

        return captured[-1]


    def multiple( ac ):

        def run():

            reset( ac )

            panel = ClientGUILocation.EditMultipleLocationContextPanel( gui, my_files(), ac._location_context_button._IsAllKnownFilesServiceTypeAllowed(), False, False, False )

            box = panel._location_list

            out = [ { 'text' : box.item( i ).text(), 'checked' : box.IsChecked( i ) } for i in range( box.count() ) ]

            panel.deleteLater()

            return out


        return qt( run )


    services = controller.services_manager

    out = {}

    for advanced in ( False, True ):

        controller.new_options.SetBoolean( 'advanced_mode', advanced )

        ac = open_page()

        qt( lambda: reset( ac ) )

        mode = {
            'start' : qt( lambda: labels( ac ) ),
            'location_menu' : entries( menu_of( ac, ac._location_context_button ) ),
            'tag_menu' : entries( menu_of( ac, ac._tag_context_button ) ),
            'location_choices' : choices( ac, ac._location_context_button, skip = ( 'multiple/deleted locations', ) ),
            'tag_choices' : choices( ac, ac._tag_context_button ),
            'multiple' : multiple( ac ),
        }

        # all known tags chosen on all known files
        if advanced:

            def all_known_then_tags():

                reset( ac )

                ac._location_context_button.SetValue( ClientLocation.LocationContext.STATICCreateSimple( CC.COMBINED_FILE_SERVICE_KEY ) )

                after_files = labels( ac )

                menu = menu_of_now( ac, ac._tag_context_button )

                [ a for a in menu.actions() if a.text() == 'all known tags' ][0].trigger()

                return { 'after_files' : after_files, 'after_tags' : labels( ac ) }


            mode[ 'all_known_files_then_all_known_tags' ] = qt( all_known_then_tags )


        out[ 'advanced' if advanced else 'normal' ] = mode


    controller.new_options.SetBoolean( 'advanced_mode', False )

    # labels for mixed domains
    keys = {
        'my files' : CC.LOCAL_FILE_SERVICE_KEY,
        'trash' : CC.TRASH_SERVICE_KEY,
        'all my files' : CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY,
        'all local files' : CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY,
        'all known files' : CC.COMBINED_FILE_SERVICE_KEY,
    }

    art = [ s.GetServiceKey() for s in services.GetServices( ( HC.LOCAL_FILE_DOMAIN, ) ) if s.GetServiceKey() != CC.LOCAL_FILE_SERVICE_KEY ]

    keys[ 'art' ] = art[ 0 ]

    def context( current, deleted = () ):

        return ClientLocation.LocationContext( current_service_keys = [ keys[ n ] for n in current ], deleted_service_keys = [ keys[ n ] for n in deleted ] )


    mixes = [
        ( [ 'my files', 'trash' ], [] ),
        ( [ 'my files', 'art', 'trash' ], [] ),
        ( [], [ 'my files' ] ),
        ( [ 'my files' ], [ 'my files' ] ),
        ( [ 'my files' ], [ 'trash' ] ),
        ( [ 'all my files' ], [ 'all my files' ] ),
        ( [], [] ),
    ]

    out[ 'labels' ] = [ { 'current' : c, 'deleted' : d, 'label' : context( c, d ).ToString( services.GetName ) } for ( c, d ) in mixes ]

    surplus_cases = [
        ( [ 'my files', 'all known files' ], [] ),
        ( [ 'my files', 'art', 'all my files' ], [] ),
        ( [ 'my files', 'trash', 'all local files' ], [] ),
        ( [ 'my files', 'trash' ], [] ),
        ( [ 'all my files', 'trash' ], [] ),
    ]

    def surplus( c, d ):

        location_context = context( c, d )

        location_context.ClearSurplusLocalFilesServices( services )

        name = services.GetName

        return { 'current' : sorted( name( k ) for k in location_context.current_service_keys ), 'deleted' : sorted( name( k ) for k in location_context.deleted_service_keys ) }


    out[ 'surplus' ] = [ { 'checked' : c, 'deleted' : d, 'kept' : surplus( c, d ) } for ( c, d ) in surplus_cases ]

    # the include buttons
    ac = open_page()

    def include_buttons():

        reset( ac )

        start = ( ac._include_current_tags.text(), ac._include_pending_tags.text() )

        ac._include_current_tags.Flip()
        ac._include_pending_tags.Flip()

        flipped = ( ac._include_current_tags.text(), ac._include_pending_tags.text() )

        tag_context = ac._tag_context_button.GetValue()

        result = {
            'start' : start,
            'flipped' : flipped,
            'tag_label_flipped' : ac._tag_context_button.text(),
            'include_current' : tag_context.include_current_tags,
            'include_pending' : tag_context.include_pending_tags,
        }

        ac._include_current_tags.Flip()
        ac._include_pending_tags.Flip()

        return result


    out[ 'include_buttons' ] = qt( include_buttons )

    return out


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
