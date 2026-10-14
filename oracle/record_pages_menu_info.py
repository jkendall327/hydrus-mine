#!/usr/bin/env python3
"""Record three small things of the reference's Pages menu.

On the `basic` fixture in the running client:

* the history submenu with no history (`page_nav_history_menu`, as the menu
  bar builds it, and after "Clear History" has emptied a real history);
* the "weight" report (`_ShowPageWeightInfo`, the message handed to
  `ShowInformation`) and the "total session weight" menu label
  (`_UpdateMenuPagesCountIfDirty`) for a search page of `FILES` files, a URL
  page of `URLS` URLs and a multiple watcher page whose first watcher has
  `WATCHER_SEEDS` file seeds, then again after the search page is closed (its
  weight moves to the undo list) and the other pages too;
* two multiple watcher pages: which watcher each highlights after URLs are
  added, and after `clear_multiwatcher_highlights` (what Pages > clear > all
  multiwatcher highlights publishes), as the page keeps it for the session.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_pages_menu_info.py
       (writes fixtures/pages_menu_info.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'pages_menu_info.json' )

FILES = 5
URLS = [ 'https://files.example/0.jpg', 'https://files.example/1.jpg', 'https://files.example/2.jpg' ]
WATCHER_URLS = [ 'https://boards.example/a/thread/1', 'https://boards.example/a/thread/2' ]
WATCHER_SEEDS = 4


def record( session ):

    from hydrus.client import ClientConstants as CC
    from qtpy import QtWidgets as QW

    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.importing import ClientImportFileSeeds

    controller = session.controller
    gui = controller.gui

    import record_api

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ][ : FILES ] ]

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    def history_entries():

        gui._pages_history_dirty = True

        gui._UpdateMenuPagesHistoryIfDirty()

        out = []

        for action in gui.page_nav_history_menu.actions():

            out.append( '---' if action.isSeparator() else { 'text' : action.text(), 'enabled' : action.isEnabled() } )


        return out


    questions = []

    def get_yes_no( parent, message, *args, **kwargs ):

        questions.append( message )

        return QW.QDialog.DialogCode.Accepted


    ClientGUIDialogsQuick.GetYesNo = get_yes_no

    out = {}

    # --- history
    def clear_pages():

        notebook = gui._notebook

        while notebook.count() > 0:

            notebook.widget( 0 ).deleteLater()
            notebook.removeTab( 0 )


        gui.page_nav_history.CleanPages( {} )


    qt( clear_pages )

    out[ 'history_empty' ] = qt( history_entries )

    def real_history():

        location_context = controller.new_options.GetDefaultLocalLocationContext()

        for name in [ 'one', 'two' ]:

            gui._notebook.NewPageQuery( location_context, page_name = name )


        for name in [ 'one', 'two', 'one' ]:

            for page in gui._notebook.GetPages():

                if page.GetName() == name:

                    gui._notebook.setCurrentWidget( page )

                    gui.NotifyPageJustChanged()



    qt( real_history )

    out[ 'history_filled' ] = qt( history_entries )

    qt( lambda: gui._SetPagesHistoryEmpty() )

    out[ 'history_cleared' ] = qt( history_entries )

    # --- weight
    shown = []

    real_show_information = ClientGUIDialogsMessage.ShowInformation

    ClientGUIDialogsMessage.ShowInformation = lambda win, message: shown.append( message )

    def weight():

        gui._pages_count_dirty = True

        gui._UpdateMenuPagesCountIfDirty()

        del shown[:]

        gui._ShowPageWeightInfo()

        return { 'label' : gui._menubar_pages_session_weight.text(), 'count_label' : gui._menubar_pages_page_count.text(), 'message' : shown[ 0 ] }


    def build():

        clear_pages()

        location_context = controller.new_options.GetDefaultLocalLocationContext()

        gui._notebook.NewPageQuery( location_context, initial_hashes = hashes, page_name = 'files' )

        page = gui._notebook.NewPageImportURLs( page_name = 'urls' )

        urls_import = page.GetPageManager().GetVariable( 'urls_import' )

        urls_import.PendURLs( URLS )

        page = gui._notebook.NewPageImportMultipleWatcher()

        page.SetName( 'watchers' )

        sidebar = page.GetSidebar()

        sidebar._AddURLs( WATCHER_URLS[ : 1 ] )

        watcher = list( sidebar._multiple_watcher_import.GetWatchers() )[ 0 ]

        watcher.PausePlayChecking()

        file_seeds = [ ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, f'https://boards.example/a/file/{i}.jpg' ) for i in range( WATCHER_SEEDS ) ]

        watcher.GetFileSeedCache().AddFileSeeds( file_seeds )


    qt( build )

    time.sleep( 1.0 )

    weights = []

    weights.append( { 'when' : 'open', 'report' : qt( weight ), 'counts' : qt( gui.GetTotalPageCounts ) } )

    def close_named( name ):

        for page in gui._notebook.GetPages():

            if page.GetName() == name:

                gui._notebook.setCurrentWidget( page )

                gui._notebook.CloseCurrentPage()

                return



    qt( lambda: close_named( 'files' ) )

    time.sleep( 0.5 )

    weights.append( { 'when' : 'files page closed', 'report' : qt( weight ), 'counts' : qt( gui.GetTotalPageCounts ) } )

    del questions[:]

    qt( lambda: close_named( 'urls' ) )
    qt( lambda: close_named( 'watchers' ) )

    out[ 'close_questions' ] = list( questions )

    time.sleep( 0.5 )

    weights.append( { 'when' : 'all closed', 'report' : qt( weight ), 'counts' : qt( gui.GetTotalPageCounts ) } )

    ClientGUIDialogsMessage.ShowInformation = real_show_information

    out[ 'weight' ] = { 'files' : FILES, 'urls' : URLS, 'watcher_seeds' : WATCHER_SEEDS, 'steps' : weights }

    # --- multiwatcher highlights
    def watcher_pages():

        clear_pages()

        pages = []

        for i in range( 2 ):

            page = gui._notebook.NewPageImportMultipleWatcher()

            sidebar = page.GetSidebar()

            sidebar._AddURLs( WATCHER_URLS )

            for watcher in sidebar._multiple_watcher_import.GetWatchers():

                watcher.PausePlayChecking()


            pages.append( page )


        return pages


    pages = qt( watcher_pages )

    time.sleep( 1.5 )

    def highlights():

        result = []

        for page in pages:

            sidebar = page.GetSidebar()

            highlighted = sidebar._multiple_watcher_import.GetHighlightedWatcher()

            result.append( {
                'highlighted' : None if highlighted is None else highlighted.GetURL(),
                'panel' : None if sidebar._highlighted_watcher is None else sidebar._highlighted_watcher.GetURL()
            } )


        return result


    before = qt( highlights )

    controller.pub( 'clear_multiwatcher_highlights' )

    time.sleep( 1.0 )

    after = qt( highlights )

    out[ 'multiwatcher' ] = { 'urls' : WATCHER_URLS, 'before' : before, 'after' : after }

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
