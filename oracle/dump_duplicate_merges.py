#!/usr/bin/env python3
"""Record what the reference's duplicate metadata merges do to its database.

Random duplicate decisions (/manage_file_relationships/set_file_relationships
with the default metadata merge, sometimes deleting a file) on the basic
fixture, one pair per request, after seeding the files with ratings, notes,
URLs and times. After every decision the database is committed and each
file's stored tags, ratings, notes, URLs, inbox state, modified times and
local file domains are read straight from it: not through the Client API,
whose answers come from an in-memory cache that can drift from the database
after merges (see docs/rust/DIFFERENCES.md). Each step keeps only the
files that decision changed.

Two phases, each on a fresh copy of the fixture: the reference's default
merge options, and custom options that exercise the other merge actions. Each
phase also writes the options in hydrus-rs's settings format, so the Rust
test can configure itself the same way.

Usage: python oracle/dump_duplicate_merges.py      (writes fixtures/duplicate_merges.json)
"""

import json
import os
import random
import sqlite3
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'duplicate_merges.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )
NAMES = { f[ 'hash' ]: f[ 'name' ] for f in MANIFEST[ 'files' ] }
BY_NAME = { f[ 'name' ]: f[ 'hash' ] for f in MANIFEST[ 'files' ] }
KEYS = { 'my_tags': b'local tags'.hex(), 'downloader_tags': b'downloader tags'.hex(), 'favourites': b'favourites'.hex(), **MANIFEST[ 'service_keys' ] }

PORT = 45960
NUM_DECISIONS = 70


def plan( seed ):
    """The seeding requests and the decisions of one phase."""

    rng = random.Random( seed )

    images = sorted( n for n in BY_NAME if n.startswith( ( 'jpeg_', 'png_', 'dupe_' ) ) )
    pool = rng.sample( images, 8 )
    hashes = [ BY_NAME[ n ] for n in pool ]

    seed_steps = []

    domains = [ 'example.com', 'danbooru.donmai.us', 'site.example.org' ]

    for ( i, h ) in enumerate( hashes ):

        if rng.random() < 0.6:

            seed_steps.append( ( '/edit_ratings/set_rating', { 'hash': h, 'rating_service_key': KEYS[ 'favourites' ], 'rating': rng.choice( ( True, False ) ) } ) )


        if rng.random() < 0.6:

            seed_steps.append( ( '/edit_ratings/set_rating', { 'hash': h, 'rating_service_key': KEYS[ 'stars' ], 'rating': rng.randint( 0, 5 ) } ) )


        if rng.random() < 0.6:

            seed_steps.append( ( '/edit_ratings/set_rating', { 'hash': h, 'rating_service_key': KEYS[ 'counter' ], 'rating': rng.randint( 0, 4 ) } ) )


        if rng.random() < 0.6:

            notes = { rng.choice( ( 'comment', 'source', 'translation' ) ): rng.choice( ( 'hello', 'hello there', 'a note', 'another note' ) ) for _ in range( rng.randint( 1, 2 ) ) }

            seed_steps.append( ( '/add_notes/set_notes', { 'hash': h, 'notes': notes } ) )


        if rng.random() < 0.7:

            urls = [ f'https://{rng.choice( domains )}/post/{rng.randint( 1, 6 )}' for _ in range( rng.randint( 1, 2 ) ) ]

            seed_steps.append( ( '/add_urls/associate_url', { 'hash': h, 'urls_to_add': urls, 'normalise_urls': False } ) )

            for url in urls:

                if rng.random() < 0.7:

                    domain = url.split( '/' )[ 2 ]

                    seed_steps.append( ( '/edit_times/set_time', { 'hash': h, 'timestamp_type': 0, 'domain': domain, 'timestamp_ms': 1_600_000_000_000 + rng.randint( 0, 10 ) * 86_400_000 } ) )




        if rng.random() < 0.7:

            seed_steps.append( ( '/edit_times/set_time', { 'hash': h, 'timestamp_type': 1, 'timestamp_ms': 1_500_000_000_000 + rng.randint( 0, 10 ) * 86_400_000 } ) )


        if rng.random() < 0.4:

            seed_steps.append( ( '/add_files/archive_files', { 'hash': h } ) )


        if rng.random() < 0.5:

            tags = rng.sample( [ 'blue eyes', 'series:metroid', 'meta:lowres', 'creator:someone else', 'rating:safe', 'page:1' ], rng.randint( 1, 3 ) )

            seed_steps.append( ( '/add_tags/add_tags', { 'hash': h, 'service_keys_to_tags': { rng.choice( ( KEYS[ 'my_tags' ], KEYS[ 'downloader_tags' ], KEYS[ 'second_tags' ] ) ): tags } } ) )



    decisions = []

    for _ in range( NUM_DECISIONS ):

        ( a, b ) = rng.sample( hashes, 2 )

        row = { 'hash_a': a, 'hash_b': b, 'relationship': rng.choice( ( 2, 3, 4, 4, 4, 7 ) ), 'do_default_content_merge': rng.random() < 0.85 }

        if rng.random() < 0.08:

            row[ rng.choice( ( 'delete_a', 'delete_b' ) ) ] = True


        decisions.append( row )


    return ( pool, seed_steps, decisions )


