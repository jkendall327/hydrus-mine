#!/usr/bin/env python3
"""Record the reference's shared file domain button under its callers' flags.

In the running client, on the `basic` fixture (two local file domains,
"my files" and "art"), the file domain button (`LocationSearchContextButton`)
is used by the search page (everything), the import options container
editor's destination (`SetOnlyImportableDomainsAllowed`) and presentation
location (no restrictions, all known files allowed), and the Options file
search page (importable). For advanced mode off and on this records:

* `matrix`: for each flag combination the callers can use, the menu the
  button builds (each entry's text and whether it is checked, separators
  as "---"), and the "multiple/deleted locations" panel's check boxes
* `destination`: the real `EditLocationImportOptionsPanel`, for each kind
  of caller: whether the downloader's URL boxes show, and for a start
  value, then each menu entry chosen in turn, and a multi-domain choice
  from the panel: the button's label, whether the "THIS WILL NOT IMPORT
  ANYWHERE" warning shows, the value's destination names, and whether the
  "even for already in db files" box is enabled when auto-archive is
  ticked or not
* `presentation`: the real `EditPresentationImportOptions` panel's
  location button menu
* `presentation_gates`: that panel's status choices, and for each status
  the inbox choices, whether the inbox and location controls are enabled,
  and the value each inbox choice gives; and what an "or in inbox" choice
  becomes when the status moves off "new files"

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_location_selector_flags.py
       (writes fixtures/location_selector_flags.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'location_selector_flags.json' )


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
    from hydrus.client.gui.importing import ClientGUIImportOptionsPanels
    from hydrus.client.gui.search import ClientGUILocation
    from hydrus.client.importing.options import ImportOptionsConstants as IOC
    from hydrus.client.importing.options import LocationImportOptions, PresentationImportOptions

    controller = session.controller
    gui = controller.gui
    services = controller.services_manager

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    captured = []

    CGC.core().PopupMenu = lambda window, menu: captured.append( menu )

    def name( key ):

        return services.GetName( key )


    def menu_of( button ):

        captured.clear()

        button._EditLocation()

        return entries( captured[ -1 ] ), captured[ -1 ]


    def multiple_of( button ):

        panel = ClientGUILocation.EditMultipleLocationContextPanel( gui, button._location_context, button._IsAllKnownFilesServiceTypeAllowed(), button._only_importable_domains_allowed, button._only_local_file_domains_allowed, button._only_combined_local_file_domains_allowed )

        box = panel._location_list

        out = [ { 'text' : box.item( i ).text(), 'checked' : box.IsChecked( i ) } for i in range( box.count() ) ]

        panel.deleteLater()

        return out


    def my_files():

        return ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )


    # the flag combinations callers use
    presets = {
        'search' : dict( all_known = ( True, True ) ),
        'search_not_advanced_all_known' : dict( all_known = ( True, False ) ),
        'no_all_known' : dict( all_known = ( False, False ) ),
        'importable' : dict( importable = True ),
        'only_local' : dict( local = True ),
        'only_combined_local' : dict( combined = True ),
        'no_multiple' : dict( multiple = False ),
    }

    def make_button( flags ):

        button = ClientGUILocation.LocationSearchContextButton( gui, my_files() )

        if 'all_known' in flags:

            button.SetAllKnownFilesAllowed( *flags[ 'all_known' ] )


        button.SetOnlyImportableDomainsAllowed( flags.get( 'importable', False ) )
        button.SetOnlyLocalFileDomainsAllowed( flags.get( 'local', False ) )
        button.SetOnlyCombinedLocalFileDomainsAllowed( flags.get( 'combined', False ) )
        button.SetMultipleFileDomainsAllowed( flags.get( 'multiple', True ) )

        return button


    def matrix():

        out = {}

        for ( preset, flags ) in presets.items():

            button = make_button( flags )

            out[ preset ] = { 'menu' : menu_of( button )[ 0 ], 'multiple' : multiple_of( button ) }

            button.deleteLater()


        return out


    def destination_case( caller_type ):

        options = LocationImportOptions.LocationImportOptions()

        options.SetDestinationLocationContext( my_files() )

        panel = ClientGUIImportOptionsPanels.EditLocationImportOptionsPanel( gui, options, caller_type )

        panel.show()

        return panel


    def destination_state( panel ):

        value = panel.GetValue()
        context = value.GetDestinationLocationContext()

        return {
            'label' : panel._destination_location_context.text(),
            'warning' : panel._destination_location_context_st.isVisibleTo( panel ),
            'current' : sorted( name( k ) for k in context.current_service_keys ),
            'deleted' : sorted( name( k ) for k in context.deleted_service_keys ),
            'even_if_already' : panel._do_archive_on_already_in_db_files.isEnabled(),
            'auto_archive' : value.AutomaticallyArchives(),
        }


    def destination():

        out = { 'callers' : {} }

        callers = {
            'local import' : IOC.IMPORT_OPTIONS_CALLER_TYPE_LOCAL_IMPORT,
            'import folder' : IOC.IMPORT_OPTIONS_CALLER_TYPE_LOCAL_IMPORT_FOLDER,
            'client api' : IOC.IMPORT_OPTIONS_CALLER_TYPE_CLIENT_API,
            'gallery/post urls' : IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS,
            'watchable urls' : IOC.IMPORT_OPTIONS_CALLER_TYPE_WATCHER_URLS,
            'subscription' : IOC.IMPORT_OPTIONS_CALLER_TYPE_SUBSCRIPTION,
            'global' : IOC.IMPORT_OPTIONS_CALLER_TYPE_GLOBAL,
            'url class' : IOC.IMPORT_OPTIONS_CALLER_TYPE_URL_CLASS,
            'specific importer' : IOC.IMPORT_OPTIONS_CALLER_TYPE_SPECIFIC_IMPORTER,
            'favourites' : IOC.IMPORT_OPTIONS_CALLER_TYPE_FAVOURITES,
        }

        for ( label, caller_type ) in callers.items():

            panel = destination_case( caller_type )

            out[ 'callers' ][ label ] = {
                'primary_urls' : panel._associate_primary_urls.isVisibleTo( panel ),
                'source_urls' : panel._associate_source_urls.isVisibleTo( panel ),
            }

            panel.deleteLater()


        panel = destination_case( IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS )

        out[ 'start' ] = destination_state( panel )

        # each menu entry chosen in turn
        button = panel._destination_location_context

        menu_texts, menu = menu_of( button )

        out[ 'menu' ] = menu_texts

        chosen = []

        for text in [ a.text() for a in menu.actions() if not a.isSeparator() and a.text() != 'multiple/deleted locations' ]:

            panel.deleteLater()

            panel = destination_case( IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS )

            _, menu = menu_of( panel._destination_location_context )

            [ a for a in menu.actions() if a.text() == text ][ 0 ].trigger()

            chosen.append( dict( destination_state( panel ), chosen = text ) )


        out[ 'chosen' ] = chosen

        # the panel's own multiple choice: art and my files, and then none
        art = [ s.GetServiceKey() for s in services.GetServices( ( HC.LOCAL_FILE_DOMAIN, ) ) if s.GetServiceKey() != CC.LOCAL_FILE_SERVICE_KEY ][ 0 ]

        multiple = []

        for ( label, keys ) in [ ( 'my files and art', [ CC.LOCAL_FILE_SERVICE_KEY, art ] ), ( 'art', [ art ] ), ( 'nothing', [] ) ]:

            panel.deleteLater()

            panel = destination_case( IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS )

            panel._destination_location_context.SetValue( ClientLocation.LocationContext( current_service_keys = keys ) )

            multiple.append( dict( destination_state( panel ), chosen = label ) )


        out[ 'multiple' ] = multiple

        # the archive interlock
        interlock = []

        for ticked in ( False, True ):

            panel.deleteLater()

            panel = destination_case( IOC.IMPORT_OPTIONS_CALLER_TYPE_POST_URLS )

            panel._auto_archive.setChecked( ticked )
            panel._auto_archive.clicked.emit()

            interlock.append( dict( destination_state( panel ), ticked = ticked ) )


        out[ 'interlock' ] = interlock

        panel.deleteLater()

        return out


    def presentation():

        panel = ClientGUIImportOptionsPanels.EditPresentationImportOptions( gui, PresentationImportOptions.PresentationImportOptions() )

        button = panel._presentation_location

        texts, _ = menu_of( button )

        out = { 'label' : button.text(), 'menu' : texts, 'multiple' : multiple_of( button ) }

        panel.deleteLater()

        return out


    def presentation_gates():

        panel = ClientGUIImportOptionsPanels.EditPresentationImportOptions( gui, PresentationImportOptions.PresentationImportOptions() )

        status_box = panel._presentation_status
        inbox_box = panel._presentation_inbox

        out = { 'status_texts' : [ status_box.itemText( i ) for i in range( status_box.count() ) ], 'statuses' : [] }

        for i in range( status_box.count() ):

            status_box.setCurrentIndex( i )

            status = status_box.GetValue()

            entry = {
                'status' : status_box.itemText( i ),
                'inbox_texts' : [ inbox_box.itemText( j ) for j in range( inbox_box.count() ) ],
                'inbox_enabled' : inbox_box.isEnabled(),
                'location_enabled' : panel._presentation_location.isEnabled(),
                'values' : [],
            }

            for j in range( inbox_box.count() ):

                inbox_box.setCurrentIndex( j )

                value = panel.GetValue()

                entry[ 'values' ].append( { 'inbox' : inbox_box.itemText( j ), 'status' : value.GetPresentationStatus(), 'inbox_value' : value.GetPresentationInbox() } )

            out[ 'statuses' ].append( entry )

        # "or in inbox", then back to all files
        status_box.setCurrentIndex( 1 )
        inbox_box.setCurrentIndex( 2 )

        before = inbox_box.itemText( inbox_box.currentIndex() )

        status_box.setCurrentIndex( 0 )

        out[ 'or_in_inbox_then_all_files' ] = { 'before' : before, 'after' : inbox_box.itemText( inbox_box.currentIndex() ) }

        panel.deleteLater()

        return out

    out = {}

    out[ 'presentation_gates' ] = qt( presentation_gates )

    for advanced in ( False, True ):

        controller.new_options.SetBoolean( 'advanced_mode', advanced )

        out[ 'advanced' if advanced else 'normal' ] = {
            'matrix' : qt( matrix ),
            'destination' : qt( destination ),
            'presentation' : qt( presentation ),
        }


    controller.new_options.SetBoolean( 'advanced_mode', False )

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
