#!/usr/bin/env python3
"""Record the reference client's Client API responses for conformance scenarios.

For each scenario in oracle/scenarios/, unpack its fixture database, boot the
reference client on it with the Client API enabled, send every step, and
write oracle/recordings/<scenario>.json. Read-only scenarios share a boot;
mutating ones get a fresh fixture each.

Placeholders substituted in request bodies before sending:
    {MEDIA}   absolute path of oracle/fixtures/import_media
Values replaced in responses before saving (so recordings don't depend on
where they were made):
    the db dir's absolute path  ->  {DB_DIR}
    {MEDIA}'s absolute path     ->  {MEDIA}

Usage: python oracle/record_api.py [scenario names...]    (default: all)
"""

import hashlib
import json
import os
import sys
import tarfile
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

SCENARIOS_DIR = os.path.join( HERE, 'scenarios' )
RECORDINGS_DIR = os.path.join( HERE, 'recordings' )
FIXTURES_DIR = os.path.join( HERE, 'fixtures' )
MEDIA_DIR = os.path.join( FIXTURES_DIR, 'import_media' )

PORT = 45902


def load_scenarios( names ):

    all_names = sorted( n[ :-5 ] for n in os.listdir( SCENARIOS_DIR ) if n.endswith( '.json' ) )

    for name in names:

        if name not in all_names:

            raise SystemExit( f'no scenario named {name}' )



    return [ json.load( open( os.path.join( SCENARIOS_DIR, n + '.json' ) ) ) for n in ( names or all_names ) ]


def substitute( value ):

    if isinstance( value, str ):

        return value.replace( '{MEDIA}', MEDIA_DIR )


    if isinstance( value, list ):

        return [ substitute( v ) for v in value ]


    if isinstance( value, dict ):

        return { k: substitute( v ) for ( k, v ) in value.items() }


    return value


def unsubstitute( value, db_dir ):

    if isinstance( value, str ):

        return value.replace( db_dir, '{DB_DIR}' ).replace( MEDIA_DIR, '{MEDIA}' )


    if isinstance( value, list ):

        return [ unsubstitute( v, db_dir ) for v in value ]


    if isinstance( value, dict ):

        return { unsubstitute( k, db_dir ): unsubstitute( v, db_dir ) for ( k, v ) in value.items() }


    return value


def access_key_header( spec, manifest ):
    """The Hydrus-Client-API-Access-Key header value for a step's `key` spec:
    a named key from the fixture manifest, `none` for no header, or any other
    string sent verbatim (to test malformed keys)."""
    
    if spec == 'none':
        
        return None
        
    
    return manifest[ 'access_keys' ].get( spec, spec )
    

def describe_body( content_type, body, db_dir ):

    if 'json' in content_type:

        try:

            return { 'json': unsubstitute( json.loads( body ), db_dir ) }

        except ValueError:

            pass



    if content_type.startswith( 'text/' ):

        try:

            return { 'text': unsubstitute( body.decode( 'utf-8' ), db_dir ) }

        except UnicodeDecodeError:

            pass



    return { 'sha256': hashlib.sha256( body ).hexdigest(), 'length': len( body ) }


def run_steps( session, scenario, manifest ):

    import hydrus_driver

    db_dir = session.controller.db_dir

    responses = []

    for step in scenario[ 'steps' ]:

        api = hydrus_driver.Api( session.port, access_key = None )
        headers = {}
        
        key = access_key_header( step.get( 'key', 'full' ), manifest )
        
        if key is not None:
            
            headers[ 'Hydrus-Client-API-Access-Key' ] = key
            
        
        query = [ ( k, substitute( v ) ) for ( k, v ) in step.get( 'query', [] ) ]

        data = None
        json_body = None

        if 'json' in step:

            json_body = substitute( step[ 'json' ] )

        elif 'body_file' in step:

            with open( os.path.join( FIXTURES_DIR, step[ 'body_file' ] ), 'rb' ) as f:

                data = f.read()


            headers[ 'Content-Type' ] = 'application/octet-stream'


        path = step[ 'path' ]

        if query:

            import urllib.parse

            path += '?' + urllib.parse.urlencode( query )


        ( status, content_type, body ) = api.request( step[ 'method' ], path, json_body = json_body, data = data, headers = headers )

        response = { 'status': status, 'content_type': content_type.split( ';' )[0].strip() }
        response.update( describe_body( content_type, body, db_dir ) )

        responses.append( response )

        if not scenario[ 'read_only' ]:

            # the reference applies some writes asynchronously (pubsub to managers);
            # give it a beat so the next read sees a settled state
            time.sleep( 0.05 )



    return responses


def unpack_fixture( name ):

    work = tempfile.mkdtemp( prefix = 'hydrus_record_' )
    db_dir = os.path.join( work, 'db' )

    with tarfile.open( os.path.join( FIXTURES_DIR, 'legacy_db', name + '.tar.gz' ) ) as tar:

        tar.extractall( db_dir, filter = 'data' )


    return db_dir


def record_group( scenarios ):
    """Run scenarios (all on the same fixture) in one boot, in a child process."""

    import hydrus_driver

    fixture = scenarios[0][ 'fixture' ]
    manifest = json.load( open( os.path.join( FIXTURES_DIR, 'legacy_db', fixture + '.manifest.json' ) ) )
    db_dir = unpack_fixture( fixture )

    def hook( session ):

        results = {}

        for sc in scenarios:

            started = int( time.time() * 1000 )

            results[ sc[ 'name' ] ] = ( started, run_steps( session, sc, manifest ) )


        return results


    results = hydrus_driver.run_client( db_dir, hook, port = PORT )

    os.makedirs( RECORDINGS_DIR, exist_ok = True )

    for sc in scenarios:

        ( started, responses ) = results[ sc[ 'name' ] ]

        recording = {
            'scenario': sc[ 'name' ],
            'reference_version': 688,
            'recorded_at_ms': started,
            'responses': responses,
        }

        with open( os.path.join( RECORDINGS_DIR, sc[ 'name' ] + '.json' ), 'w' ) as f:

            json.dump( recording, f, indent = 1, ensure_ascii = False, sort_keys = True )
            f.write( '\n' )


        bad = sum( 1 for r in responses if r[ 'status' ] >= 500 )

        print( f'recorded {sc[ "name" ]}: {len( responses )} steps ({bad} server errors)' )



def main():

    args = sys.argv[ 1: ]

    if args and args[0] == '--child':

        # child process: record exactly the named scenarios in one boot
        record_group( load_scenarios( args[ 1: ] ) )

        return


    scenarios = load_scenarios( args )

    read_only = [ sc[ 'name' ] for sc in scenarios if sc[ 'read_only' ] ]
    mutating = [ sc[ 'name' ] for sc in scenarios if not sc[ 'read_only' ] ]

    import hydrus_driver

    if read_only:

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', *read_only )


    for name in mutating:

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', name )



if __name__ == '__main__':

    main()