def custom_options():
    """Non-default merge options, as the reference's objects and in
    hydrus-rs's settings format."""

    from hydrus.client import ClientConstants as CC
    from hydrus.client.duplicates import ClientDuplicates
    from hydrus.client.importing.options import NoteImportOptions
    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusTags

    only_unnamespaced = HydrusTags.TagFilter()
    only_unnamespaced.SetRule( ':', HC.FILTER_BLACKLIST )

    def options( tag_action, rating_action, archive, sync, notes, conflict ):

        o = ClientDuplicates.DuplicateContentMergeOptions()
        o.SetTagServiceActions( [
            ( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, tag_action, only_unnamespaced ),
            ( bytes.fromhex( KEYS[ 'second_tags' ] ), HC.CONTENT_MERGE_ACTION_TWO_WAY_MERGE, HydrusTags.TagFilter() ),
        ] )
        o.SetRatingServiceActions( [
            ( CC.DEFAULT_FAVOURITES_RATING_SERVICE_KEY, rating_action ),
            ( bytes.fromhex( KEYS[ 'stars' ] ), rating_action ),
            ( bytes.fromhex( KEYS[ 'counter' ] ), rating_action ),
        ] )
        o.SetSyncArchiveAction( archive )
        o.SetSyncURLsAction( sync )
        o.SetSyncFileModifiedDateAction( sync )
        o.SetSyncNotesAction( notes )
        nio = NoteImportOptions.NoteImportOptions()
        nio.SetGetNotes( True )
        nio.SetExtendExistingNoteIfPossible( False )
        nio.SetConflictResolution( conflict )
        o.SetSyncNoteImportOptions( nio )

        return o


    action_names = { HC.CONTENT_MERGE_ACTION_COPY: 'copy', HC.CONTENT_MERGE_ACTION_MOVE: 'move', HC.CONTENT_MERGE_ACTION_TWO_WAY_MERGE: 'two_way' }
    archive_names = { ClientDuplicates.SYNC_ARCHIVE_NONE: 'never', ClientDuplicates.SYNC_ARCHIVE_IF_ONE_DO_BOTH: 'if_either', ClientDuplicates.SYNC_ARCHIVE_DO_BOTH_REGARDLESS: 'always' }
    conflict_names = { NoteImportOptions.NOTE_IMPORT_CONFLICT_REPLACE: 'replace', NoteImportOptions.NOTE_IMPORT_CONFLICT_IGNORE: 'ignore', NoteImportOptions.NOTE_IMPORT_CONFLICT_APPEND: 'append', NoteImportOptions.NOTE_IMPORT_CONFLICT_RENAME: 'rename' }

    def ours( tag_action, rating_action, archive, sync, notes, conflict ):

        return {
            'tags': [
                { 'service': KEYS[ 'my_tags' ], 'action': action_names[ tag_action ], 'filter': { 'rules': { ':': 'blacklist' } } },
                { 'service': KEYS[ 'second_tags' ], 'action': 'two_way' },
            ],
            'ratings': [ { 'service': KEYS[ k ], 'action': action_names[ rating_action ] } for k in ( 'favourites', 'stars', 'counter' ) ],
            'notes': action_names[ notes ],
            'note_merge': { 'extend_existing': False, 'conflict': conflict_names[ conflict ] },
            'archive': archive_names[ archive ],
            'urls': action_names[ sync ],
            'file_modified': action_names[ sync ],
        }


    better = ( HC.CONTENT_MERGE_ACTION_COPY, HC.CONTENT_MERGE_ACTION_MOVE, ClientDuplicates.SYNC_ARCHIVE_IF_ONE_DO_BOTH, HC.CONTENT_MERGE_ACTION_TWO_WAY_MERGE, HC.CONTENT_MERGE_ACTION_MOVE, NoteImportOptions.NOTE_IMPORT_CONFLICT_APPEND )
    same = ( HC.CONTENT_MERGE_ACTION_MOVE, HC.CONTENT_MERGE_ACTION_TWO_WAY_MERGE, ClientDuplicates.SYNC_ARCHIVE_NONE, HC.CONTENT_MERGE_ACTION_COPY, HC.CONTENT_MERGE_ACTION_COPY, NoteImportOptions.NOTE_IMPORT_CONFLICT_REPLACE )
    alternate = ( HC.CONTENT_MERGE_ACTION_TWO_WAY_MERGE, HC.CONTENT_MERGE_ACTION_COPY, ClientDuplicates.SYNC_ARCHIVE_DO_BOTH_REGARDLESS, HC.CONTENT_MERGE_ACTION_COPY, HC.CONTENT_MERGE_ACTION_TWO_WAY_MERGE, NoteImportOptions.NOTE_IMPORT_CONFLICT_IGNORE )

    reference = { HC.DUPLICATE_BETTER: options( *better ), HC.DUPLICATE_SAME_QUALITY: options( *same ), HC.DUPLICATE_ALTERNATE: options( *alternate ) }
    settings = { 'better': ours( *better ), 'same_quality': ours( *same ), 'alternate': ours( *alternate ) }

    return ( reference, settings )


