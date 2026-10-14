#!/usr/bin/env python3
"""Record the icon buttons of the reference's favourite search editor.

`EditFavouriteSearchPanel` (its autocomplete is built with
`hide_favourites_edit_actions`) is given: the paste button with lines on the
clipboard, the empty search button, and the star menu (its entries as drawn,
then each loaded). After each step the predicates the editor would save, its
sort, collect, whether it searches as it changes, its file domain and tag
service flags, and any message shown are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_favourite_editor_buttons.py
       (writes fixtures/favourite_editor_buttons.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'favourite_editor_buttons.json' )

# ( folder, name, tags, searches as it changes, sort ( CC name, ascending ) or None, collect namespaces or None )
ROWS = [
    ( None, 'by size', [ 'blue eyes' ], True, ( 'SORT_FILES_BY_FILESIZE', False ), [ 'creator' ] ),
    ( None, 'paused', [ 'series:metroid', 'samus' ], False, None, None ),
    ( 'work', 'in a folder', [ 'character:samus aran' ], True, ( 'SORT_FILES_BY_WIDTH', True ), None ),
    ( 'work/deeper', 'nested', [ '-bad tag' ], True, None, [] ),
]

STEPS = [
    [ 'initial' ],
    [ 'paste', 'series:metroid\nsystem:everything\n-creator:samus aran\nblue eyes\n\n  spaced   tag  \nsam*' ],
    [ 'paste', 'system:nonsense words' ],
    [ 'paste', '' ],
    [ 'paste', 'system:limit = 64\nzz*' ],
    [ 'status', False, False ],
    [ 'empty search' ],
    [ 'menu' ],
    [ 'load', None, 'by size' ],
    [ 'load', None, 'paused' ],
    [ 'load', 'work', 'in a folder' ],
    [ 'load', 'work/deeper', 'nested' ],
    [ 'empty search' ],
]


def record( session ):

    from qtpy import QtTest
    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUICore
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui.search import ClientGUISearchPanels
    from hydrus.client.media import ClientMediaCollect
    from hydrus.client.media import ClientMediaSort
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchPredicate

    controller = session.controller
    gui = controller.gui

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    rows = []

    for ( folder, name, tags, sync, sort, collect ) in ROWS:

        media_sort = None if sort is None else ClientMediaSort.MediaSort( sort_type = ( 'system', getattr( CC, sort[ 0 ] ) ), sort_order = CC.SORT_ASC if sort[ 1 ] else CC.SORT_DESC )
        media_collect = None if collect is None else ClientMediaCollect.MediaCollect( namespaces = collect )

        predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, t[ 1: ] if t.startswith( '-' ) else t, not t.startswith( '-' ) ) for t in tags ]

        rows.append( ( folder, name, ClientSearchFileSearchContext.FileSearchContext( location_context = location, predicates = predicates ), sync, media_sort, media_collect ) )


    controller.favourite_search_manager.SetFavouriteSearchRows( rows )

    def drive():

        controller.new_options.SetBoolean( 'advanced_mode', True )

        search = ClientSearchFileSearchContext.FileSearchContext(
            location_context = location,
            predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, 'blue eyes' ) ]
        )

        dlg = QW.QDialog( gui )

        panel = ClientGUISearchPanels.EditFavouriteSearchPanel( dlg, {}, None, 'recorded', search, True, None, None )

        layout = QW.QVBoxLayout( dlg )
        layout.addWidget( panel )

        dlg.resize( 1000, 700 )
        dlg.show()

        QW.QApplication.processEvents()

        messages = []

        ClientGUIDialogsMessage.ShowInformation = lambda win, message: messages.append( [ 'information', message ] )
        ClientGUIDialogsMessage.ShowWarning = lambda win, message: messages.append( [ 'warning', message ] )
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: messages.append( [ 'critical', title, message ] )
        ClientGUIDialogsQuick.PresentClipboardParseError = lambda win, raw_text, kind, e: messages.append( [ 'parse error', kind ] )

        ac = panel._tag_autocomplete

        menus = []

        core = ClientGUICore.core()

        core.PopupMenu = lambda window, menu: menus.append( menu )

        def describe( menu ):

            out = []

            for action in menu.actions():

                if action.isSeparator():

                    out.append( '---' )

                elif action.menu() is not None:

                    out.append( [ action.text(), describe( action.menu() ) ] )

                else:

                    out.append( action.text() )


            return out


        def state():

            fsc, synchronised, media_sort, media_collect = panel.GetValue()[ 2: 6 ]

            return {
                'predicates' : sorted( p.ToString( with_count = False ) for p in fsc.GetPredicates() ),
                'shown' : [ term.GetPredicate().ToString( with_count = False ) for term in ac._predicates_listbox._ordered_terms ],
                'synchronised' : synchronised,
                'sort' : None if media_sort is None else media_sort.ToString(),
                'collect' : None if media_collect is None else media_collect.ToString(),
                'sort_widget' : panel._media_sort.GetSort().ToString(),
                'collect_widget' : panel._media_collect.GetValue().ToString(),
                'location' : fsc.GetLocationContext().ToString( controller.services_manager.GetName ),
                'include_current' : fsc.GetTagContext().include_current_tags,
                'include_pending' : fsc.GetTagContext().include_pending_tags,
                'tag_service' : controller.services_manager.GetName( fsc.GetTagContext().service_key )
            }


        out = []

        for step in STEPS:

            del messages[:]

            extra = {}

            if step[ 0 ] == 'paste':

                QW.QApplication.clipboard().setText( step[ 1 ] )

                ac._Paste()

            elif step[ 0 ] == 'status':

                ac._include_current_tags.SetOnOff( step[ 1 ] )
                ac._include_pending_tags.SetOnOff( step[ 2 ] )

            elif step[ 0 ] == 'empty search':

                ac._empty_search_button.click()

            elif step[ 0 ] == 'menu':

                del menus[:]

                ac._favourite_searches_button.click()

                extra[ 'menu' ] = describe( menus[ 0 ] )

            elif step[ 0 ] == 'load':

                ac._LoadFavouriteSearch( step[ 1 ], step[ 2 ] )


            QW.QApplication.processEvents()

            QtTest.QTest.qWait( 100 )

            entry = { 'step' : step, 'messages' : list( messages ), 'state' : state() }

            entry.update( extra )

            out.append( entry )


        dlg.reject()

        return { 'rows' : [ list( r ) for r in ROWS ], 'steps' : out }


    return controller.CallBlockingToQt( gui, drive )


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
