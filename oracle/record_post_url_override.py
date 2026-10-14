#!/usr/bin/env python3
"""Record "Force file downloads to occur quickly after Post URL fetches".

The real client downloads one post of a loopback booru through a URL page,
with the global bandwidth rules set to one request a day (so the post page
uses the allowance and the post's file is held back). `FileSeed.WorkOnURL`
gives the file's network job `OverrideBandwidth( 3 )` if the option
'override_bandwidth_on_file_urls_from_post_urls' is on. With the option on,
it records how long after the post page the site was asked for the file; with
it off, whether the file had been asked for after 15 seconds.

The moments are those this script saw the site's requests, to 50 ms.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_post_url_override.py
       (writes fixtures/post_url_override.json)
"""

import json
import os
import shutil
import sys
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import flows
import hydrus_driver
import record_prefetch_outcomes as prefetch

OUT = os.path.join( HERE, 'fixtures', 'post_url_override.json' )

SITE_PORT = 45989
API_PORT = 45990

WATCH_SECONDS = 15


def run( session, site, option_on ):

    from hydrus.core.networking import HydrusNetworking
    from hydrus.client import ClientConstants as CC
    from hydrus.client.networking import ClientNetworkingContexts

    controller = session.controller

    flows.install( controller, site, prefetch.definitions )

    controller.new_options.SetBoolean( 'override_bandwidth_on_file_urls_from_post_urls', option_on )

    rules = HydrusNetworking.BandwidthRules()
    rules.AddRule( HC_REQUESTS(), 86400, 1 )

    controller.network_engine.bandwidth_manager.SetRules( ClientNetworkingContexts.GLOBAL_NETWORK_CONTEXT, rules )

    site.take_hits()

    session.api.post( '/add_urls/add_url', { 'url' : site.base + '/post/1', 'destination_page_name' : 'urls' } )

    started = time.time()
    seen = {}

    while time.time() - started < WATCH_SECONDS:

        for path in [ path for ( phase, path ) in site.hits ]:

            seen.setdefault( path, time.time() - started )


        if '/files/p1.jpg' in seen and not option_on:

            break


        if '/files/p1.jpg' in seen:

            time.sleep( 0.5 )

            break


        time.sleep( 0.05 )


    post = seen.get( '/post/1' )
    file = seen.get( '/files/p1.jpg' )

    return {
        'option_on' : option_on,
        'post_seen' : post is not None,
        'file_seen' : file is not None,
        'file_after_post_seconds' : None if post is None or file is None else round( file - post, 2 ),
        'watched_seconds' : WATCH_SECONDS,
    }


def HC_REQUESTS():

    from hydrus.core.networking import HydrusNetworking
    from hydrus.core import HydrusConstants as HC

    return HC.BANDWIDTH_TYPE_REQUESTS


def record( option_on ):

    work = tempfile.mkdtemp( prefix = 'hydrus_override_' )

    phases = [ prefetch.build_phase() ]

    try:

        with flows.Site( SITE_PORT, phases ) as site:

            return hydrus_driver.run_client( os.path.join( work, 'db' ), lambda session: run( session, site, option_on ), port = API_PORT, network = True )


    finally:

        shutil.rmtree( work, ignore_errors = True )



def main():

    # (a client each: the bandwidth used is remembered)
    which = sys.argv[ 1 ] if len( sys.argv ) > 1 else None

    if which in ( 'on', 'off' ):

        print( 'RESULT ' + json.dumps( record( which == 'on' ) ) )

        return


    import subprocess

    results = {}

    for name in ( 'on', 'off' ):

        proc = subprocess.run( [ sys.executable, __file__, name ], capture_output = True, text = True )

        lines = [ l for l in proc.stdout.splitlines() if l.startswith( 'RESULT ' ) ]

        if not lines:

            raise Exception( proc.stdout[ -2000: ] + proc.stderr[ -2000: ] )


        results[ name ] = json.loads( lines[ -1 ][ len( 'RESULT ' ): ] )


    with open( OUT, 'w' ) as f:

        json.dump( results, f, indent = 1, sort_keys = True )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
