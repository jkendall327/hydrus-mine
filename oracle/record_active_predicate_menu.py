#!/usr/bin/env python3
"""Record the whole right-click menu of a search page's active predicate list.

The real `ListBoxTagsActiveSearchPredicates` of a new search page, with tags
that have siblings and parents on "my tags" and favourite and most-used tags
set: for each selection, the menu as built by `ShowMenuFromSignal` once its
sibling/parent lookup has come back (labels, separators and submenus, in
order). Then the entries it inherits from the tag lists are triggered: adding
and removing a favourite or most-used tag (the question answered yes), the
siblings and parents dialogs it opens (recorded by title and the tags they
are given, then closed), regenerating tag display (the question recorded and
answered yes, the database write captured), and selecting files with the
tags (which does nothing on this list).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_active_predicate_menu.py
       (writes fixtures/active_predicate_menu.json)
"""
import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'active_predicate_menu.json' )

CORPUS = [ ( 'series:active alpha', 3 ), ( 'series:active beta', 1 ), ( 'series:active root', 2 ) ]
SIBLINGS = [ [ 'series:active old', 'series:active alpha' ], [ 'series:active older', 'series:active alpha' ] ]
PARENTS = [ [ 'series:active alpha', 'series:active root' ], [ 'series:active kid', 'series:active alpha' ] ]
FAVOURITES = [ 'series:active beta' ]


