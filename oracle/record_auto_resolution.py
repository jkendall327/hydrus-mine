#!/usr/bin/env python3
"""Record the reference's duplicates auto-resolution on generated files.

Generates (oracle/fixtures/auto_resolution/) images in families that the
suggested rules care about: a png, the same pixels as a bigger png, a bmp and
a png with an ICC profile; a jpeg, a png of its pixels and the same jpeg with EXIF data; a gif and
a png of the same palette image; a lossier jpeg; some variants left out at
random. A fresh reference client imports them all through the Client API in
name order, runs its similar-files search at distance 4, sets up rules (the
suggested ones, some made fully automatic, one with a small cap on pairs
awaiting approval), and runs auto-resolution until no rule has work. Then a
human approves one pair waiting on the capped rule and denies another (the
first two by hash; recorded as `decisions`), and the rules run again.

Two things the reference leaves to chance are pinned so the Rust side can do
the same: the pair selector does not shuffle a pair before testing (A is
tried as the pair's first file first), and a rule tests its pairs in order of
their kings' hashes (the reference takes whichever row the table gives).

Recorded (oracle/fixtures/auto_resolution_run.json), by file hashes: each
rule's stored form and each pair's status for it (pairs by their kings'
hashes), each rule's actioned log, the final duplicate groups, which files
were deleted, and the potential pairs left. The reference's database after
the run is kept as oracle/fixtures/legacy_db/auto_resolution.tar.gz, for the
migration test.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_auto_resolution.py
"""

import io
import json
import os
import random
import shutil
import sqlite3
import sys
import tarfile
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

from PIL import Image, ImageCms

IMAGE_DIR = os.path.join( HERE, 'fixtures', 'auto_resolution' )
OUT = os.path.join( HERE, 'fixtures', 'auto_resolution_run.json' )
DB_OUT = os.path.join( HERE, 'fixtures', 'legacy_db', 'auto_resolution.tar.gz' )

PORT = 45981


def generate():

    import record_similar_files

    if os.path.exists( IMAGE_DIR ):

        shutil.rmtree( IMAGE_DIR )


    os.makedirs( IMAGE_DIR )

    rng = random.Random( 5 )

    srgb = ImageCms.ImageCmsProfile( ImageCms.createProfile( 'sRGB' ) ).tobytes()

    for i in range( 8 ):

        # the suggested rules only consider files over 128 pixels a side
        base = record_similar_files.base_pattern( 3000 + i ).resize( ( 160, 160 ), Image.BICUBIC )

        def path( name ):

            return os.path.join( IMAGE_DIR, f'p{i:02}_{name}' )


        keep = lambda: rng.random() < 0.75

        base.save( path( 'a.png' ), compress_level = 9 )

        if keep():

            base.save( path( 'b_bloated.png' ), compress_level = 0 )


        if keep():

            base.save( path( 'c.bmp' ) )


        if keep():

            base.save( path( 'd_icc.png' ), icc_profile = srgb )


        quality = rng.choice( [ 85, 92 ] )

        if keep():

            base.save( path( 'e.jpg' ), quality = quality )


        if keep():

            # a png of a jpeg's pixels: pixel-perfect with it
            jpeg = io.BytesIO()
            base.save( jpeg, 'JPEG', quality = quality )
            Image.open( jpeg ).save( path( 'j_from_jpeg.png' ) )
            base.save( path( 'e.jpg' ), quality = quality )


        if keep():

            exif = Image.Exif()
            exif[ 0x010e ] = f'pattern {i}'

            base.save( path( 'f_exif.jpg' ), quality = quality, exif = exif.tobytes() )


        if keep():

            base.save( path( 'g_lossy.jpg' ), quality = 40 )


        if keep():

            palette = base.quantize( 32 )

            palette.save( path( 'h.gif' ) )
            palette.save( path( 'i_palette.png' ) )




def setup_rules( session ):

    from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR

    rules = AR.GetDefaultRuleSuggestions()

    for rule in rules:

        name = rule.GetName()

        if name in ( 'pixel-perfect jpegs vs pngs', 'pixel-perfect pairs' ):

            rule.SetOperationMode( AR.DUPLICATES_AUTO_RESOLUTION_RULE_OPERATION_MODE_FULLY_AUTOMATIC )


        if name == 'pixel-perfect gifs vs pngs':

            rule.SetMaxPendingPairs( 2 )



    session.write( 'duplicates_auto_resolution_set_rules', rules )

    return [ json.loads( json.dumps( r.GetSerialisableTuple() ) ) for r in session.read( 'duplicates_auto_resolution_rules_with_counts' ) ]


