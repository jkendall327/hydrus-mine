#!/usr/bin/env python3
"""Record the reference's edit import folder and edit export folder dialogs.

On the folders `record_folders_lists.py` makes (`IMPORT_FOLDERS`,
`EXPORT_FOLDERS`), and on a new one each as the lists' "add" makes it,
this records:

* `import_opened`: `EditImportFolderPanel`'s fields as it opens: name,
  path, search subdirectories, paused, check regularly, check period (and
  whether it can be edited), recent modified time skip period, check on
  manage dialog ok, the popup and page publication checkboxes, each file
  outcome's action (the choices' labels, the one chosen) and move location
  (and whether it can be edited), the filename tagging rows, and the
  import options button's label;
* `import_applied`: "apply" on edited copies (`IMPORT_EDITS`): the error
  that refuses it, or the warnings it shows and what it gives back;
* `export_opened`: `EditExportFolderPanel`'s fields as it opens: name,
  path, type, delete from the client (and whether it can be ticked),
  symlinks, run regularly, period and the popup checkbox (and whether they
  can be edited), run on dialog ok, the query's predicates (as written),
  the phrase, and the sidecar overwrite checkboxes;
* `export_synchronise`: the delete checkbox once the type is set to
  synchronise;
* `export_applied`: "apply" on edited copies (`EXPORT_EDITS`): the
  question asked first (delete from the client), the error that refuses
  it, or what it gives back.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_folders_dialogs.py
       (writes fixtures/folders_dialogs.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import record_folders_lists as lists

OUT = os.path.join( HERE, 'fixtures', 'folders_dialogs.json' )

MISSING = '/nonexistent/hydrus-rs-oracle'
EXISTING = '/usr'

# ( which folder, edits ): fields to set before "apply"; actions are
# 'delete', 'ignore' or 'move' with a location
IMPORT_EDITS = [
    ( 'inbox drop', { 'path' : '' } ),
    ( 'inbox drop', { 'path' : EXISTING } ),
    ( 'inbox drop', { 'path' : MISSING } ),
    ( 'inbox drop', { 'path' : EXISTING, 'actions' : { 'successful' : ( 'move', '' ) } } ),
    ( 'inbox drop', { 'path' : EXISTING, 'actions' : { 'redundant' : ( 'move', MISSING ), 'failed' : ( 'delete', None ) } } ),
    ( 'manual only', { 'path' : EXISTING, 'name' : 'renamed', 'paused' : True, 'check_regularly' : True, 'period' : 2 * lists.HOUR, 'check_now' : True, 'search_subdirectories' : False, 'skip' : 120, 'popup' : False, 'popup_button' : False, 'page' : True } ),
]

EXPORT_EDITS = [
    ( 'favourites', { 'path' : '' } ),
    ( 'favourites', { 'phrase' : '{hash' } ),
    ( 'favourites', { 'phrase' : '[series' } ),
    ( 'favourites', { 'delete' : True, 'answer' : False } ),
    ( 'favourites', { 'delete' : True, 'answer' : True } ),
    ( 'Mirror', { 'name' : 'mirror 2', 'run_regularly' : False, 'run_now' : False, 'period' : 3 * lists.DAY, 'phrase' : '{hash} [series]', 'symlinks' : True, 'popup' : False, 'overwrite_next' : True, 'overwrite_always' : True } ),
]

ACTIONS = { 'successful' : 1, 'redundant' : 2, 'deleted' : 3, 'failed' : 4 }


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.core import HydrusConstants as HC
        from hydrus.core import HydrusExceptions

        from hydrus.client import ClientConstants as CC
        from hydrus.client import ClientLocation
        from hydrus.client.exporting import ClientExportingFiles
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui.exporting import ClientGUIExport
        from hydrus.client.gui.importing import ClientGUIImportFolders
        from hydrus.client.importing import ClientImportLocal
        from hydrus.client.networking.api import ClientLocalServerCore
        from hydrus.client.search import ClientSearchFileSearchContext

        asked = []
        answers = []

        def show( kind ):

            def shower( win, message, **kwargs ):

                asked.append( { 'kind' : kind, 'message' : message } )


            return shower


        def get_yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', **kwargs ):

            answer = answers.pop( 0 )

            asked.append( { 'kind' : 'yes/no', 'title' : title, 'message' : message, 'answer' : answer } )

            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


        ClientGUIDialogsMessage.ShowWarning = show( 'warning' )
        ClientGUIDialogsMessage.ShowInformation = show( 'information' )
        ClientGUIDialogsQuick.GetYesNo = get_yes_no

        statuses = {
            'successful' : CC.STATUS_SUCCESSFUL_AND_NEW,
            'redundant' : CC.STATUS_SUCCESSFUL_BUT_REDUNDANT,
            'deleted' : CC.STATUS_DELETED,
            'failed' : CC.STATUS_ERROR,
        }

        action_ids = { 'delete' : CC.IMPORT_FOLDER_DELETE, 'ignore' : CC.IMPORT_FOLDER_IGNORE, 'move' : CC.IMPORT_FOLDER_MOVE }
        action_names = { v : k for ( k, v ) in action_ids.items() }

        # import folders

        def import_folder( case ):

            folder = ClientImportLocal.ImportFolder( case[ 'name' ], path = case[ 'path' ], period = case[ 'period' ], check_regularly = case[ 'check_regularly' ] )

            folder._paused = case[ 'paused' ]

            return folder


        import_cases = [ ( c[ 'name' ], import_folder( c ) ) for c in lists.IMPORT_FOLDERS ]
        import_cases.append( ( 'new', ClientImportLocal.ImportFolder( 'import folder' ) ) )

        def import_fields( panel ):

            def action( which ):

                choice = getattr( panel, f'_action_{which}' )
                location = getattr( panel, f'_location_{which}' )

                return {
                    'choices' : [ choice.itemText( i ) for i in range( choice.count() ) ],
                    'action' : action_names[ choice.GetValue() ],
                    'location' : location.GetPath(),
                    'location_enabled' : location.isEnabled(),
                }


            return {
                'name' : panel._name.text(),
                'path' : panel._path.GetPath(),
                'search_subdirectories' : panel._search_subdirectories.isChecked(),
                'paused' : panel._paused.isChecked(),
                'check_regularly' : panel._check_regularly.isChecked(),
                'period' : panel._period.GetValue(),
                'period_enabled' : panel._period.isEnabled(),
                'skip' : panel._last_modified_time_skip_period.GetValue(),
                'check_now' : panel._check_now.isChecked(),
                'popup' : panel._show_working_popup.isChecked(),
                'popup_button' : panel._publish_files_to_popup_button.isChecked(),
                'page' : panel._publish_files_to_page.isChecked(),
                'actions' : { which : action( which ) for which in ( 'successful', 'redundant', 'deleted', 'failed' ) },
                'filename_tagging' : [ list( t ) for t in [ panel._ConvertFilenameTaggingOptionsToDisplayTuple( d ) for d in panel._filename_tagging_options.GetData() ] ],
                'import_options' : panel._import_options_container_button.text(),
            }


        import_opened = []

        for ( name, folder ) in import_cases:

            panel = ClientGUIImportFolders.EditImportFolderPanel( gui, folder )

            import_opened.append( { 'folder' : name, 'fields' : import_fields( panel ) } )


        import_applied = []

        for ( name, edits ) in IMPORT_EDITS:

            folder = dict( import_cases )[ name ]

            panel = ClientGUIImportFolders.EditImportFolderPanel( gui, folder )

            for ( field, value ) in edits.items():

                if field == 'name':

                    panel._name.setText( value )

                elif field == 'path':

                    panel._path.SetPath( value )

                elif field == 'actions':

                    for ( which, ( action, location ) ) in value.items():

                        getattr( panel, f'_action_{which}' ).SetValue( action_ids[ action ] )

                        if location is not None:

                            getattr( panel, f'_location_{which}' ).SetPath( location )



                elif field == 'period':

                    panel._period.SetValue( value )

                elif field == 'skip':

                    panel._last_modified_time_skip_period.SetValue( value )

                else:

                    widget = {
                        'paused' : panel._paused,
                        'check_regularly' : panel._check_regularly,
                        'check_now' : panel._check_now,
                        'search_subdirectories' : panel._search_subdirectories,
                        'popup' : panel._show_working_popup,
                        'popup_button' : panel._publish_files_to_popup_button,
                        'page' : panel._publish_files_to_page,
                    }[ field ]

                    widget.setChecked( value )



            asked.clear()

            try:

                value = panel.GetValue()

                ( v_name, v_path, _, _, actions, action_locations, period, check_regularly, paused, check_now, popup, popup_button, page ) = value.ToTuple()

                result = {
                    'value' : {
                        'name' : v_name,
                        'path' : v_path,
                        'actions' : { which : action_names[ actions[ statuses[ which ] ] ] for which in statuses },
                        'locations' : { which : action_locations[ statuses[ which ] ] for which in statuses if statuses[ which ] in action_locations },
                        'period' : period,
                        'check_regularly' : check_regularly,
                        'paused' : paused,
                        'check_now' : check_now,
                        'popup' : popup,
                        'popup_button' : popup_button,
                        'page' : page,
                        'skip' : value.GetLastModifiedTimeSkipPeriod(),
                        'search_subdirectories' : value.GetSearchSubdirectories(),
                    }
                }

            except HydrusExceptions.VetoException as e:

                result = { 'veto' : str( e ) }


            result[ 'folder' ] = name
            result[ 'edits' ] = edits
            result[ 'asked' ] = list( asked )

            import_applied.append( result )


        # export folders

        location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY )

        def export_folder( case ):

            predicates = ClientLocalServerCore.ConvertTagListToPredicates( None, case[ 'tags' ], do_permission_check = False ) if case[ 'tags' ] else []

            file_search_context = ClientSearchFileSearchContext.FileSearchContext( location_context = location_context, predicates = predicates )

            export_type = HC.EXPORT_FOLDER_TYPE_SYNCHRONISE if case[ 'type' ] == 'synchronise' else HC.EXPORT_FOLDER_TYPE_REGULAR

            return ClientExportingFiles.ExportFolder(
                case[ 'name' ],
                path = case[ 'path' ],
                export_type = export_type,
                delete_from_client_after_export = case[ 'delete' ],
                file_search_context = file_search_context,
                run_regularly = case[ 'run_regularly' ],
                period = case[ 'period' ],
                phrase = case[ 'phrase' ],
                run_now = case[ 'run_now' ],
                last_error = case[ 'last_error' ]
            )


        export_cases = [ ( c[ 'name' ], export_folder( c ) ) for c in lists.EXPORT_FOLDERS ]

        new_options = controller.new_options

        export_cases.append( ( 'new', ClientExportingFiles.ExportFolder(
            'export folder',
            '',
            export_type = HC.EXPORT_FOLDER_TYPE_REGULAR,
            delete_from_client_after_export = False,
            file_search_context = ClientSearchFileSearchContext.FileSearchContext( location_context = new_options.GetDefaultLocalLocationContext() ),
            metadata_routers = [],
            period = 86400,
            phrase = new_options.GetString( 'export_phrase' )
        ) ) )

        def export_fields( panel ):

            return {
                'name' : panel._name.text(),
                'path' : panel._path.GetPath(),
                'type' : panel._type.currentText(),
                'types' : [ panel._type.itemText( i ) for i in range( panel._type.count() ) ],
                'delete' : panel._delete_from_client_after_export.isChecked(),
                'delete_enabled' : panel._delete_from_client_after_export.isEnabled(),
                'symlinks' : panel._export_symlinks.isChecked(),
                'run_regularly' : panel._run_regularly.isChecked(),
                'period' : panel._period.GetValue(),
                'period_enabled' : panel._period.isEnabled(),
                'popup' : panel._show_working_popup.isChecked(),
                'popup_enabled' : panel._show_working_popup.isEnabled(),
                'run_now' : panel._run_now.isChecked(),
                'predicates' : [ p.ToString( with_count = False ) for p in panel._tag_autocomplete.GetFileSearchContext().GetPredicates() ],
                'phrase' : panel._pattern.text(),
                'overwrite_next' : panel._overwrite_sidecars_on_next_run.isChecked(),
                'overwrite_always' : panel._always_overwrite_sidecars.isChecked(),
            }


        export_opened = []

        for ( name, folder ) in export_cases:

            panel = ClientGUIExport.EditExportFolderPanel( gui, folder )

            export_opened.append( { 'folder' : name, 'fields' : export_fields( panel ) } )


        panel = ClientGUIExport.EditExportFolderPanel( gui, dict( export_cases )[ 'clear out' ] )

        panel._type.SetValue( HC.EXPORT_FOLDER_TYPE_SYNCHRONISE )

        export_synchronise = { 'delete' : panel._delete_from_client_after_export.isChecked(), 'delete_enabled' : panel._delete_from_client_after_export.isEnabled() }

        export_applied = []

        for ( name, edits ) in EXPORT_EDITS:

            panel = ClientGUIExport.EditExportFolderPanel( gui, dict( export_cases )[ name ] )

            asked.clear()

            for ( field, value ) in edits.items():

                if field == 'name':

                    panel._name.setText( value )

                elif field == 'path':

                    panel._path.SetPath( value )

                elif field == 'phrase':

                    panel._pattern.setText( value )

                elif field == 'period':

                    panel._period.SetValue( value )

                elif field == 'delete':

                    panel._delete_from_client_after_export.setChecked( value )

                    panel.EventDeleteFilesAfterExport()

                elif field == 'answer':

                    answers.append( value )

                else:

                    widget = {
                        'run_regularly' : panel._run_regularly,
                        'run_now' : panel._run_now,
                        'symlinks' : panel._export_symlinks,
                        'popup' : panel._show_working_popup,
                        'overwrite_next' : panel._overwrite_sidecars_on_next_run,
                        'overwrite_always' : panel._always_overwrite_sidecars,
                    }[ field ]

                    widget.setChecked( value )



            result = { 'folder' : name, 'edits' : edits }

            if not panel.UserIsOKToOK():

                result[ 'refused' ] = True

            else:

                try:

                    value = panel.GetValue()

                    ( v_name, v_path, export_type, delete, symlinks, _, run_regularly, period, phrase, _, run_now ) = value.ToTuple()

                    result[ 'value' ] = {
                        'name' : v_name,
                        'path' : v_path,
                        'type' : 'synchronise' if export_type == HC.EXPORT_FOLDER_TYPE_SYNCHRONISE else 'regular',
                        'delete' : delete,
                        'symlinks' : symlinks,
                        'run_regularly' : run_regularly,
                        'period' : period,
                        'phrase' : phrase,
                        'run_now' : run_now,
                        'popup' : value.ShowWorkingPopup(),
                        'overwrite_next' : value.GetOverwriteSidecarsOnNextRun(),
                        'overwrite_always' : value.GetAlwaysOverwriteSidecars(),
                    }

                except HydrusExceptions.VetoException as e:

                    result[ 'veto' ] = str( e )



            result[ 'asked' ] = list( asked )

            if answers:

                raise Exception( f'unasked answers left: {answers}' )


            export_applied.append( result )


        return {
            'missing' : MISSING,
            'existing' : EXISTING,
            'import_opened' : import_opened,
            'import_applied' : import_applied,
            'export_opened' : export_opened,
            'export_synchronise' : export_synchronise,
            'export_applied' : export_applied,
        }


    return controller.CallBlockingToQt( gui, f )


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


    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'folders_dialogs.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
