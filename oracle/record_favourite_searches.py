#!/usr/bin/env python3
"""Record the reference's favourite searches: the star button's menu, saving
the page's search, and the dialogs that manage them.

In the running client, on the `basic` fixture (whose one favourite search,
"inbox filter" in the folder "example search", is the reference's default),
a search page's autocomplete has a star button. This records:

* `menu`: the star button's menu as it builds it (each entry's text, a
  submenu's entries under it, separators as "---"), with the fixture's
  favourites, and `nested_menu` with `rows` set as the favourites
* `saved`: what "save this search" hands the manage dialog, from a page
  searching `page_predicates`, with its sort and collect
* `list`: the manage dialog's list of `rows`, as it shows them, in order
  (folder, name, the search's predicates, sort, collect)
* `added`: searches added in turn through the dialog (the saved one
  twice, "add" with no row twice, and one named as a search in another
  folder is), what the edit dialog starts with, and the list after
* `edits`: the edit dialog's own checks: renaming onto a search that
  exists asks whether to overwrite it (`asked`, and whether it lets the
  edit through when told no)
* `values`: what the edit dialog gives back for some folder texts and
  ticks

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_favourite_searches.py
       (writes fixtures/favourite_searches.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'favourite_searches.json' )

# the favourites set for the nested menu and the manage dialog: (folder,
# name, sort, collect), each searching the fixture's favourite's search; a
# sort is ( 'system', the CC sort type's name, ascending ), ( 'namespaces',
# namespaces, ascending ) or ( 'rating', service name, ascending ), and a
# collect ( namespaces, rating service names )
ROWS = [
    ( None, 'zebra', None, None ),
    ( None, 'apple', ( 'system', 'SORT_FILES_BY_IMPORT_TIME', False ), ( [ 'creator' ], [] ) ),
    ( None, 'Banana', ( 'namespaces', [ 'series', 'creator' ], True ), ( [ 'series', 'creator' ], [ 'favourites' ] ) ),
    ( 'art', 'paintings', ( 'rating', 'stars', False ), ( [], [ 'stars' ] ) ),
    ( 'art', 'Zed', ( 'system', 'SORT_FILES_BY_WIDTH', True ), ( [], [] ) ),
    ( 'art', 'apple', None, None ),
    ( 'art/sketches', 'pencil', None, None ),
    ( 'art/sketches/old', 'charcoal', None, None ),
    ( '/art/', 'colour', None, None ),
    ( 'Beta', 'b', None, None ),
    ( 'beta', 'a', None, None ),
    ( 'example search', 'inbox filter', None, None ),
]

# the tags the saving page searches, after system:inbox
PAGE_TAGS = [ 'blue eyes', 'series:metroid' ]


def entries( menu ):

    out = []

    for action in menu.actions():

        if action.isSeparator():

            out.append( '---' )

        elif action.menu() is not None:

            out.append( { 'text' : action.text(), 'submenu' : entries( action.menu() ) } )

        else:

            out.append( { 'text' : action.text() } )


    return out


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUICore as CGC
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.gui.search import ClientGUISearchPanels
    from hydrus.client.search import ClientSearchPredicate

    controller = session.controller
    gui = controller.gui
    manager = controller.favourite_search_manager

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    # menus are captured rather than shown
    captured = []

    CGC.core().PopupMenu = lambda window, menu: captured.append( menu )

    def my_files():

        return ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )


    def open_page( predicates = None ):

        page = qt( lambda: gui._notebook.NewPageQuery( my_files(), initial_predicates = predicates ) )

        for _ in range( 600 ):

            sidebar = qt( lambda: page.GetSidebar() )

            if sidebar is not None and hasattr( sidebar, '_tag_autocomplete' ):

                return sidebar._tag_autocomplete


            time.sleep( 0.05 )


        raise Exception( 'the page never loaded' )


    def menu_now( ac ):

        captured.clear()

        ac._FavouriteSearchesMenu()

        return entries( captured[-1] )


    def describe_row( row ):

        ( folder, name, file_search_context, synchronised, media_sort, media_collect ) = row

        return {
            'folder' : folder,
            'name' : name,
            'location' : file_search_context.GetLocationContext().ToString( controller.services_manager.GetName ),
            'predicates' : [ p.ToString() for p in file_search_context.GetPredicates() ],
            'synchronised' : synchronised,
            'sort' : None if media_sort is None else media_sort.ToString(),
            'collect' : None if media_collect is None else media_collect.ToString(),
        }


    out = {}

    original_rows = manager.GetFavouriteSearchRows()

    ac = open_page()

    out[ 'menu' ] = qt( lambda: menu_now( ac ) )

    ( _, _, fixture_search, _, fixture_sort, fixture_collect ) = original_rows[0]

    from hydrus.core import HydrusConstants as HC
    from hydrus.client.media import ClientMediaCollect
    from hydrus.client.media import ClientMediaSort
    from hydrus.client.metadata import ClientTags

    ratings = { s.GetName() : s.GetServiceKey() for s in controller.services_manager.GetServices( HC.RATINGS_SERVICES ) }

    def make_sort( spec ):

        if spec is None:

            return None


        ( kind, data, asc ) = spec

        order = CC.SORT_ASC if asc else CC.SORT_DESC

        if kind == 'system':

            sort_type = ( 'system', getattr( CC, data ) )

        elif kind == 'namespaces':

            sort_type = ( 'namespaces', ( data, ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL ) )

        else:

            sort_type = ( 'rating', ratings[ data ] )


        return ClientMediaSort.MediaSort( sort_type = sort_type, sort_order = order )


    def make_collect( spec ):

        if spec is None:

            return None


        ( namespaces, rating_names ) = spec

        return ClientMediaCollect.MediaCollect( namespaces = namespaces, rating_service_keys = [ ratings[ n ] for n in rating_names ] )


    rows = [ ( folder, name, fixture_search, True, make_sort( sort ), make_collect( collect ) ) for ( folder, name, sort, collect ) in ROWS ]

    out[ 'rows' ] = [ list( r ) for r in ROWS ]

    manager.SetFavouriteSearchRows( rows )

    out[ 'nested_menu' ] = qt( lambda: menu_now( ac ) )

    # "save this search": what it hands the manage dialog
    predicates = [ ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_INBOX ) ]
    predicates.extend( ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, value = tag ) for tag in PAGE_TAGS )

    saving = open_page( predicates )

    saved_rows = []

    def save():

        saving._ManageFavouriteSearches = lambda favourite_search_row_to_save = None: saved_rows.append( favourite_search_row_to_save )

        saving._SaveFavouriteSearch()


    qt( save )

    saved_row = saved_rows[0]

    out[ 'page_predicates' ] = [ p.ToString() for p in predicates ]
    out[ 'saved' ] = describe_row( saved_row )

    # the manage dialog's list
    def list_of( panel ):

        view = panel._favourite_searches
        model = view.model()

        return [
            [ model.data( model.index( r, c ), QC.Qt.ItemDataRole.DisplayRole ) for c in range( model.columnCount() ) ]
            for r in range( model.rowCount() )
        ]


    def manage_dialog():

        dlg = ClientGUITopLevelWindowsPanels.DialogEdit( gui, 'edit favourite searches' )

        panel = ClientGUISearchPanels.EditFavouriteSearchesPanel( dlg, manager.GetFavouriteSearchRows() )

        dlg.SetPanel( panel )

        return ( dlg, panel )


    out[ 'list' ] = qt( lambda: list_of( manage_dialog()[1] ) )

    # adding through the dialog: the edit dialog's start, accepted as it is
    started = []

    def fake_exec( dlg ):

        panel = dlg._panel

        started.append( {
            'folder' : panel._foldername.text(),
            'name' : panel._name.text(),
            'save_sort' : panel._include_media_sort.isChecked(),
            'save_collect' : panel._include_media_collect.isChecked(),
            'value' : describe_row( panel.GetValue() ),
        } )

        return QW.QDialog.DialogCode.Accepted


    ClientGUITopLevelWindowsPanels.DialogEdit.exec = fake_exec

    def additions():

        ( dlg, panel ) = manage_dialog()

        results = []

        # (a name another folder has is taken too)
        paintings = ( None, 'paintings', fixture_search, True, None, None )

        for ( label, row ) in [ ( 'saved', saved_row ), ( 'saved', saved_row ), ( 'add', None ), ( 'add', None ), ( 'paintings', paintings ) ]:

            started.clear()

            panel._AddNewFavouriteSearch( row )

            results.append( { 'what' : label, 'started' : started[0], 'list' : list_of( panel ) } )


        return results


    out[ 'added' ] = qt( additions )

    # the edit dialog's overwrite question
    asked = []

    def edits():

        results = []

        existing = { None : [ 'zebra', 'apple' ], 'art' : [ 'paintings' ] }

        for ( original, renamed, answer ) in [
            ( ( 'art', 'pencil' ), ( 'art', 'paintings' ), False ),
            ( ( 'art', 'pencil' ), ( 'art', 'paintings' ), True ),
            ( ( 'art', 'pencil' ), ( '', 'zebra' ), False ),
            ( ( 'art', 'pencil' ), ( 'art', 'new name' ), False ),
            ( ( 'art', 'paintings' ), ( 'art', 'paintings' ), False ),
            ( ( None, 'zebra' ), ( 'Art', 'paintings' ), False ),
        ]:

            asked.clear()

            def get_yes_no( win, message, *args, **kwargs ):

                asked.append( message )

                return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


            ClientGUIDialogsQuick.GetYesNo = get_yes_no

            dlg = ClientGUITopLevelWindowsPanels.DialogEdit( gui, 'edit favourite search' )

            panel = ClientGUISearchPanels.EditFavouriteSearchPanel( dlg, existing, original[0], original[1], fixture_search, True, None, None )

            panel._foldername.setText( renamed[0] )
            panel._name.setText( renamed[1] )

            ok = panel.UserIsOKToOK()

            results.append( { 'original' : list( original ), 'renamed' : list( renamed ), 'answer' : answer, 'asked' : list( asked ), 'ok' : ok } )


        return results


    out[ 'edits' ] = qt( edits )

    # what the edit dialog gives back
    def values():

        results = []

        for ( folder, save_sort, save_collect ) in [ ( '', False, False ), ( 'a/b', True, False ), ( ' spaced ', False, True ) ]:

            dlg = ClientGUITopLevelWindowsPanels.DialogEdit( gui, 'edit favourite search' )

            panel = ClientGUISearchPanels.EditFavouriteSearchPanel( dlg, {}, None, 'name', fixture_search, False, fixture_sort, fixture_collect )

            panel._foldername.setText( folder )
            panel._include_media_sort.setChecked( save_sort )
            panel._include_media_collect.setChecked( save_collect )

            results.append( { 'folder' : folder, 'save_sort' : save_sort, 'save_collect' : save_collect, 'value' : describe_row( panel.GetValue() ) } )


        return results


    out[ 'values' ] = qt( values )

    manager.SetFavouriteSearchRows( original_rows )

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