def pin_randomness():

    from hydrus.client.duplicates import ClientDuplicatesAutoResolutionComparators
    from hydrus.client.db import ClientDBFilesDuplicatesAutoResolutionStorage as S
    from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR

    class NoShuffle( object ):

        @staticmethod
        def shuffle( seq ):

            pass



    ClientDuplicatesAutoResolutionComparators.random = NoShuffle

    def ordered_untested_pair( self, rule ):

        if not self._have_initialised_rules:

            self._Reinit()


        rule_id = rule.GetId()

        if rule_id not in self._rule_ids_to_rules:

            return None


        table = S.GenerateAutoResolutionQueueTableName( rule_id, AR.DUPLICATE_STATUS_MATCHES_SEARCH_BUT_NOT_TESTED )

        rows = self._Execute(
            f'SELECT t.smaller_media_id, t.larger_media_id, hs.hash, hl.hash FROM {table} AS t '
            'JOIN duplicate_files AS ds ON ds.media_id = t.smaller_media_id '
            'JOIN duplicate_files AS dl ON dl.media_id = t.larger_media_id '
            'JOIN external_master.hashes AS hs ON hs.hash_id = ds.king_hash_id '
            'JOIN external_master.hashes AS hl ON hl.hash_id = dl.king_hash_id'
        ).fetchall()

        if len( rows ) == 0:

            return None


        rows.sort( key = lambda r: tuple( sorted( [ bytes( r[2] ), bytes( r[3] ) ] ) ) )

        return ( rows[0][0], rows[0][1] )


    S.ClientDBFilesDuplicatesAutoResolutionStorage.GetMatchingUntestedPair = ordered_untested_pair


def run_rules( session ):
    """The manager's work loop, synchronously, until no rule has work."""

    from hydrus.core import HydrusText

    passes = 0

    while True:

        passes += 1

        worked = False

        rules = sorted( session.read( 'duplicates_auto_resolution_rules_with_counts' ), key = lambda r: HydrusText.HumanTextSortKey( r.GetName() ) )

        for rule in rules:

            if rule.IsPaused():

                continue


            while rule.HasSearchWorkToDo():

                session.write( 'duplicates_auto_resolution_do_search_work', rule )

                worked = True


            while rule.HasResolutionWorkToDo():

                pair = session.read( 'duplicates_auto_resolution_resolution_pair', rule )

                if pair is None:

                    break


                result = rule.TestPair( *pair )

                if result is None:

                    session.write( 'duplicates_auto_resolution_commit_resolution_pair_failed', rule, pair )

                else:

                    session.write( 'duplicates_auto_resolution_commit_resolution_pair_passed', rule, result )


                worked = True



        if not worked or passes > 50:

            return passes




def record( db_dir, rules ):

    db = sqlite3.connect( os.path.join( db_dir, 'client.db' ) )

    db.execute( f"ATTACH '{os.path.join( db_dir, 'client.master.db' )}' AS m" )

    def h( hash_id ):

        return db.execute( 'SELECT hex( hash ) FROM m.hashes WHERE hash_id = ?', ( hash_id, ) ).fetchone()[0].lower()


    def king( media_id ):

        ( k, ) = db.execute( 'SELECT king_hash_id FROM duplicate_files WHERE media_id = ?', ( media_id, ) ).fetchone()

        return h( k )


    def pair( smaller, larger ):

        return sorted( [ king( smaller ), king( larger ) ] )


    names = { 0 : 'does_not_match', 1 : 'not_tested', 2 : 'failed', 4 : 'not_searched' }

    out_rules = []

    for stored in rules:

        rule_id = stored[ 3 ][ 0 ]

        statuses = {}

        for ( code, label ) in names.items():

            statuses[ label ] = sorted( pair( a, b ) for ( a, b ) in db.execute( f'SELECT smaller_media_id, larger_media_id FROM duplicate_files_auto_resolution_pair_decisions_{rule_id}_{code}' ) )


        statuses[ 'denied' ] = sorted( pair( a, b ) for ( a, b ) in db.execute( f'SELECT smaller_media_id, larger_media_id FROM duplicate_files_auto_resolution_declined_{rule_id}' ) )
        statuses[ 'pending' ] = sorted( [ h( a ), h( b ) ] for ( a, b ) in db.execute( f'SELECT hash_id_a, hash_id_b FROM duplicate_files_auto_resolution_pending_actions_{rule_id}' ) )

        actioned = sorted( [ h( a ), h( b ), t ] for ( a, b, t ) in db.execute( f'SELECT hash_id_a, hash_id_b, duplicate_type FROM duplicate_files_auto_resolution_actioned_{rule_id}' ) )

        out_rules.append( { 'stored' : stored, 'statuses' : statuses, 'actioned' : actioned } )


    kings = { h( hash_id ) : king( media_id ) for ( media_id, hash_id ) in db.execute( 'SELECT media_id, hash_id FROM duplicate_file_members' ) }

    potentials = sorted( pair( a, b ) + [ d ] for ( a, b, d ) in db.execute( 'SELECT smaller_media_id, larger_media_id, distance FROM potential_duplicate_pairs' ) )

    local_media = db.execute( "SELECT service_id FROM services WHERE service_key = ?", ( b'local files', ) ).fetchone()[0]

    current = { h( hash_id ) for ( hash_id, ) in db.execute( f'SELECT hash_id FROM current_files_{local_media}' ) }

    db.close()

    return { 'rules' : out_rules, 'kings' : kings, 'potentials' : potentials, 'current_in_my_files' : sorted( current ) }


