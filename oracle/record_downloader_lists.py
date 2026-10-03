#!/usr/bin/env python3
"""Record a gallery and a watcher downloader page's lists with several
rows selected, as the reference's sidebars act on them.

The lists select several (Qt's extended selection), and their buttons act
on each selected: highlight only with one selected that isn't shown
(`_CanHighlight`); remove asks how many, how many are still working (and,
for watchers, aren't yet DEAD), and whether the highlighted one is among
them (`_RemoveGalleryImports`, `_RemoveWatchers`). "update selected with
current options" shows while a selected query's file limit or import
options, or a selected watcher's checker or import options, differ from
the page's (`_UpdateImportOptionsSetButtonVisibility`), and gives them
the page's after asking (`_SetOptionsToGalleryImports`,
`_SetOptionsToWatchers`).

In the running client, on the `basic` fixture, with all new network
traffic paused (so nothing is checked or downloaded), this opens a
gallery page and pends three queries (their files and searches paused at
start, so none works), and a watcher page with three watchers (their
checking paused, so none is checked), and for
each step records the rows selected (by query or URL), whether highlight
is offered, whether "update selected with current options" shows, what
was asked and answered, and each row's file limit or checker options
(`[intended files per check, never faster than, never slower than,
[files, seconds]]`).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_downloader_lists.py
       (writes fixtures/downloader_lists.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'downloader_lists.json' )

DAY = 86400

QUERIES = [ 'blue', 'green', 'red' ]
URLS = [ 'https://boards.example/thread/1', 'https://boards.example/thread/2', 'https://boards.example/thread/3' ]

# ( what, rows selected (by index), argument, answers )
GALLERY_STEPS = [
    ( 'highlight', [ 0 ], None, [] ),
    ( 'select', [ 1, 2 ], None, [] ),
    ( 'remove', [ 1, 2 ], None, [ False ] ),
    ( 'select', [ 0, 1 ], None, [] ),
    ( 'remove', [ 0, 1 ], None, [ False ] ),
    ( 'select', [ 1 ], None, [] ),
    ( 'work', [ 2 ], None, [] ),
    ( 'remove', [ 0, 1, 2 ], None, [ False ] ),
    ( 'file limit', [ 0, 1 ], 50, [] ),
    ( 'select', [ 2 ], None, [] ),
    ( 'set options', [ 0, 1 ], None, [ False ] ),
    ( 'set options', [ 0, 1 ], None, [ True ] ),
    ( 'select', [ 1, 2 ], None, [] ),
    ( 'remove', [ 1, 2 ], None, [ True ] ),
]

WATCHER_STEPS = [
    ( 'highlight', [ 0 ], None, [] ),
    ( 'select', [ 1, 2 ], None, [] ),
    ( 'remove', [ 1, 2 ], None, [ False ] ),
    ( 'select', [ 0, 1 ], None, [] ),
    ( 'remove', [ 0, 1 ], None, [ False ] ),
    ( 'work', [ 2 ], None, [] ),
    ( 'remove', [ 0, 1, 2 ], None, [ False ] ),
    ( 'checker', [ 0, 1 ], ( 1, 4 * 3600, 7 * DAY, ( 1, 30 * DAY ) ), [] ),
    ( 'select', [ 2 ], None, [] ),
    ( 'set options', [ 0, 1 ], None, [ False ] ),
    ( 'set options', [ 0, 1 ], None, [ True ] ),
    ( 'select', [ 1, 2 ], None, [] ),
    ( 'remove', [ 1, 2 ], None, [ True ] ),
]


def as_list( checker_options ):

    ( intended, faster, slower, ( files, seconds ) ) = checker_options.ToTuple()

    return [ intended, faster, slower, [ files, seconds ] ]


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtWidgets as QW

    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick

    answers = []
    asked = []

    def get_yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', check_for_cancelled = False, **kwargs ):

        answer = answers.pop( 0 )

        asked.append( { 'message' : message, 'title' : title, 'yes' : yes_label, 'no' : no_label, 'answer' : answer } )

        return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


    def show_warning( win, message, **kwargs ):

        asked.append( { 'warning' : message } )


    ClientGUIDialogsQuick.GetYesNo = get_yes_no
    ClientGUIDialogsMessage.ShowWarning = show_warning
    ClientGUIDialogsMessage.ShowInformation = show_warning

    def setup( kind ):

        if kind == 'gallery':

            # (the fixture has no downloaders: one, never used, the network
            # being paused)
            from hydrus.client.networking import ClientNetworkingGUG

            gug = ClientNetworkingGUG.GalleryURLGenerator( 'example tag search', url_template = 'https://booru.example/search/%tags%/1', replacement_phrase = '%tags%', search_terms_separator = '+', initial_search_text = 'tag', example_search_text = 'blue_eyes' )

            controller.network_engine.domain_manager.SetGUGs( [ gug ] )

            page = gui._notebook.NewPageImportGallery()

            sidebar = page.GetSidebar()

            sidebar._multiple_gallery_import.SetStartFileQueuesPaused( True )
            sidebar._multiple_gallery_import.SetStartGalleryQueuesPaused( True )

            sidebar._multiple_gallery_import.SetGUGKeyAndName( ( gug.GetGUGKey(), gug.GetName() ) )

            sidebar._PendQueries( QUERIES )

            importers = list( sidebar._multiple_gallery_import.GetGalleryImports() )
            names = { importer : importer.GetQueryText() for importer in importers }
            order = [ next( i for i in importers if names[ i ] == q ) for q in QUERIES ]
            listctrl = sidebar._gallery_importers_listctrl
            button = sidebar._set_options_to_queries_button
            steps = GALLERY_STEPS

            def options( importer ):

                return importer.GetFileLimit()


            highlighted = lambda: sidebar._highlighted_gallery_import

        else:

            page = gui._notebook.NewPageImportMultipleWatcher()

            sidebar = page.GetSidebar()

            sidebar._AddURLs( URLS )

            importers = list( sidebar._multiple_watcher_import.GetWatchers() )

            # (checking paused at once, before the client's worker can
            # check them: these threads have no parser, and would go DEAD)
            for importer in importers:

                importer.PausePlayChecking()


            names = { importer : importer.GetURL() for importer in importers }
            order = [ next( i for i in importers if names[ i ] == u ) for u in URLS ]
            listctrl = sidebar._watchers_listctrl
            button = sidebar._set_options_to_watchers_button
            steps = WATCHER_STEPS

            def options( importer ):

                return as_list( importer.GetCheckerOptions() )


            highlighted = lambda: sidebar._highlighted_watcher


        sidebar._UpdateImportStatusNow()

        asked.clear()

        def state( do, rows, argument ):

            alive = [ i for i in order if i in listctrl.GetData() ]

            return {
                'do' : do,
                'rows' : rows,
                'argument' : argument,
                'selected' : [ names[ i ] for i in listctrl.GetData( only_selected = True ) ],
                'highlighted' : names[ highlighted() ] if highlighted() is not None else None,
                'can_highlight' : sidebar._CanHighlight(),
                'set_options_shown' : not button.isHidden(),
                'asked' : list( asked ),
                'rows_now' : [ names[ i ] for i in alive ],
                'options' : [ options( i ) for i in alive ],
            }


        return {
            'kind' : kind,
            'sidebar' : sidebar,
            'order' : order,
            'listctrl' : listctrl,
            'steps' : steps,
            'state' : state,
        }


    def step( c, do, rows, argument, step_answers ):

        from hydrus.client.importing import ClientImportFileSeeds
        from hydrus.client.importing.options import CheckerImportOptions

        ( kind, sidebar, order, listctrl ) = ( c[ 'kind' ], c[ 'sidebar' ], c[ 'order' ], c[ 'listctrl' ] )

        answers[:] = list( step_answers )
        asked.clear()

        if True:

            listctrl.SelectDatas( [ order[ r ] for r in rows ], deselect_others = True )

            if do == 'highlight':

                sidebar._HighlightSelectedGalleryImport() if kind == 'gallery' else sidebar._HighlightSelectedWatcher()

            elif do == 'remove':

                sidebar._RemoveGalleryImports() if kind == 'gallery' else sidebar._RemoveWatchers()

            elif do == 'work':

                # (a file to import, its files unpaused: it is working)
                for importer in listctrl.GetData( only_selected = True ):

                    file_seed = ClientImportFileSeeds.FileSeed( ClientImportFileSeeds.FILE_SEED_TYPE_URL, 'https://booru.example/post/1' )

                    importer.GetFileSeedCache().AddFileSeeds( [ file_seed ] )

                    if importer.FilesPaused():

                        importer.PausePlayFiles()



                sidebar._UpdateImportStatusNow()

            elif do == 'file limit':

                sidebar._file_limit.SetValue( argument )

                sidebar.EventFileLimit()

            elif do == 'checker':

                ( intended, faster, slower, death ) = argument

                sidebar._checker_options.SetValue( CheckerImportOptions.CheckerOptions( intended_files_per_check = intended, never_faster_than = faster, never_slower_than = slower, death_file_velocity = death ) )

            elif do == 'set options':

                sidebar._SetOptionsToGalleryImports() if kind == 'gallery' else sidebar._SetOptionsToWatchers()


            sidebar._UpdateImportOptionsSetButtonVisibility()

            if answers:

                raise Exception( f'unasked answers left at {do}: {answers}' )




    def state( c, do, rows, argument ):

        return c[ 'state' ]( do, rows, list( argument ) if isinstance( argument, tuple ) else argument )


    import time

    result = {}

    for kind in ( 'gallery', 'watcher' ):

        c = controller.CallBlockingToQt( gui, lambda: setup( kind ) )

        # (a highlight loads on another thread)
        time.sleep( 2 )

        recorded = [ controller.CallBlockingToQt( gui, lambda: state( c, 'start', [], None ) ) ]

        for ( do, rows, argument, step_answers ) in c[ 'steps' ]:

            controller.CallBlockingToQt( gui, lambda: step( c, do, rows, argument, step_answers ) )

            time.sleep( 1 )

            recorded.append( controller.CallBlockingToQt( gui, lambda: state( c, do, rows, argument ) ) )


        sidebar = c[ 'sidebar' ]

        result[ kind ] = {
            'page_options' : controller.CallBlockingToQt( gui, lambda: sidebar._file_limit.GetValue() if kind == 'gallery' else as_list( sidebar._checker_options.GetValue() ) ),
            'steps' : recorded,
        }


    return result


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

        path = os.path.join( work, 'downloader_lists.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "gallery" ][ "steps" ] )} gallery steps, {len( result[ "watcher" ][ "steps" ] )} watcher steps' )


if __name__ == '__main__':

    main()