def snapshot( db_dir, pool ):
    """The merge-relevant state of `pool`'s files, from the database."""

    from hydrus.core import HydrusTags

    c = sqlite3.connect( f'file:{db_dir}/client.db?mode=ro', uri = True )
    c.execute( f"ATTACH 'file:{db_dir}/client.master.db?mode=ro' AS master" )
    c.execute( f"ATTACH 'file:{db_dir}/client.mappings.db?mode=ro' AS mappings" )

    services = { name: service_id for ( service_id, name ) in c.execute( 'SELECT service_id, name FROM services' ) }

    def one( sql, *args ):

        row = c.execute( sql, args ).fetchone()

        return None if row is None else row[0]


    out = {}

    for name in pool:

        hash_id = one( 'SELECT hash_id FROM master.hashes WHERE hash = ?', bytes.fromhex( BY_NAME[ name ] ) )

        tags = {}

        for service in ( 'my tags', 'downloader tags', 'second tags' ):

            for ( table, status ) in ( ( 'current_mappings', 'current' ), ( 'deleted_mappings', 'deleted' ), ( 'pending_mappings', 'pending' ) ):

                rows = c.execute( f'SELECT n.namespace, s.subtag FROM mappings.{table}_{services[ service ]} JOIN master.tags USING ( tag_id ) JOIN master.namespaces n USING ( namespace_id ) JOIN master.subtags s USING ( subtag_id ) WHERE hash_id = ?', ( hash_id, ) ).fetchall()

                if rows:

                    tags[ f'{service} {status}' ] = sorted( HydrusTags.CombineTag( ns, sub ) for ( ns, sub ) in rows )




        ratings = {}

        for service in ( 'favourites', 'stars' ):

            value = one( 'SELECT rating FROM local_ratings WHERE service_id = ? AND hash_id = ?', services[ service ], hash_id )

            if value is not None:

                ratings[ service ] = value



        value = one( 'SELECT rating FROM local_incdec_ratings WHERE service_id = ? AND hash_id = ?', services[ 'counter' ], hash_id )

        if value:

            ratings[ 'counter' ] = value


        notes = dict( c.execute( 'SELECT l.label, n.note FROM file_notes JOIN master.labels l ON l.label_id = name_id JOIN master.notes n USING ( note_id ) WHERE hash_id = ?', ( hash_id, ) ).fetchall() )
        urls = sorted( r[0] for r in c.execute( 'SELECT url FROM url_map JOIN master.urls USING ( url_id ) WHERE hash_id = ?', ( hash_id, ) ) )
        domain_times = dict( c.execute( 'SELECT domain, file_modified_timestamp_ms FROM file_domain_modified_timestamps JOIN master.url_domains USING ( domain_id ) WHERE hash_id = ?', ( hash_id, ) ).fetchall() )
        domains = sorted( service for service in ( 'my files', 'art', 'trash' ) if one( f'SELECT 1 FROM current_files_{services[ service ]} WHERE hash_id = ?', hash_id ) )

        out[ name ] = {
            'tags': tags,
            'ratings': ratings,
            'notes': notes,
            'urls': urls,
            'inbox': one( 'SELECT 1 FROM file_inbox WHERE hash_id = ?', hash_id ) is not None,
            'file_modified_ms': one( 'SELECT file_modified_timestamp_ms FROM file_modified_timestamps WHERE hash_id = ?', hash_id ),
            'domain_modified_ms': domain_times,
            'domains': domains,
        }


    c.close()

    return out


