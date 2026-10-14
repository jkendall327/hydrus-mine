#!/usr/bin/env python3
"""Record the reference's undo > undo / redo content operation.

In the running client, on the `basic` fixture, a script of content writes is
run through `controller.Write( 'content_updates', ... )` (which hands the
package to the UndoManager, as every GUI write does), interleaved with Undo and
Redo (what the menu items' `undo` / `redo` pubsub calls). After each step the
manager's `GetUndoRedoStrings`, the Undo menu items' visibility and label (as
`_InitialiseMenuInfoUndo` / `_UpdateMenuUndo` set them) and, for the first
three files, whether each is in the inbox and which of the recorded tags each
has in "my tags" are recorded.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_content_undo.py
       (writes fixtures/content_undo.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'content_undo.json' )

FILES = [ 0, 1, 2 ]

# ( kind, files ) writes; ( 'undo' ) and ( 'redo' )
STEPS = [
    [ 'undo' ],
    [ 'redo' ],
    [ 'archive', [ 0, 1 ] ],
    [ 'undo' ],
    [ 'redo' ],
    [ 'undo' ],
    [ 'undo' ],
    [ 'redo' ],
    [ 'archive', [ 0, 1 ] ],
    [ 'inbox', [ 0 ] ],
    [ 'add_tag', 'oracle:undo', [ 0, 1, 2 ] ],
    [ 'undo' ],
    [ 'undo' ],
    [ 'redo' ],
    [ 'redo' ],
    [ 'delete_tag', 'oracle:undo', [ 1 ] ],
    [ 'undo' ],
    [ 'archive_and_tag', 'oracle:both', [ 2 ] ],
    [ 'undo' ],
    [ 'redo' ],
    [ 'tag_and_archive', 'oracle:reverse', [ 2 ] ],
    [ 'undo' ],
    [ 'archive', [ 1 ] ],
    [ 'undo' ],
    [ 'undo' ],
    [ 'undo' ],
    [ 'undo' ],
    [ 'undo' ],
    [ 'undo' ],
    [ 'undo' ],
]


def record( session ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientContentUpdates
    from hydrus.client.metadata import ClientTags

    controller = session.controller
    gui = controller.gui

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    import record_api

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ][ : len( FILES ) ] ]

    undo_manager = controller.GetManager( 'undo' )

    tag_service_key = CC.DEFAULT_LOCAL_TAG_SERVICE_KEY

    def package( updates_by_service ):

        p = ClientContentUpdates.ContentUpdatePackage()

        for ( service_key, updates ) in updates_by_service:

            p.AddContentUpdates( service_key, updates )


        return p


    def file_update( action, indices ):

        return ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_FILES, action, { hashes[ i ] for i in indices } )


    def mapping_update( action, tag, indices ):

        return ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, action, ( tag, { hashes[ i ] for i in indices } ) )


    def state():

        results = controller.Read( 'media_results', hashes )

        by_hash = { r.GetHash() : r for r in results }

        out = []

        for h in hashes:

            r = by_hash[ h ]

            tags = r.GetTagsManager().GetCurrent( tag_service_key, ClientTags.TAG_DISPLAY_STORAGE )

            out.append( { 'inbox' : r.GetInbox(), 'tags' : sorted( t for t in tags if t.startswith( 'oracle:' ) ) } )


        return out


    def menu():

        ( undo_string, redo_string ) = undo_manager.GetUndoRedoStrings()

        # (the menu is refreshed by the 'notify_new_undo' pubsub)
        return {
            'strings' : [ undo_string, redo_string ],
            'undo_visible' : gui._menubar_undo_undo.isVisible() if hasattr( gui, '_menubar_undo_undo' ) else None,
            'undo_label' : gui._menubar_undo_undo.text(),
            'redo_visible' : gui._menubar_undo_redo.isVisible(),
            'redo_label' : gui._menubar_undo_redo.text()
        }


    results = []

    for step in STEPS:

        kind = step[ 0 ]

        if kind == 'undo':

            undo_manager.Undo()

        elif kind == 'redo':

            undo_manager.Redo()

        elif kind == 'archive':

            controller.Write( 'content_updates', package( [ ( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, [ file_update( HC.CONTENT_UPDATE_ARCHIVE, step[ 1 ] ) ] ) ] ) )

        elif kind == 'inbox':

            controller.Write( 'content_updates', package( [ ( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, [ file_update( HC.CONTENT_UPDATE_INBOX, step[ 1 ] ) ] ) ] ) )

        elif kind == 'add_tag':

            controller.Write( 'content_updates', package( [ ( tag_service_key, [ mapping_update( HC.CONTENT_UPDATE_ADD, step[ 1 ], step[ 2 ] ) ] ) ] ) )

        elif kind == 'delete_tag':

            controller.Write( 'content_updates', package( [ ( tag_service_key, [ mapping_update( HC.CONTENT_UPDATE_DELETE, step[ 1 ], step[ 2 ] ) ] ) ] ) )

        elif kind == 'tag_and_archive':

            controller.Write( 'content_updates', package( [
                ( tag_service_key, [ mapping_update( HC.CONTENT_UPDATE_ADD, step[ 1 ], step[ 2 ] ) ] ),
                ( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, [ file_update( HC.CONTENT_UPDATE_ARCHIVE, step[ 2 ] ) ] )
            ] ) )

        elif kind == 'archive_and_tag':

            controller.Write( 'content_updates', package( [
                ( CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, [ file_update( HC.CONTENT_UPDATE_ARCHIVE, step[ 2 ] ) ] ),
                ( tag_service_key, [ mapping_update( HC.CONTENT_UPDATE_ADD, step[ 1 ], step[ 2 ] ) ] )
            ] ) )


        time.sleep( 0.4 )

        results.append( { 'step' : step, 'menu' : qt( menu ), 'files' : state() } )


    return { 'files' : FILES, 'tag' : 'my tags', 'steps' : results }


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
