#!/usr/bin/env python3
"""Flow 4: search, select, and export files with a filename pattern and
sidecars.

On the `basic` fixture:

1. "open": a new search page for "safe" (`NewPageQuery`), once its search
   has finished all its thumbnails selected, and the share > export >
   files command (`ClientGUIMediaModalActions.ExportFiles`'s window, the
   real `ReviewExportFilesPanel`).
2. "pattern": a destination folder and the filename pattern
   "[creator] [character]" (which makes some names alike), then "update"; then
   the pattern "{#} - [character] - {hash}" and "update". The rows (number,
   filetype, destination) as the window lists them after each.
3. "sidecars": the sidecars button (`SingleFileMetadataRoutersButton`, its
   dialog) and the "easy one-click JSON" template, apply.
4. "export": the export button; wait until it is done. The destination
   folder afterwards: each file's name, its SHA-256 (for a media file) or
   its JSON (for a sidecar), and the popups.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_export.py
       (writes oracle/fixtures/flow_export.json)
"""

import hashlib
import json
import os
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_export.json' )

# (the flow has no site of its own, but every flow has a port)
SITE_PORT = 46014

PHASES = [ flows.booru_phase( {} ) ]

SEARCH = 'safe'

PATTERNS = [ '[creator] [character]', '{#} - [character] - {hash}' ]


def contents( folder ):
    """Each file under `folder`: a media file's SHA-256, a sidecar's JSON."""

    out = {}

    for ( root, dirs, files ) in os.walk( folder ):

        for name in files:

            path = os.path.join( root, name )

            with open( path, 'rb' ) as f:

                data = f.read()


            relative = os.path.relpath( path, folder )

            if name.endswith( '.json' ):

                out[ relative ] = json.loads( data.decode( 'utf-8' ) )

            else:

                out[ relative ] = hashlib.sha256( data ).hexdigest()




    return dict( sorted( out.items() ) )


