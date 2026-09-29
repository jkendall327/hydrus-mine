#!/usr/bin/env python3
"""Build a large synthetic library in the reference client, for benchmarks.

Files are added as database records only (no media on disk): enough for
metadata, tags, search and autocomplete, which is what the Python-vs-Rust
comparison times. The shape matches hydrus_store::synth: skewed tag
popularity, namespaces, siblings and parents in "my tags".

Also prints how long the reference's own write path took, for comparison
with `cargo run --release -p hydrus-store --example synth_throughput`.

Usage: python oracle/make_bench_db.py <out db dir> [files] [tags_per_file]
"""

import collections
import hashlib
import os
import random
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

BATCH = 1000


def tag_name( i ):

    namespaces = [ '', 'character', 'series', 'creator', 'meta' ]
    ns = namespaces[ i % 5 ]
    subtag = 'tag number {}'.format( i )

    return subtag if ns == '' else '{}:{}'.format( ns, subtag )


def populate( s, n_files, tags_per_file ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client.media import ClientMediaManagers
    from hydrus.client.metadata import ClientContentUpdates
    from hydrus.core import HydrusConstants as HC

    n_tags = max( 100, n_files // 4 )
    rng = random.Random( 688 )

    def skewed( n ):

        return int( ( rng.random() ** 3 ) * n )


    tags = [ tag_name( i ) for i in range( n_tags ) ]

    timings = {}

    # relations first, as the Rust generator does
    started = time.time()

    for ( content_type, count ) in ( ( HC.CONTENT_TYPE_TAG_SIBLINGS, n_files // 50 ), ( HC.CONTENT_TYPE_TAG_PARENTS, n_files // 50 ) ):

        pairs = set()

        for _ in range( count ):

            ( a, b ) = ( rng.randrange( n_tags ), rng.randrange( n_tags ) )

            if a != b:

                pairs.add( ( tags[ a ], tags[ b ] ) )



        updates = [ ClientContentUpdates.ContentUpdate( content_type, HC.CONTENT_UPDATE_ADD, pair ) for pair in sorted( pairs ) ]

        s.write( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, updates ) )


    s.sync_tag_display()

    timings[ 'relations' ] = time.time() - started

    started = time.time()
    now_ms = int( time.time() * 1000 )

    for start in range( 0, n_files, BATCH ):

        end = min( start + BATCH, n_files )

        hashes = [ hashlib.sha256( b'bench %d' % i ).digest() for i in range( start, end ) ]

        ids_to_hashes = s.read( 'hash_ids_to_hashes', hashes = hashes )
        hashes_to_ids = { h : i for ( i, h ) in ids_to_hashes.items() }

        file_updates = []
        by_tag = collections.defaultdict( set )

        for ( offset, h ) in enumerate( hashes ):

            i = start + offset

            info = ClientMediaManagers.FileInfoManager(
                hashes_to_ids[ h ], h,
                size = 50000 + rng.randrange( 5000000 ),
                mime = HC.IMAGE_PNG if i % 10 == 0 else HC.IMAGE_JPEG,
                width = 200 + rng.randrange( 3000 ),
                height = 200 + rng.randrange( 3000 ),
            )

            file_updates.append( ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_FILES, HC.CONTENT_UPDATE_ADD, ( info, now_ms - ( n_files - i ) * 1000 ) ) )

            for _ in range( tags_per_file ):

                by_tag[ tags[ skewed( n_tags ) ] ].add( h )



        s.write( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( CC.LOCAL_FILE_SERVICE_KEY, file_updates ) )

        mapping_updates = [ ClientContentUpdates.ContentUpdate( HC.CONTENT_TYPE_MAPPINGS, HC.CONTENT_UPDATE_ADD, ( tag, hs ) ) for ( tag, hs ) in by_tag.items() ]

        s.write( 'content_updates', ClientContentUpdates.ContentUpdatePackage.STATICCreateFromContentUpdates( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, mapping_updates ) )

        if ( end // BATCH ) % 10 == 0:

            print( '  {} files, {:.0f}s'.format( end, time.time() - started ), flush = True )



    timings[ 'files_and_tags' ] = time.time() - started

    started = time.time()
    s.sync_tag_display()
    timings[ 'display_sync' ] = time.time() - started

    return timings


def main():

    import hydrus_driver

    out = os.path.abspath( sys.argv[ 1 ] )
    n_files = int( sys.argv[ 2 ] ) if len( sys.argv ) > 2 else 100000
    tags_per_file = int( sys.argv[ 3 ] ) if len( sys.argv ) > 3 else 20

    if os.path.exists( os.path.join( out, 'client.db' ) ):

        raise SystemExit( '{} already has a database'.format( out ) )


    timings = hydrus_driver.run_client( out, lambda s: populate( s, n_files, tags_per_file ) )

    mappings = n_files * tags_per_file
    t = timings[ 'files_and_tags' ]

    print( 'reference: {} files, ~{} mappings in {:.1f}s: {:.0f} files/s, {:.0f} mappings/s'.format( n_files, mappings, t, n_files / t, mappings / t ) )
    print( 'reference: relations {:.1f}s, final display sync {:.1f}s'.format( timings[ 'relations' ], timings[ 'display_sync' ] ) )


if __name__ == '__main__':

    main()