def main():

    if len( sys.argv ) > 1:

        ( step, db_dir ) = sys.argv[ 1 : 3 ]

        if step == 'import':

            names = sorted( os.listdir( IMAGE_DIR ) )

            def import_all( s ):

                return [ s.api.post( '/add_files/add_file', { 'path' : os.path.join( IMAGE_DIR, name ) } )[ 'hash' ] for name in names ]


            hashes = hydrus_driver.run_client( db_dir, import_all, port = PORT )

            with open( os.path.join( db_dir, 'imported.json' ), 'w' ) as f:

                json.dump( [ [ name, h ] for ( name, h ) in zip( names, hashes ) ], f )



        elif step == 'search':

            hydrus_driver.run_client( db_dir, lambda s: s.write( 'maintain_similar_files_search_for_potential_duplicates', 4 ) )

        elif step == 'resolve':

            pin_randomness()

            def resolve( s ):

                setup_rules( s )

                passes = run_rules( s )

                # a human approves one pair waiting on the capped rule and
                # denies another, then the rules run again
                [ capped ] = [ r for r in s.read( 'duplicates_auto_resolution_rules_with_counts' ) if r.GetName() == 'pixel-perfect gifs vs pngs' ]

                pending = sorted( s.read( 'duplicates_auto_resolution_pending_action_pairs', capped ), key = lambda p: ( p[0].GetHash(), p[1].GetHash() ) )

                decisions = []

                if len( pending ) >= 2:

                    s.write( 'duplicates_auto_resolution_approve_pending_pairs', capped, [ pending[0] ] )
                    s.write( 'duplicates_auto_resolution_deny_pending_pairs', capped, [ pending[1] ] )

                    decisions = [ [ pending[0][0].GetHash().hex(), pending[0][1].GetHash().hex(), 'approve' ], [ pending[1][0].GetHash().hex(), pending[1][1].GetHash().hex(), 'deny' ] ]


                passes += run_rules( s )

                rules = [ json.loads( json.dumps( r.GetSerialisableTuple() ) ) for r in s.read( 'duplicates_auto_resolution_rules_with_counts' ) ]

                return ( rules, passes, decisions )


            ( rules, passes, decisions ) = hydrus_driver.run_client( db_dir, resolve )

            with open( os.path.join( db_dir, 'rules.json' ), 'w' ) as f:

                json.dump( { 'rules' : rules, 'passes' : passes, 'decisions' : decisions }, f )




        return


    generate()

    work = tempfile.mkdtemp( prefix = 'hydrus_auto_resolution_' )

    try:

        db_dir = os.path.join( work, 'db' )

        hydrus_driver.run_in_subprocess( __file__, 'import', db_dir )
        hydrus_driver.run_in_subprocess( __file__, 'search', db_dir )
        hydrus_driver.run_in_subprocess( __file__, 'resolve', db_dir )

        with open( os.path.join( db_dir, 'imported.json' ) ) as f:

            files = json.load( f )


        with open( os.path.join( db_dir, 'rules.json' ) ) as f:

            run = json.load( f )


        result = record( db_dir, run[ 'rules' ] )
        result[ 'files' ] = files
        result[ 'passes' ] = run[ 'passes' ]
        result[ 'decisions' ] = run[ 'decisions' ]

        for name in ( 'imported.json', 'rules.json' ):

            os.remove( os.path.join( db_dir, name ) )


        with tarfile.open( DB_OUT, 'w:gz' ) as tar:

            for name in sorted( os.listdir( db_dir ) ):

                if name.endswith( '.db' ) or name == 'client_files':

                    tar.add( os.path.join( db_dir, name ), arcname = name )




    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "files" ] )} files, {result[ "passes" ]} passes' )


if __name__ == '__main__':

    main()