def run_phase( phase_name, seed, out_path ):

    import hydrus_driver
    import record_api

    ( pool, seed_steps, decisions ) = plan( seed )

    ( reference_options, settings ) = custom_options() if phase_name == 'custom' else ( None, None )

    db_dir = record_api.unpack_fixture( 'basic' )

    def hook( session ):

        controller = session.controller

        if reference_options is not None:

            for ( duplicate_type, options ) in reference_options.items():

                controller.new_options.SetDuplicateContentMergeOptions( duplicate_type, options )



        api = hydrus_driver.Api( session.port )

        def settle():

            # the reference commits between jobs, after a job that asks for it:
            # once a second job has run, the first one's commit has happened
            time.sleep( 0.05 )
            controller.ForceDatabaseCommit()
            controller.WriteSynchronous( 'null' )


        for ( path, body ) in seed_steps:

            api.post( path, body )


        settle()

        initial = snapshot( db_dir, pool )

        steps = []

        for row in decisions:

            api.post( '/manage_file_relationships/set_file_relationships', { 'relationships': [ row ] } )

            settle()

            steps.append( { 'relationship': row, 'state': snapshot( db_dir, pool ) } )


        # the options as the reference serialises them, for the importer's tests
        legacy = { str( duplicate_type ): options.GetSerialisableTuple() for ( duplicate_type, options ) in ( reference_options or {} ).items() }

        return { 'name': phase_name, 'settings': settings, 'legacy_options': legacy, 'pool': pool, 'seed_steps': [ { 'path': p, 'json': b } for ( p, b ) in seed_steps ], 'initial': initial, 'steps': steps }


    result = hydrus_driver.run_client( db_dir, hook, port = PORT )

    with open( out_path, 'w' ) as f:

        json.dump( result, f )



def main():

    import hydrus_driver

    if len( sys.argv ) > 1 and sys.argv[1] == '--child':

        run_phase( sys.argv[2], int( sys.argv[3] ), sys.argv[4] )

        return


    phases = []

    for ( name, seed ) in ( ( 'default', 11 ), ( 'custom', 12 ) ):

        path = OUT + f'.{name}.tmp'

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', name, str( seed ), path )

        phase = json.load( open( path ) )

        os.remove( path )

        # keep only the files each decision changed
        previous = phase[ 'initial' ]

        for step in phase[ 'steps' ]:

            state = step.pop( 'state' )
            step[ 'changed' ] = { name: s for ( name, s ) in state.items() if previous[ name ] != s }
            previous = state


        phases.append( phase )


    with open( OUT, 'w' ) as f:

        json.dump( { 'reference_version': 688, 'phases': phases }, f, ensure_ascii = False, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}: {sum( len( p[ "steps" ] ) for p in phases )} decisions' )


if __name__ == '__main__':

    main()