def record( session ):

    from qtpy import QtCore as QC
    from qtpy import QtWidgets as QW

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui import ClientGUIAsync
    from hydrus.client.gui import ClientGUICore as Core
    from hydrus.client.gui import ClientGUIDialogsQuick
    from hydrus.client.gui import ClientGUITopLevelWindowsPanels
    from hydrus.client.metadata import ClientContentUpdates as U
    from hydrus.client.search import ClientSearchFileSearchContext as F
    from hydrus.client.search import ClientSearchPredicate as P
    from hydrus.client.search import ClientSearchTagContext as T
    from hydrus.core import HydrusConstants as HC

    c = session.controller
    gui = c.gui
    qt = lambda fn: c.CallBlockingToQt( gui, fn )

    service = next( s.GetServiceKey() for s in c.services_manager.GetServices( ( HC.LOCAL_TAG, ) ) if s.GetName() == 'my tags' )

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ][ :4 ] ]

    updates = [ U.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ( tag, set( hashes[ :n ] ) ) ) for ( tag, n ) in CORPUS ]
    updates.extend( U.ContentUpdate( HC.CONTENT_TYPE_TAG_SIBLINGS, HC.CONTENT_UPDATE_ADD, tuple( pair ) ) for pair in SIBLINGS )
    updates.extend( U.ContentUpdate( HC.CONTENT_TYPE_TAG_PARENTS, HC.CONTENT_UPDATE_ADD, tuple( pair ) ) for pair in PARENTS )

    c.WriteSynchronous( 'content_updates', U.ContentUpdatePackage.STATICCreateFromContentUpdates( service, updates ) )

    for _ in range( 12 ):

        if not c.WriteSynchronous( 'sync_tag_display_maintenance', service, .5 ):

            break


    c.new_options.SetStringList( 'favourite_tags', FAVOURITES )
    c.new_options.SetSuggestedTagsMostUsed( service, [ 'series:active alpha' ] )

    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )
    context = F.FileSearchContext( location_context = location, tag_context = T.TagContext() )

    page = qt( lambda: gui._notebook.NewPageQueryFileSearchContext( context, page_name = 'active menu' ) )

    for _ in range( 500 ):

        sidebar = qt( page.GetSidebar )

        if sidebar is not None and hasattr( sidebar, '_tag_autocomplete' ):

            break

        time.sleep( .01 )


    ac = sidebar._tag_autocomplete
    box = ac._predicates_listbox

    tag = lambda name: P.Predicate( P.PREDICATE_TYPE_TAG, name )
    alpha, beta, root = tag( 'series:active alpha' ), tag( 'series:active beta' ), tag( 'series:active root' )
    inbox = P.Predicate( P.PREDICATE_TYPE_SYSTEM_INBOX )
    group = P.Predicate( P.PREDICATE_TYPE_OR_CONTAINER, [ alpha, beta ] )
    namespace = P.Predicate( P.PREDICATE_TYPE_NAMESPACE, 'series' )

    current = [ alpha, beta, root, inbox, group, namespace ]

    cases = [
        ( 'tag with siblings and parents', [ alpha ] ),
        ( 'favourite tag', [ beta ] ),
        ( 'parent tag', [ root ] ),
        ( 'two tags', [ alpha, beta ] ),
        ( 'system predicate', [ inbox ] ),
        ( 'OR predicate', [ group ] ),
        ( 'namespace', [ namespace ] ),
        ( 'tag and system predicate', [ alpha, inbox ] )
    ]

    menus = []
    questions = []
    dialogs = []
    writes = []

    old_popup, old_yes_no, old_manage, old_write = Core.core().PopupMenu, ClientGUIDialogsQuick.GetYesNo, ClientGUITopLevelWindowsPanels.DialogManage, c.Write
    old_start = ClientGUIAsync.AsyncQtJob.start

    Core.core().PopupMenu = lambda _widget, menu: menus.append( menu )

    def yes_no( win, message, title = 'Are you sure?', yes_label = 'yes', no_label = 'no', **kwargs ):

        questions.append( { 'message' : message, 'title' : title, 'yes' : yes_label, 'no' : no_label } )

        return QW.QDialog.DialogCode.Accepted


    ClientGUIDialogsQuick.GetYesNo = yes_no

    class Manage( QW.QDialog ):

        def __init__( self, parent, title, *args, **kwargs ):

            super().__init__( parent )

            self._title = title

        def __enter__( self ):

            return self

        def __exit__( self, *args ):

            self.deleteLater()

            return False

        def SetPanel( self, panel ):

            dialogs.append( { 'title' : self._title, 'panel' : type( panel ).__name__ } )

        def exec( self ):

            return QW.QDialog.DialogCode.Rejected


    ClientGUITopLevelWindowsPanels.DialogManage = Manage

    def write( action, *args, **kwargs ):

        if action == 'regenerate_tag_mappings_tags':

            writes.append( { 'action' : action, 'tags' : sorted( args[ 0 ] ) } )

            return


        return old_write( action, *args, **kwargs )


    c.Write = write

    old_pub = c.pub

    clipboard = []

    def pub( topic, *args, **kwargs ):

        if topic == 'clipboard':

            clipboard.append( { 'type' : args[ 0 ], 'text' : args[ 1 ] } )

        else:

            old_pub( topic, *args, **kwargs )


    c.pub = pub

    # (the sibling/parent lookup runs where we stand, so the menu is complete)
    ClientGUIAsync.AsyncQtJob.start = lambda job: job._publish_callable( job._work_callable() )

    def tree( menu ):

        out = []

        for action in menu.actions():

            if action.isSeparator():

                out.append( '---' )

            elif action.menu() is not None:

                out.append( { action.menu().title().replace( '&&', '&' ) or action.text().replace( '&&', '&' ) : tree( action.menu() ) } )

            else:

                text = action.text().replace( '&&', '&' )

                out.append( text if action.isEnabled() else [ 'label', text ] )


        return out


    def find( menu, path ):

        for action in menu.actions():

            text = ( action.menu().title() if action.menu() is not None else action.text() ).replace( '&&', '&' )

            if text == path[ 0 ]:

                return action if len( path ) == 1 else find( action.menu(), path[ 1: ] )


        raise Exception( 'no menu entry ' + repr( path ) )


    def show( selected ):

        ctx = context.Duplicate()
        ctx.SetPredicates( current )
        ac.SetFileSearchContext( ctx )
        ac.SetSynchronised( False )

        box._selected_terms = { box._GenerateTermFromPredicate( p ) for p in selected }

        menus.clear()

        box.ShowMenuFromSignal( QC.QPoint( 1, 1 ) )

        return menus[ -1 ]


    triggers = [
        ( 'favourite tag', [ 'favourites', 'remove "series:active beta" from favourites' ] ),
        ( 'tag with siblings and parents', [ 'favourites', 'add "series:active alpha" to favourites' ] ),
        ( 'tag with siblings and parents', [ 'favourites', 'remove "series:active alpha" from most used for "my tags"' ] ),
        ( 'parent tag', [ 'favourites', 'add "series:active root" to most used for "my tags"' ] ),
        ( 'tag with siblings and parents', [ '2 siblings', 'add siblings to series:active alpha' ] ),
        ( 'tag with siblings and parents', [ '2 siblings', 'series:active old' ] ),
        ( 'tag with siblings and parents', [ '1 parents, 1 children', 'child: series:active kid' ] ),
        ( 'two tags', [ 'parents', 'add parents to selection' ] ),
        ( 'two tags', [ 'maintenance', 'regenerate tag display' ] ),
        ( 'tag with siblings and parents', [ 'select', 'files with "series:active alpha"' ] )
    ]

    def replay():

        out = { 'cases' : [], 'triggers' : [] }

        for ( name, selected ) in cases:

            menu = show( selected )

            out[ 'cases' ].append( { 'name' : name, 'selected' : [ p.ToString() for p in selected ], 'menu' : tree( menu ) } )


        for ( name, path ) in triggers:

            selected = dict( cases )[ name ]

            menu = show( selected )

            del questions[:]
            del dialogs[:]
            del writes[:]
            del clipboard[:]

            find( menu, path ).trigger()

            QW.QApplication.processEvents()

            out[ 'triggers' ].append( {
                'case' : name,
                'path' : path,
                'questions' : list( questions ),
                'dialogs' : list( dialogs ),
                'writes' : list( writes ),
                'clipboard' : list( clipboard ),
                'favourite_tags' : list( c.new_options.GetStringList( 'favourite_tags' ) ),
                'most_used' : list( c.new_options.GetSuggestedTagsMostUsed( service ) ),
                'shown' : [ term.GetPredicate().ToString() for term in box._ordered_terms ],
                'menu_after' : tree( show( selected ) )
            } )


        return out


    try:

        result = qt( replay )

    finally:

        Core.core().PopupMenu, ClientGUIDialogsQuick.GetYesNo, ClientGUITopLevelWindowsPanels.DialogManage, c.Write = old_popup, old_yes_no, old_manage, old_write
        c.pub = old_pub
        ClientGUIAsync.AsyncQtJob.start = old_start


    result.update( { 'corpus' : [ { 'tag' : tag, 'hashes' : [ h.hex() for h in hashes[ :n ] ] } for ( tag, n ) in CORPUS ], 'siblings' : SIBLINGS, 'parents' : PARENTS, 'favourites' : FAVOURITES, 'most_used' : [ 'series:active alpha' ] } )

    return result


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
