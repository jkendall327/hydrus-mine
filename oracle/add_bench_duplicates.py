#!/usr/bin/env python3
"""Add duplicates to a benchmark library from make_bench_db.py, at the
target install's scale (docs/rust/BENCHMARKS.md), for timing the duplicate
filter's requests.

Every file gets a duplicate group, as the similar-files search gives every
file it searches. About a quarter of the files are merged into groups of two
or three, and 23,344 potential pairs join group kings, by distance as in the
target install: 12,648 at 0, 8,831 at 1 or 2 and 1,865 at 3 or 4. Pairs are
drawn from a pool of kings so that they form networks, as similar images do.

Written straight into the reference's tables (the client must not be
running), the way the reference writes them: a group is a `duplicate_files`
row with its members in `duplicate_file_members`, and a potential pair is
stored by the two groups' ids, smaller first. Alternates groups are left for
the reference to create when it needs them, as it does.

Usage: python oracle/add_bench_duplicates.py <bench db dir>
"""

import hashlib
import json
import os
import random
import sqlite3
import sys

PAIRS_BY_DISTANCE = [ ( 0, 12648 ), ( 1, 4416 ), ( 2, 4415 ), ( 3, 933 ), ( 4, 932 ) ]


def main():

    out = os.path.abspath( sys.argv[ 1 ] )

    with open( os.path.join( out, 'bench.json' ) ) as f:

        bench = json.load( f )


    n_files = bench[ 'files' ]
    rng = random.Random( 689 )

    db = sqlite3.connect( os.path.join( out, 'client.db' ) )
    db.execute( 'ATTACH ? AS master', ( os.path.join( out, 'client.master.db' ), ) )

    ( existing, ) = db.execute( 'SELECT count(*) FROM duplicate_files' ).fetchone()

    if existing > 0:

        raise SystemExit( '{} already has duplicates'.format( out ) )


    hash_ids = []

    for i in range( n_files ):

        h = hashlib.sha256( b'bench %d' % i ).digest()

        ( hash_id, ) = db.execute( 'SELECT hash_id FROM master.hashes WHERE hash = ?', ( h, ) ).fetchone()

        hash_ids.append( hash_id )


    # groups: a quarter of the files in groups of two or three, the rest alone
    order = list( range( n_files ) )
    rng.shuffle( order )

    merged = order[ : n_files // 4 ]
    groups = []
    position = 0

    while position < len( merged ):

        size = rng.choice( ( 2, 2, 3 ) )
        groups.append( merged[ position : position + size ] )
        position += size


    groups.extend( [ i ] for i in order[ n_files // 4 : ] )

    kings = []

    for members in groups:

        king = hash_ids[ members[ 0 ] ]

        cursor = db.execute( 'INSERT INTO duplicate_files ( king_hash_id ) VALUES ( ? )', ( king, ) )
        media_id = cursor.lastrowid

        db.executemany( 'INSERT INTO duplicate_file_members ( media_id, hash_id ) VALUES ( ?, ? )', [ ( media_id, hash_ids[ m ] ) for m in members ] )

        kings.append( media_id )


    # potential pairs, among a pool of kings so they form networks
    pool = rng.sample( kings, 30000 )
    pairs = {}

    for ( distance, count ) in PAIRS_BY_DISTANCE:

        added = 0

        while added < count:

            ( a, b ) = ( rng.choice( pool ), rng.choice( pool ) )

            key = ( min( a, b ), max( a, b ) )

            if a != b and key not in pairs:

                pairs[ key ] = distance
                added += 1



    db.executemany( 'INSERT INTO potential_duplicate_pairs ( smaller_media_id, larger_media_id, distance ) VALUES ( ?, ?, ? )', [ ( a, b, d ) for ( ( a, b ), d ) in pairs.items() ] )

    db.commit()
    db.close()

    bench[ 'duplicates' ] = { 'groups' : len( groups ), 'files_in_groups_of_several' : len( merged ), 'potential_pairs' : len( pairs ) }

    with open( os.path.join( out, 'bench.json' ), 'w' ) as f:

        json.dump( bench, f )


    print( '{} groups ({} files in groups of several), {} potential pairs'.format( len( groups ), len( merged ), len( pairs ) ) )


if __name__ == '__main__':

    main()
