#!/usr/bin/env python3
"""Time the same Client API request mix against the reference client and
hydrus-rs, on the same library.

1. copies <bench db> (from make_bench_db.py), boots the reference on the
   copy with the Client API on, and times the request mix;
2. imports that copy with `hydrus import-legacy --files in-place`, runs
   `hydrus serve` on it, and times the same mix.

Both sides are measured from this process with the same HTTP client, one
request at a time, after a warm-up pass.

Usage: python oracle/bench_api.py <bench db dir> <hydrus binary> [repeats]
"""

import hashlib
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

REF_PORT = 45911
RUST_PORT = 45912
KEY = hydrus_driver.ORACLE_ACCESS_KEY.hex()


def bench_hash( i ):

    return hashlib.sha256( b'bench %d' % i ).hexdigest()


def request_mix( n_files ):
    """(name, method, path, query or json body). Endpoints hydrus-rs doesn't
    serve yet are reported as such."""

    step = max( 1, n_files // 100 )
    hundred = [ bench_hash( i ) for i in range( 0, n_files, step ) ][ :100 ]
    one = bench_hash( n_files // 2 )

    return [
        ( 'get_services', 'GET', '/get_services', {} ),
        ( 'file_metadata, 1 file', 'GET', '/get_files/file_metadata', { 'hashes': json.dumps( [ one ] ) } ),
        ( 'file_metadata, 100 files', 'GET', '/get_files/file_metadata', { 'hashes': json.dumps( hundred ) } ),
        ( 'file_metadata, 100 files, basic', 'GET', '/get_files/file_metadata', { 'hashes': json.dumps( hundred ), 'only_return_basic_information': 'true' } ),
        ( 'search_tags "tag number 1"', 'GET', '/add_tags/search_tags', { 'search': 'tag number 1' } ),
        ( 'search_tags "character:*"', 'GET', '/add_tags/search_tags', { 'search': 'character:*' } ),
        ( 'search_files, popular tag', 'GET', '/get_files/search_files', { 'tags': json.dumps( [ 'tag number 0' ] ) } ),
        ( 'search_files, 2 tags', 'GET', '/get_files/search_files', { 'tags': json.dumps( [ 'tag number 5', 'series:tag number 7' ] ) } ),
        ( 'search_files, tag + system', 'GET', '/get_files/search_files', { 'tags': json.dumps( [ 'tag number 0', 'system:width > 2000' ] ) } ),
        ( 'search_files, everything', 'GET', '/get_files/search_files', { 'tags': json.dumps( [ 'system:everything' ] ) } ),
        ( 'add_tags, 1 file, 5 tags', 'POST', '/add_tags/add_tags', { 'hash': one, 'service_keys_to_tags': { '6c6f63616c2074616773': [ 'bench:a', 'bench:b', 'bench:c', 'bench:d', 'bench:e' ] } } ),
        ( 'add_tags, 100 files, 1 tag', 'POST', '/add_tags/add_tags', { 'hashes': hundred, 'service_keys_to_tags': { '6c6f63616c2074616773': [ 'bench:bulk' ] } } ),
        # the duplicate filter's reads (add_bench_duplicates.py gives the library some)
        ( 'potentials count', 'GET', '/manage_file_relationships/get_potentials_count', {} ),
        ( 'potentials count, distance 0', 'GET', '/manage_file_relationships/get_potentials_count', { 'max_hamming_distance': '0' } ),
        ( 'potentials count, popular tag', 'GET', '/manage_file_relationships/get_potentials_count', { 'tags_1': json.dumps( [ 'tag number 0' ] ) } ),
        ( 'potential pairs, batch of 50', 'GET', '/manage_file_relationships/get_potential_pairs', { 'max_num_pairs': '50' } ),
        ( 'potential pairs, group mode', 'GET', '/manage_file_relationships/get_potential_pairs', { 'group_mode': 'true' } ),
        ( 'random potentials', 'GET', '/manage_file_relationships/get_random_potentials', {} ),
        ( 'file_relationships, 100 files', 'GET', '/manage_file_relationships/get_file_relationships', { 'hashes': json.dumps( hundred ) } ),
    ]


def call( port, method, path, params ):

    headers = { 'Hydrus-Client-API-Access-Key': KEY }
    data = None

    if method == 'GET':

        if params:

            path += '?' + urllib.parse.urlencode( params )


    else:

        data = json.dumps( params ).encode( 'utf-8' )
        headers[ 'Content-Type' ] = 'application/json'


    request = urllib.request.Request( 'http://127.0.0.1:{}{}'.format( port, path ), data = data, headers = headers, method = method )

    started = time.perf_counter()

    try:

        with urllib.request.urlopen( request, timeout = 600 ) as response:

            response.read()
            status = response.status


    except urllib.error.HTTPError as e:

        e.read()
        status = e.code


    return ( status, time.perf_counter() - started )


def time_mix( port, mix, repeats ):

    results = {}

    for ( name, method, path, params ) in mix:

        ( status, _ ) = call( port, method, path, params )  # warm-up

        if status != 200:

            results[ name ] = None

            continue


        timings = [ call( port, method, path, params )[ 1 ] for _ in range( repeats ) ]

        results[ name ] = ( statistics.median( timings ), max( timings ) )


    return results


def wait_for( port, timeout = 60 ):

    deadline = time.time() + timeout

    while time.time() < deadline:

        try:

            call( port, 'GET', '/api_version', {} )

            return

        except OSError:

            time.sleep( 0.2 )



    raise Exception( 'server on port {} never came up'.format( port ) )


def main():

    source = os.path.abspath( sys.argv[ 1 ] )
    binary = os.path.abspath( sys.argv[ 2 ] )
    repeats = int( sys.argv[ 3 ] ) if len( sys.argv ) > 3 else 10

    with open( os.path.join( source, 'bench.json' ) ) as f:

        n_files = json.load( f )[ 'files' ]


    mix = request_mix( n_files )
    work = tempfile.mkdtemp( prefix = 'hydrus_bench_' )

    try:

        ref_db = os.path.join( work, 'reference' )
        shutil.copytree( source, ref_db )

        reference = hydrus_driver.run_client( ref_db, lambda s: time_mix( REF_PORT, mix, repeats ), port = REF_PORT )

        native = os.path.join( work, 'native' )
        subprocess.run( [ binary, 'import-legacy', ref_db, native, '--files', 'in-place' ], check = True, stdout = subprocess.DEVNULL )
        server = subprocess.Popen( [ binary, 'serve', native, '--port', str( RUST_PORT ) ], stdout = subprocess.DEVNULL, stderr = subprocess.DEVNULL )

        try:

            wait_for( RUST_PORT )
            rust = time_mix( RUST_PORT, mix, repeats )

        finally:

            server.terminate()
            server.wait()


    finally:

        shutil.rmtree( work, ignore_errors = True )


    print( '| request ({} files) | reference, median | hydrus-rs, median | speed-up |'.format( n_files ) )
    print( '|---|---|---|---|' )

    for ( name, _, _, _ ) in mix:

        r = reference.get( name )
        n = rust.get( name )
        fmt = lambda t: '{:.1f} ms'.format( t[ 0 ] * 1000 ) if t else 'n/a'
        speedup = '{:.0f}x'.format( r[ 0 ] / n[ 0 ] ) if r and n else ''

        print( '| {} | {} | {} | {} |'.format( name, fmt( r ), fmt( n ), speedup ) )



if __name__ == '__main__':

    main()
