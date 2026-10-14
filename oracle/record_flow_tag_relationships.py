#!/usr/bin/env python3
"""Flow: tag relationships reaching search.

On the `basic` fixture (no site; the flow is all local):

1. "siblings": tags > manage tag siblings (the real `ManageTagSiblings`
   dialog): on "my tags", `doggo` entered as the tag to replace and `dog` as
   the ideal, added to the list, OK.
2. "parents": tags > manage tag parents: child `dog`, parent `animal`,
   added, OK.
3. "tag": a search page of every file; the first file selected and
   `doggo` added in manage tags, the second file selected and `dog` added.
   Then the client's tag display maintenance is run to the end (the reference
   does it in the background; a user waits for it).
4. "search": a search for `dog`, then `animal`, then `doggo` on search pages.

After each step: what the dialogs showed (the tabs, the sync status, the
list of pairs), each of the two files' tags as stored and as displayed,
the search page's results, the tag list of its sidebar for all its files
(the reference's own rows, with sibling and parent display), and the
autocomplete's rows (with counts) for what a user types.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_flow_tag_relationships.py
       (writes oracle/fixtures/flow_tag_relationships.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

import flows

OUT = os.path.join( HERE, 'fixtures', 'flow_tag_relationships.json' )


def record( session ):

    from qtpy import QtWidgets as QW
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.metadata import ClientTags
    from hydrus.client.search import ClientSearchPredicate as P
    from hydrus.core import HydrusConstants as HC

    controller = session.controller
    gui = controller.gui
    services = controller.services_manager

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )
    # (the first two of the fixture's files a search page lists: some, such as
    # a zip, are not shown)
    manifest_hashes = [ f[ 'hash' ] for f in manifest[ 'files' ] ]

    local = next( s.GetServiceKey() for s in services.GetServices( ( HC.LOCAL_TAG, ) ) if s.GetName() == 'my tags' )
    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )

    steps = []

    def pump( condition, what, timeout = 30 ):
        """Let the Qt event loop run (on the Qt thread) until `condition`."""

        deadline = time.time() + timeout

        while not condition():

            if time.time() > deadline:

                raise Exception( 'timed out: ' + what )


            QW.QApplication.processEvents()
            time.sleep( 0.02 )



    def tags_of( hash_hex ):

        media_result = session.read( 'media_result', bytes.fromhex( hash_hex ) )
        tags_manager = media_result.GetTagsManager()

        return {
            'stored' : sorted( tags_manager.GetCurrent( local, ClientTags.TAG_DISPLAY_STORAGE ) ),
            'displayed' : sorted( tags_manager.GetCurrent( local, ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL ) ),
        }


    def pairs_dialog( panel_name ):
        """A siblings or parents dialog's script: the page for "my tags"
        made current, `enter` run on it, the pair added, and what showed."""

        def script_for( enter, table_attr, pertinent, seen ):

            def script( dlg, panel ):

                tabs = panel._tag_services
                page = next( tabs.widget( i ) for i in range( tabs.count() ) if tabs.widget( i ).GetServiceKey() == local )

                tabs.setCurrentWidget( page )

                # (the dialog's status line is read by a worker as it opens)
                pump( lambda: page._sync_status_st.text() not in ( '', 'initialising sync data\u2026' ), 'the dialog\'s sync status' )

                seen[ 'title' ] = dlg.windowTitle()
                seen[ 'services' ] = [ tabs.tabText( i ) for i in range( tabs.count() ) ]
                seen[ 'sync status' ] = page._sync_status_st.text()

                enter( page )

                # (a user pauses before pressing "add": the list has caught up
                # with the tags entered, and keeps their pairs in view)
                pump( lambda: pertinent <= page._current_pertinent_tags, 'the entered tags in the workspace' )

                page._AddButton()

                table = getattr( page, table_attr )

                pump( lambda: len( table.GetData() ) > 0, 'the pair in the list' )

                seen[ 'rows' ] = [ list( page._ConvertPairToDisplayTuple( pair ) ) for pair in table.GetData() ]

                return True

            return script

        return script_for


    def run_dialog( panel_class, script, method ):

        with flows.scripted_dialogs( { panel_class : [ script ] } ) as dialogs:

            qt( method )


    def sync_tag_display():
        """Tag display maintenance run to the end (the reference does it in the background when idle)."""

        for _ in range( 60 ):

            if not controller.WriteSynchronous( 'sync_tag_display_maintenance', local, 1.0 ):

                return


        raise Exception( 'tag display maintenance did not finish' )


    def step( name, seen, extra = None ):

        steps.append( {
            'step' : name,
            'dialog' : seen,
            'files' : { h : tags_of( h ) for h in ( first, second ) },
            **( extra or {} ),
        } )


    MINE = ( 'dog', 'doggo', 'animal' )

    def tag_lists( page ):
        """All the page's files selected, and the sidebar's tag list for them
        (only the rows of this flow's tags: the fixture's own are not its
        business), as the list box words them."""

        from hydrus.client.media import ClientMediaFileFilter

        panel = qt( page.GetMediaResultsPanel )

        qt( lambda: panel._Select( ClientMediaFileFilter.FileFilter( ClientMediaFileFilter.FILE_FILTER_ALL ) ) )

        box = page.GetSidebar()._current_selection_tags_list

        def rows():

            out = []

            for term in box._ordered_terms:

                if term.GetTag() in MINE:

                    out.append( { 'tag' : term.GetTag(), 'rows' : [ ''.join( text for ( text, colour ) in row ) for row in box._GetRowsOfTextsAndColours( term ) ] } )



            return sorted( out, key = lambda r: r[ 'tag' ] )


        pump_off_thread( lambda: qt( lambda: box._ordered_terms is not None and len( box._ordered_terms ) > 0 ), 'the sidebar tag list' )

        time.sleep( 1.0 )

        return { 'hashes' : sorted( m.GetHash().hex() for m in qt( panel.GetSortedMedia ) ), 'sidebar' : qt( rows ) }


    # 3. tag -------------------------------------------------------------

    everything = P.Predicate( P.PREDICATE_TYPE_SYSTEM_EVERYTHING )

    def new_page( predicates ):

        page = qt( lambda: gui._notebook.NewPageQuery( location, initial_predicates = predicates, do_sort = False ) )

        # (loaded: its thumbnails there, and still)
        last = [ None, 0 ]

        def loaded():

            panel = qt( page.GetMediaResultsPanel )

            if not hasattr( panel, '_sorted_media' ):

                return False


            n = len( panel._sorted_media )

            if n != last[0]:

                last[0] = n
                last[1] = time.time()

                return False


            return n > 0 and time.time() - last[1] > 1.0


        pump_off_thread( loaded, 'the page to load' )

        return page


    def pump_off_thread( condition, what, timeout = 60 ):

        deadline = time.time() + timeout

        while not condition():

            if time.time() > deadline:

                raise Exception( 'timed out: ' + what )


            time.sleep( 0.1 )



    page = new_page( [ everything ] )

    listed = { m.GetHash().hex() for m in qt( lambda: page.GetMediaResultsPanel().GetSortedMedia() ) }
    ( first, second ) = [ h for h in manifest_hashes if h in listed ][ : 2 ]

    # 1. siblings --------------------------------------------------------

    seen = {}

    def enter_sibling( page ):

        page.EnterOlds( { 'doggo' } )
        page.SetNew( { 'dog' } )


    run_dialog( 'ManageTagSiblings', pairs_dialog( 'siblings' )( enter_sibling, '_tag_siblings', { 'doggo', 'dog' }, seen ), lambda: gui._ManageTagSiblings() )

    sync_tag_display()

    step( 'siblings', seen )

    # 2. parents ---------------------------------------------------------

    seen = {}

    def enter_parent( page ):

        page.EnterChildren( { 'dog' } )
        page.EnterParents( { 'animal' } )


    run_dialog( 'ManageTagParents', pairs_dialog( 'parents' )( enter_parent, '_tag_parents', { 'dog', 'animal' }, seen ), lambda: gui._ManageTagParents() )

    sync_tag_display()

    step( 'parents', seen )

    def media_panel():

        return page.GetMediaResultsPanel()


    def select_only( hash_hex ):

        panel = media_panel()

        media = next( m for m in panel.GetSortedMedia() if m.GetHash().hex() == hash_hex )

        panel._HitMedia( media, False, False )


    def manage_tags_script( tag, seen ):

        def script( dlg, panel ):

            tabs = panel._tag_services
            mine = next( tabs.widget( i ) for i in range( tabs.count() ) if tabs.widget( i ).GetServiceKey() == local )

            tabs.setCurrentWidget( mine )

            mine.EnterTags( { tag } )

            return True

        return script


    for ( hash_hex, tag ) in ( ( first, 'doggo' ), ( second, 'dog' ) ):

        qt( lambda: select_only( hash_hex ) )

        run_dialog( 'ManageTagsPanel', manage_tags_script( tag, {} ), lambda: media_panel()._ManageTags() )


    flows.wait_until_idle( controller )

    sync_tag_display()

    lists = tag_lists( page )

    # (the page's own files are the fixture's: only the sidebar is the flow's)
    step( 'tag', {}, { 'sidebar' : lists[ 'sidebar' ] } )

    # 4. search ----------------------------------------------------------

    def tag_predicate( text ):

        return P.Predicate( P.PREDICATE_TYPE_TAG, value = text )


    def autocomplete( page, text ):
        """What the search page's dropdown lists for `text` typed, as it words
        the rows, and the counts."""

        ac = page.GetSidebar()._tag_autocomplete

        qt( lambda: ac._text_ctrl.setText( text ) )

        # (the dropdown fetches after a pause, in a thread; a sibling's old tag
        # lists its ideal, so the rows can't be waited for by their text)
        time.sleep( 3.0 )

        def rows():

            box = ac._search_results_list

            return [ { 'predicate' : t.GetPredicate().ToString( with_count = True ), 'rows' : [ ''.join( x[0] for x in row ) for row in t.GetRowsOfPresentationTextsWithNamespaces( True, box._show_sibling_decorators, ' → ', None, box._show_parent_decorators, box._show_parent_decorators and box._extra_parent_rows_allowed ) ] } for t in box._ordered_terms if any( word in t.GetPredicate().ToString( with_count = False ) for word in MINE ) ]


        return qt( rows )


    searches = {}

    for text in ( 'dog', 'animal', 'doggo' ):

        searched = new_page( [ tag_predicate( text ) ] )

        searches[ text ] = tag_lists( searched )


    typed = {}

    # (of more than the two characters up to which the dropdown only matches exactly)
    for text in ( 'dog', 'doggo', 'ani' ):

        typed[ text ] = autocomplete( searched, text )


    step( 'search', {}, { 'searches' : searches, 'autocomplete' : typed } )

    return { 'tagged' : [ first, second ], 'steps' : steps }


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
