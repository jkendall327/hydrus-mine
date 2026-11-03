#!/usr/bin/env python3
"""Flow: import files from disk by hand, with tags, import options and
"delete original files after successful import".

On the `basic` fixture (no site; the flow is all local), a folder (`WORK`)
holding `FILES`: two images, an exact copy of the first, a file of a type
hydrus does not import, and a subfolder with an image:

1. "review": file > import files with the folder (`gui._ImportFiles`, as a
   folder dropped on the window gives it, the real "review files to
   import" frame and its parsing worker), parsed; "delete original files
   after successful import" ticked.
2. "options": its import options button (the real
   `EditSpecificImportOptionsContainerPanel` dialog): "locations" set to
   custom with "archive all imports" ticked, apply. (In simple mode, as a
   new client has it, a local import's editor lists no "tags" kind.)
3. "import": "add tags/urls with the import >>" (the real filename tagging
   dialog): on "my tags", `imported` for all, the filename as a `title`
   tag, and `only second` for the second file alone; apply. The import
   page it makes imports every good file; wait until it is done.

After each step: what the windows showed (the review list's rows, its
progress text and the warning under the delete box; the import options
list's labels; the filename tagging rows on "my tags"), and from the
import on, the import page's file log and thumbnails, each imported file
(tags, URLs, inbox, domains), what is left in the folder, and the popups.
Paths are normalised to `WORK`, times as `flows.normaliser` does.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_manual_import.py
       (writes oracle/fixtures/flow_manual_import.json)
"""

import os
import re
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_manual_import.json' )
MEDIA = os.path.join( HERE, 'fixtures', 'media' )

# ( where in the folder, which media fixture, or None for text )
FILES = [
    ( 'a_first.jpg', 'jpeg_420.jpg' ),
    ( 'b_second.png', 'apng_rgba.png' ),
    ( 'c_copy.jpg', 'jpeg_420.jpg' ),
    ( 'd_readme.md', None ),
    ( os.path.join( 'sub', 'e_third.jpg' ), 'jpeg_gray.jpg' ),
]

OLD_MTIME = 1_600_000_000


def place( folder ):

    for ( name, media ) in FILES:

        path = os.path.join( folder, name )

        os.makedirs( os.path.dirname( path ), exist_ok = True )

        if media is None:

            with open( path, 'w' ) as f:

                f.write( '# not a picture\n' )


        else:

            shutil.copy( os.path.join( MEDIA, media ), path )


        os.utime( path, ( OLD_MTIME, OLD_MTIME ) )



def listing( folder ):
    """Every file under `folder` (relative path, size)."""

    out = {}

    for ( root, dirs, files ) in os.walk( folder ):

        for name in files:

            path = os.path.join( root, name )

            out[ os.path.relpath( path, folder ) ] = os.path.getsize( path )



    return dict( sorted( out.items() ) )