def record( session, site, destination ):

    controller = session.controller
    gui = controller.gui
    base_normalise = flows.normaliser( site )

    def normalise( text ):

        if text is None:

            return None


        return base_normalise( text.replace( destination, 'EXPORT' ) )


    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.exporting import ClientGUIExport
    from hydrus.client.media import ClientMediaFileFilter
    from hydrus.client import ClientLocation
    from hydrus.client.metadata import ClientTags
    from hydrus.client.search import ClientSearchPredicate
    from hydrus.core import HydrusConstants as HC

    steps = []

    asked = []

    def show( kind ):

        def shower( win, message, **kwargs ):

            asked.append( { 'kind' : kind, 'message' : normalise( message ) } )


        return shower


    ClientGUIDialogsMessage.ShowWarning = show( 'warning' )
    ClientGUIDialogsMessage.ShowInformation = show( 'information' )
    ClientGUIDialogsMessage.ShowCritical = lambda win, title, message, **kwargs: asked.append( { 'kind' : 'critical', 'message' : normalise( message ) } )

    def get_yes_no( win, message, **kwargs ):

        asked.append( { 'kind' : 'yes/no', 'message' : normalise( message ) } )

        return QW.QDialog.DialogCode.Accepted


    ClientGUIDialogsQuick.GetYesNo = get_yes_no

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def rows( panel ):

        return [ [ normalise( str( d ) ) for d in panel._ConvertDataToDisplayTuple( m ) ] for m in panel._paths.GetData() ]


    def step( name, seen ):

        steps.append( {
            'step' : name,
            'dialog' : seen,
            'asked' : list( asked ),
            'exported' : contents( destination ),
            'popups' : flows.popups_shown( controller, normalise ),
            'hits' : site.take_hits(),
        } )

        asked.clear()


    domain_manager = qt( lambda: flows.install( controller, site ) )

    # ---------------------------------------------------------------- 1. open

    seen = {}

    def new_page():

        location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

        predicate = ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, SEARCH )

        return gui._notebook.NewPageQuery( location, initial_predicates = [ predicate ] )


    page = qt( new_page )

    def results_panel():

        return page.GetMediaResultsPanel()


    def searched():

        return qt( lambda: len( results_panel().GetSortedMedia() ) ) > 0


    deadline = time.time() + 60

    while not searched():

        assert time.time() < deadline, 'the search never finished'

        time.sleep( 0.1 )


    flows.wait_until_idle( controller )

    def open_export():

        panel = results_panel()

        seen[ 'thumbnails' ] = len( panel.GetSortedMedia() )

        panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_ALL ) )

        from hydrus.client import ClientApplicationCommand as CAC

        panel.ProcessApplicationCommand( CAC.ApplicationCommand.STATICCreateSimpleCommand( CAC.SIMPLE_EXPORT_FILES ) )


    qt( open_export )

    def export_panel():

        for w in QW.QApplication.topLevelWidgets():

            found = w.findChildren( ClientGUIExport.ReviewExportFilesPanel )

            if len( found ) > 0:

                return w, found[0]



        raise Exception( 'no export window' )


    ( frame, panel ) = qt( export_panel )

    def look():

        seen[ 'title' ] = frame.windowTitle()
        seen[ 'phrase starts as' ] = panel._pattern.text()
        seen[ 'sidecars start as' ] = panel._metadata_routers_button.text()
        seen[ 'trash after export' ] = panel._delete_files_after_export.isChecked()
        seen[ 'symlinks' ] = panel._export_symlinks.isChecked()
        seen[ 'file numbers' ] = sorted( panel._media_to_number_indices.values() )


    qt( look )

    step( 'open', seen )

    # -------------------------------------------------------------- 2. pattern

    seen = { 'rows' : [] }

    def pattern( text ):

        def f():

            panel._directory_picker.SetPath( destination )
            panel._pattern.setText( text )
            panel._update.click()

            seen[ 'rows' ].append( { 'pattern' : text, 'rows' : rows( panel ) } )


        return f


    # (a new path for each pattern, as the window makes them again)
    for text in PATTERNS:

        qt( pattern( text ) )


    step( 'pattern', seen )

    # ------------------------------------------------------------- 3. sidecars

    seen = {}

    def routers_dialog( dlg, routers_panel ):

        control = routers_panel._control

        seen[ 'templates' ] = [ item.title if isinstance( item.title, str ) else item.title() for item in control._templates_button._menu_template_items ]

        item = control._templates_button._menu_template_items[0]

        item.call( *item.args, **item.kwargs )

        seen[ 'router rows' ] = [ control._data_to_pretty_callable( d ) for d in control.GetData() ]

        return True


    with flows.scripted_dialogs( { 'EditSingleCtrlPanel' : [ routers_dialog ] } ) as dialogs:

        qt( lambda: panel._metadata_routers_button._Edit() )

        flows.wait_until_idle( controller, also = dialogs.check )


    seen[ 'sidecars end as' ] = qt( lambda: panel._metadata_routers_button.text() )

    step( 'sidecars', seen )

    # --------------------------------------------------------------- 4. export

    seen = {}

    qt( lambda: panel._pattern.setText( PATTERNS[1] ) )

    qt( lambda: panel._export.click() )

    def export_done():

        return qt( lambda: panel._export.isEnabled() and panel._export.text() == 'export' )


    # (the worker disables the button at once, then enables it after a second)
    time.sleep( 0.5 )

    flows.wait_until_idle( controller, also = export_done )

    seen[ 'rows' ] = qt( lambda: rows( panel ) )

    step( 'export', seen )

    return { 'domain_manager' : domain_manager, 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    work = tempfile.mkdtemp( prefix = 'hydrus_flow_export_' )

    destination = os.path.join( work, 'out' )

    os.makedirs( destination )

    try:

        with flows.Site( SITE_PORT, PHASES ) as site:

            result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), lambda session: record( session, site, destination ), network = True )


    finally:

        shutil.rmtree( work, ignore_errors = True )


    result[ 'site' ] = site.describe()

    flows.write_fixture( out, result )


def main():

    if len( sys.argv ) > 2 and sys.argv[1] == '--child':

        child( sys.argv[2] )

        return


    import hydrus_driver

    hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', OUT )


if __name__ == '__main__':

    main()
