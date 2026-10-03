#!/usr/bin/env python3
"""Record the reference's summaries of duplicate content merges
(`DuplicateContentMergeOptions.GetMergeSummaryOnPair`), which the
auto-resolution "review actions" window and rule preview show under a
pair's action.

On the `basic` fixture, this gives two files metadata (`SETUP`: tags on
"my tags" and "second tags", "stars" and "counter" ratings, notes, URLs,
one file archived), then for each merge options case (`OPTIONS`: the
client's own for "better" and "same quality", and custom ones described
by kind) records the summary for both files each way round, deleting
neither, B or A, in and out of auto-resolution.

Options are described as `{"tags": [[service, action, blacklisted
namespace or null]], "ratings": [[service, action]], "notes": action,
"archive": 0/1/2, "urls": action, "modified": action}` with the
reference's action codes (copy 0, move 1, two-way 2; null for none), or
`{"client": duplicate type}`.

Usage: QT_QPA_PLATFORM=offscreen TZ=UTC python oracle/record_merge_summaries.py
       (writes fixtures/merge_summaries.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'merge_summaries.json' )

# (from the basic fixture's files)
FILES = [ 'dupe_jpeg_02.png', 'dupe_jpeg_02_q60.jpg' ]

SETUP = [
    [ 'tags', 0, 'my tags', [ 'blue', 'shared' ] ],
    [ 'tags', 1, 'my tags', [ 'red', 'shared', 'creator:someone' ] ],
    [ 'tags', 1, 'second tags', [ 'extra' ] ],
    [ 'rating', 0, 'stars', 0.6 ],
    [ 'rating', 1, 'stars', 0.8 ],
    [ 'rating', 0, 'counter', 2 ],
    [ 'rating', 1, 'counter', 3 ],
    [ 'note', 0, 'comment', 'hello' ],
    [ 'note', 1, 'comment', 'hello there' ],
    [ 'note', 1, 'source', 'a source' ],
    [ 'url', 0, 'https://example.com/post/0' ],
    [ 'url', 1, 'https://example.com/post/1' ],
    [ 'archive', 0 ],
]

OPTIONS = [
    { 'client' : 4 },
    { 'client' : 2 },
    { 'tags' : [ [ 'my tags', 1, 'creator' ] ], 'ratings' : [ [ 'stars', 1 ], [ 'counter', 1 ] ], 'notes' : 1, 'archive' : 2, 'urls' : 2, 'modified' : 2 },
    { 'tags' : [ [ 'my tags', 0, None ], [ 'second tags', 2, None ] ], 'ratings' : [ [ 'stars', 2 ], [ 'counter', 2 ] ], 'notes' : 2, 'archive' : 1, 'urls' : 0, 'modified' : 0 },
    { 'tags' : [ [ 'my tags', 2, None ] ], 'ratings' : [ [ 'stars', 0 ], [ 'counter', 0 ] ], 'notes' : 0, 'archive' : 0, 'urls' : None, 'modified' : None },
    {},
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusTags
    from hydrus.client import ClientConstants as CC
    from hydrus.client.duplicates import ClientDuplicates
    from hydrus.client.metadata import ClientContentUpdates

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    by_name = { f[ 'name' ] : bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ] }

    hashes = [ by_name[ name ] for name in FILES ]

    services = { s.GetName() : s.GetServiceKey() for s in controller.services_manager.GetServices() }

    for op in SETUP:

        ( kind, i, *rest ) = op

        h = { hashes[ i ] }

        if kind == 'tags':

            ( service, tags ) = rest

            updates = [ ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ( tag, h ) ) for tag in tags ]

            package = ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( services[ service ], updates )

        elif kind == 'rating':

            ( service, value ) = rest

            package = ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate( services[ service ], ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_RATINGS, HC.CONTENT_UPDATE_ADD, ( value, h ) ) )

        elif kind == 'note':

            ( name, note ) = rest

            package = ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate( CC.LOCAL_NOTES_SERVICE_KEY, ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_NOTES, HC.CONTENT_UPDATE_SET, ( hashes[ i ], name, note ) ) )

        elif kind == 'url':

            ( url, ) = rest

            package = ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_URLS, HC.CONTENT_UPDATE_ADD, ( { url }, h ) ) )

        elif kind == 'archive':

            package = ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdate( CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_ARCHIVE, h ) )


        controller.WriteSynchronous( 'content_updates', package )


    media_results = controller.Read( 'media_results', hashes )

    by_hash = { mr.GetHash() : mr for mr in media_results }

    pair = [ by_hash[ h ] for h in hashes ]

    def make( description ):

        if 'client' in description:

            return controller.new_options.GetDuplicateContentMergeOptions( description[ 'client' ] )


        default = controller.new_options.GetDuplicateContentMergeOptions( HC.DUPLICATE_BETTER )

        options = ClientDuplicates.DuplicateContentMergeOptions()

        tag_actions = []

        for ( service, action, blacklisted ) in description.get( 'tags', [] ):

            tag_filter = HydrusTags.TagFilter()

            if blacklisted is not None:

                tag_filter.SetRule( blacklisted + ':', HC.FILTER_BLACKLIST )


            tag_actions.append( ( services[ service ], action, tag_filter ) )


        options.SetTagServiceActions( tag_actions )
        options.SetRatingServiceActions( [ ( services[ service ], action ) for ( service, action ) in description.get( 'ratings', [] ) ] )

        none = HC.CONTENT_MERGE_ACTION_NONE

        options.SetSyncNotesAction( none if description.get( 'notes' ) is None else description[ 'notes' ] )
        options.SetSyncNoteImportOptions( default.GetSyncNoteImportOptions() )
        options.SetSyncArchiveAction( description.get( 'archive', 0 ) )
        options.SetSyncURLsAction( none if description.get( 'urls' ) is None else description[ 'urls' ] )
        options.SetSyncFileModifiedDateAction( none if description.get( 'modified' ) is None else description[ 'modified' ] )

        return options


    out = []

    for description in OPTIONS:

        options = make( description )

        cases = []

        for ( first, second ) in ( ( 0, 1 ), ( 1, 0 ) ):

            for ( delete_a, delete_b ) in ( ( False, False ), ( False, True ), ( True, False ) ):

                for in_auto_resolution in ( False, True ):

                    summary = options.GetMergeSummaryOnPair( pair[ first ], pair[ second ], delete_a, delete_b, in_auto_resolution = in_auto_resolution )

                    cases.append( { 'a' : first, 'b' : second, 'delete_a' : delete_a, 'delete_b' : delete_b, 'in_auto_resolution' : in_auto_resolution, 'summary' : summary } )




        out.append( { 'options' : description, 'cases' : cases } )


    return { 'files' : FILES, 'setup' : SETUP, 'options' : out }


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

    os.environ[ 'TZ' ] = 'UTC'

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'merge_summaries.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "options" ] )} options' )


if __name__ == '__main__':

    main()