def record( session ):

    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.panels import ClientGUILocalFileImports
    from hydrus.client.importing.options import ImportOptionsConstants as IOC

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    work = os.path.realpath( tempfile.mkdtemp() )

    place( work )

    def normalise( text ):

        if text is None:

            return None


        text = text.replace( work, 'WORK' )
        text = flows.DATE_TIME.sub( 'TIME', text )
        # ("which was now before this check", or "1 second ago", a moment apart)
        text = re.sub( r'which was .* before this check', 'which was DELTA before this check', text )
        text = flows.TIME_DELTA.sub( 'DELTA', text )

        return flows.JUST_NOW.sub( 'DELTA', text )


    steps = []

    def pump_off_thread( condition, what, timeout = 60 ):

        deadline = time.time() + timeout

        while not condition():

            if time.time() > deadline:

                raise Exception( 'timed out: ' + what )


            time.sleep( 0.1 )



    # 1. review ----------------------------------------------------------

    qt( lambda: gui._ImportFiles( [ work ] ) )

    def review_panel():

        panels = gui.findChildren( ClientGUILocalFileImports.ReviewLocalFileImports )

        return panels[0] if len( panels ) > 0 else None


    panel = qt( review_panel )

    def parsed():

        return panel._unparsed_paths_queue.empty() and panel._parsed_paths_queue.empty() and panel._paths_list.count() == len( FILES )


    pump_off_thread( lambda: qt( parsed ), 'the folder parsed' )

    time.sleep( 1.0 )

    def tick_delete():

        panel._delete_after_success.setChecked( True )
        panel.EventDeleteAfterSuccessCheck()


    qt( tick_delete )

    def review_seen():

        rows = sorted( panel._paths_list.GetData(), key = lambda lfp: lfp.index )

        return {
            'title' : panel.window().windowTitle(),
            'rows' : [ [ normalise( c ) for c in panel._ConvertPathToDisplayTuple( r ) ] for r in rows ],
            'progress' : normalise( panel._progress._st.text() ),
            'delete' : panel._delete_after_success.isChecked(),
            'delete warning' : panel._delete_after_success_st.text(),
            'import options' : panel._import_options_container_button.text(),
        }


    steps.append( { 'step' : 'review', 'dialog' : qt( review_seen ) } )

    # 2. options ---------------------------------------------------------

    seen = {}

    def import_options( dlg, panel_ ):

        pages = panel_._listbook.GetPages()

        seen[ 'title' ] = dlg.windowTitle()
        seen[ 'before' ] = [ p.GetLabel() for p in pages ]

        for page in pages:

            kind = page.GetImportOptionsType()

            if kind == IOC.IMPORT_OPTIONS_TYPE_LOCATIONS:

                page.SetDefault( False )

                page._options_panel._auto_archive.setChecked( True )


        panel_._UpdateLabels()

        seen[ 'after' ] = [ p.GetLabel() for p in pages ]

        return True


    with flows.scripted_dialogs( { 'EditSpecificImportOptionsContainerPanel' : [ import_options ] } ):

        qt( lambda: panel._import_options_container_button._EditOptions() )


    seen[ 'button' ] = qt( lambda: panel._import_options_container_button.text() )

    steps.append( { 'step' : 'options', 'dialog' : seen } )

    # 3. import ----------------------------------------------------------

    seen = {}

    def filename_tagging( dlg, tagging ):

        seen[ 'title' ] = dlg.windowTitle()
        seen[ 'tabs' ] = [ tagging._notebook.tabText( i ) for i in range( tagging._notebook.count() ) ]

        page = next( p for p in tagging._filename_tagging_option_pages if p._service_key == CC.DEFAULT_LOCAL_TAG_SERVICE_KEY )

        simple = page._filename_tagging_panel._simple_panel

        simple.EnterTags( { 'imported' } )
        simple._filename_namespace.SetChecked( True )
        simple._filename_namespace.SetValue( 'title' )

        second = os.path.join( work, 'b_second.png' )

        simple.SetSelectedPaths( [ second ] )
        simple.EnterTagsSingle( { 'only second' } )

        page.RefreshFileList()

        seen[ 'rows' ] = [ [ normalise( c ) for c in page._ConvertDataToDisplayTuple( ( i, path ) ) ] for ( i, path ) in enumerate( tagging._paths ) ]

        return True


    pages_before = qt( lambda: len( gui._notebook.GetMediaPages() ) )

    with flows.scripted_dialogs( { 'EditLocalImportFilenameTaggingPanel' : [ filename_tagging ] } ):

        qt( lambda: panel._AddTags() )


    pump_off_thread( lambda: qt( lambda: len( gui._notebook.GetMediaPages() ) ) > pages_before, 'the import page' )

    page = qt( lambda: gui._notebook.GetCurrentMediaPage() )
    hdd = qt( lambda: page.GetPageManager().GetVariable( 'hdd_import' ) )

    def import_done():

        cache = hdd.GetFileSeedCache()

        return cache.GetFileSeedCount() > 0 and cache.GetFileSeedCount( 0 ) == 0 and not hdd.CurrentlyWorking()


    flows.wait_until_idle( controller, also = import_done )

    files = flows.file_log_state( hdd.GetFileSeedCache(), normalise )

    steps.append( {
        'step' : 'import',
        'dialog' : seen,
        'page' : {
            'name' : qt( lambda: page.GetName() ),
            'import options' : qt( lambda: page.GetSidebar()._import_options_container_button.text() ),
            'files' : files,
            'thumbnails' : qt( lambda: len( page.GetMediaResultsPanel().GetSortedMedia() ) ),
        },
        'files' : flows.files_state( session, flows.hashes_in_log( files ), normalise ),
        'left' : listing( work ),
        'review open' : qt( review_panel ) is not None,
        'popups' : flows.popups_shown( controller, normalise ),
    } )

    return { 'files' : [ [ n, m ] for ( n, m ) in FILES ], 'steps' : steps }


def child( out ):

    import hydrus_driver
    import record_api

    result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )

    flows.write_fixture( out, result )


def main():

    if len( sys.argv ) > 2 and sys.argv[1] == '--child':

        child( sys.argv[2] )

        return


    import hydrus_driver

    hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', OUT )


if __name__ == '__main__':

    main()
