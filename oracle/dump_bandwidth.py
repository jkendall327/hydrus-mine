#!/usr/bin/env python3
"""Record the reference's bandwidth rules and usage tracking.

Bandwidth rules cap requests or bytes over a span of seconds (or the month),
per network context; a tracker counts usage in calendar buckets. This drives
`HydrusNetworking.BandwidthTracker` and `BandwidthRules`, and
`ClientNetworkingBandwidth.NetworkBandwidthManager`, through random
sequences of reported usage and time passing (the clock is patched), and
records every answer: usage per rule, the waiting estimates, and whether a
request may start, a download continue or work begin.

- `trackers`: one tracker under a random set of rules;
- `managers`: rules on several contexts (global, every domain, one domain
  with rules like the owner's for one site, a subscription), jobs asking to
  start as the network engine asks, and gallery tokens.

Spans are chosen away from the few where the tracker's pruning, which runs
every two minutes, changes what a span sees (the reference's own quirk).

Usage: python oracle/dump_bandwidth.py > oracle/fixtures/bandwidth.json
"""

import json
import os
import random
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusTime
from hydrus.core.networking import HydrusNetworking

NOW = [ 0 ]

HydrusTime.GetNow = lambda: NOW[ 0 ]
HydrusTime.GetNowFloat = lambda: float( NOW[ 0 ] )

from hydrus.client import ClientConstants as CC
from hydrus.client.networking import ClientNetworkingBandwidth
from hydrus.client.networking import ClientNetworkingContexts

class FakeOptions( object ):
    
    def GetInteger( self, name ):
        
        return { 'gallery_page_wait_period_pages' : 15, 'gallery_page_wait_period_subscriptions' : 5, 'watcher_page_wait_period' : 5 }[ name ]
        
    

class FakeController( object ):
    
    new_options = FakeOptions()
    

# the gallery token delays are the reference's default options
ClientNetworkingBandwidth.CG.client_controller = FakeController()

rng = random.Random( 94 )

DATA = HC.BANDWIDTH_TYPE_DATA
REQUESTS = HC.BANDWIDTH_TYPE_REQUESTS

DELTAS = [ 1, 2, 4, 5, 10, 15, 30, 60, 120, 180, 300, 420, 600, 3600, 7200, 86400, 172800, 604800, None ]

MB = 1024 * 1024


def random_rule():

    kind = rng.choice( [ DATA, REQUESTS ] )
    delta = rng.choice( DELTAS )

    if kind == REQUESTS:

        maximum = rng.choice( [ 0, 1, 1, 2, 3, 5, 10, 80, 1000 ] )

    else:

        maximum = rng.choice( [ 0, 1, 1000, 50000, MB, 64 * MB, 1024 * MB ] )


    return ( kind, delta, maximum )


def advance():

    r = rng.random()

    if r < 0.5:

        step = rng.choice( [ 0, 0, 1, 1, 2, 3 ] )

    elif r < 0.8:

        step = rng.randint( 4, 600 )

    elif r < 0.95:

        step = rng.randint( 600, 3 * 86400 )

    else:

        step = rng.randint( 3 * 86400, 40 * 86400 )


    NOW[ 0 ] += step

    return step


def rule_answers( rules, tracker ):

    out = { 'rules' : [] }

    for ( kind, delta, maximum ) in sorted( rules.GetRules(), key = lambda r: ( r[0], -1 if r[1] is None else r[1], r[2] ) ):

        usage = tracker.GetUsage( kind, delta )
        estimate = tracker.GetWaitingEstimate( kind, delta, maximum )

        out[ 'rules' ].append( [ usage, estimate ] )


    out[ 'can_start' ] = rules.CanStartRequest( tracker )
    out[ 'can_continue' ] = rules.CanContinueDownload( tracker )
    out[ 'can_do_work_30' ] = rules.CanDoWork( tracker, 1, MB, threshold = 30 )
    out[ 'can_do_work_90' ] = rules.CanDoWork( tracker, 1, MB, threshold = 90 )
    out[ 'waiting_estimate' ] = rules.GetWaitingEstimate( tracker )

    return out


def tracker_case():

    NOW[ 0 ] = rng.randint( 1577836800, 1790000000 ) # 2020 to 2026

    start = NOW[ 0 ]

    rules = HydrusNetworking.BandwidthRules()

    for _ in range( rng.randint( 1, 4 ) ):

        rules.AddRule( *random_rule() )


    tracker = HydrusNetworking.BandwidthTracker()

    steps = []

    for _ in range( rng.randint( 10, 60 ) ):

        step = { 'advance' : advance() }

        r = rng.random()

        if r < 0.45:

            n = rng.choice( [ 1, 1, 1, 2, 5 ] )

            tracker.ReportRequestUsed( num_requests = n )

            step[ 'requests' ] = n

        elif r < 0.85:

            n = rng.choice( [ 1, 500, 40000, MB, 3 * MB, 70 * MB ] )

            tracker.ReportDataUsed( n )

            step[ 'bytes' ] = n


        step[ 'answers' ] = rule_answers( rules, tracker )

        steps.append( step )


    counters = [ sorted( d ) for d in tracker.GetSerialisableTuple()[2] ]

    return {
        'start' : start,
        'rules' : sorted( [ list( r ) for r in rules.GetRules() ], key = lambda r: ( r[0], -1 if r[1] is None else r[1], r[2] ) ),
        'steps' : steps,
        'counters' : counters,
    }


def context_tuple( c ):

    data = c.context_data

    if isinstance( data, bytes ):

        data = data.hex()


    return [ c.context_type, data ]


def manager_case():

    NOW[ 0 ] = rng.randint( 1577836800, 1790000000 )

    start = NOW[ 0 ]

    manager = ClientNetworkingBandwidth.NetworkBandwidthManager()

    from hydrus.client import ClientDefaults

    ClientDefaults.SetDefaultBandwidthManagerRules( manager )

    site = ClientNetworkingContexts.NetworkContext( CC.NETWORK_CONTEXT_DOMAIN, 'site.example' )

    site_rules = HydrusNetworking.BandwidthRules()

    # like the owner's rules for one site, scaled to show in a short run
    site_rules.AddRule( DATA, 86400, rng.choice( [ 2 * 1024 * MB, 64 * MB, 5 * MB ] ) )
    site_rules.AddRule( REQUESTS, 4, 1 )
    site_rules.AddRule( REQUESTS, 420, rng.choice( [ 80, 5 ] ) )

    manager.SetRules( site, site_rules )

    subscription_rules = HydrusNetworking.BandwidthRules()
    subscription_rules.AddRule( REQUESTS, 86400, rng.choice( [ 1000, 12 ] ) )
    subscription_rules.AddRule( DATA, 86400, rng.choice( [ 1024 * MB, 20 * MB ] ) )

    manager.SetRules( ClientNetworkingContexts.NetworkContext( CC.NETWORK_CONTEXT_SUBSCRIPTION ), subscription_rules )

    G = ClientNetworkingContexts.GLOBAL_NETWORK_CONTEXT
    D = lambda d: ClientNetworkingContexts.NetworkContext( CC.NETWORK_CONTEXT_DOMAIN, d )
    S = ClientNetworkingContexts.NetworkContext( CC.NETWORK_CONTEXT_SUBSCRIPTION, 'my sub: samus aran' )
    P = ClientNetworkingContexts.NetworkContext( CC.NETWORK_CONTEXT_DOWNLOADER_PAGE, bytes.fromhex( 'aa' * 32 ) )

    jobs = [
        [ G, D( 'site.example' ) ],
        [ G, D( 'img.site.example' ), D( 'site.example' ) ],
        [ G, D( 'other.example' ), S ],
        [ G, D( 'site.example' ), S ],
        [ G, D( 'cdn.other.example' ), D( 'other.example' ), P ],
    ]

    steps = []

    for _ in range( rng.randint( 20, 80 ) ):

        step = { 'advance' : rng.choice( [ 0, 0, 0, 1, 1, 2, 5, rng.randint( 6, 4000 ), rng.randint( 4000, 100000 ) ] ) }

        NOW[ 0 ] += step[ 'advance' ]

        job = rng.randrange( len( jobs ) )
        contexts = jobs[ job ]

        step[ 'job' ] = job

        r = rng.random()

        if r < 0.6:

            step[ 'started' ] = manager.TryToStartRequest( contexts )

            ( estimate, context ) = manager.GetWaitingEstimateAndContext( contexts )

            step[ 'estimate' ] = [ estimate, context_tuple( context ) ]

            if step[ 'started' ] and rng.random() < 0.7:

                n = rng.choice( [ 1000, 300000, 2 * MB, 9 * MB ] )

                manager.ReportDataUsed( contexts, n )

                step[ 'bytes' ] = n


        elif r < 0.8:

            step[ 'can_do_work' ] = manager.CanDoWork( contexts, threshold = 90 )
            step[ 'can_continue' ] = manager.CanContinueDownload( contexts )

        else:

            kind = rng.choice( [ 'download page', 'subscription', 'watcher' ] )
            sld = rng.choice( [ 'site.example', 'other.example' ] )
            delay = { 'download page' : 15, 'subscription' : 5, 'watcher' : 5 }[ kind ]

            ( consumed, next_timestamp ) = manager.TryToConsumeAGalleryToken( sld, kind )

            step[ 'token' ] = [ kind, sld, delay, consumed, next_timestamp ]


        steps.append( step )


    return {
        'start' : start,
        'rules' : [ [ context_tuple( c ), sorted( [ list( r ) for r in rules.GetRules() ], key = lambda r: ( r[0], -1 if r[1] is None else r[1], r[2] ) ) ] for ( c, rules ) in manager._network_contexts_to_bandwidth_rules.items() ],
        'jobs' : [ [ context_tuple( c ) for c in job ] for job in jobs ],
        'steps' : steps,
    }


def stored_case():
    """A manager and its usage containers as the reference stores them, with
    rules like the owner's on one site and usage on several contexts."""
    
    NOW[ 0 ] = 1759000000
    
    manager = ClientNetworkingBandwidth.NetworkBandwidthManager()
    
    from hydrus.client import ClientDefaults
    
    ClientDefaults.SetDefaultBandwidthManagerRules( manager )
    
    site = ClientNetworkingContexts.NetworkContext( CC.NETWORK_CONTEXT_DOMAIN, 'site.example' )
    
    site_rules = HydrusNetworking.BandwidthRules()
    site_rules.AddRule( DATA, 86400, 2 * 1024 * MB )
    site_rules.AddRule( DATA, 86400, 64 * MB )
    site_rules.AddRule( REQUESTS, 4, 1 )
    site_rules.AddRule( REQUESTS, 420, 80 )
    
    manager.SetRules( site, site_rules )
    
    G = ClientNetworkingContexts.GLOBAL_NETWORK_CONTEXT
    S = ClientNetworkingContexts.NetworkContext( CC.NETWORK_CONTEXT_SUBSCRIPTION, 'my sub: samus aran' )
    P = ClientNetworkingContexts.NetworkContext( CC.NETWORK_CONTEXT_DOWNLOADER_PAGE, bytes.fromhex( 'bb' * 32 ) )
    
    for _ in range( 30 ):
        
        NOW[ 0 ] += rng.choice( [ 1, 7, 600, 5000 ] )
        
        contexts = rng.choice( [ [ G, site ], [ G, site, S ], [ G, site, P ] ] )
        
        manager.ReportRequestUsed( contexts )
        manager.ReportDataUsed( contexts, rng.choice( [ 1000, MB, 5 * MB ] ) )
        
    
    containers = [ c.GetSerialisableTuple() for c in manager._tracker_container_names_to_tracker_containers.values() ]
    
    return {
        'manager' : manager.GetSerialisableTuple(),
        'containers' : containers,
        'facts' : {
            'rules' : sorted( [ [ context_tuple( c ), sorted( [ list( r ) for r in rules.GetRules() ], key = lambda r: ( r[0], -1 if r[1] is None else r[1], r[2] ) ) ] for ( c, rules ) in manager._network_contexts_to_bandwidth_rules.items() ], key = lambda x: ( x[0][0], x[0][1] or '' ) ),
            'usage' : sorted( [ [ context_tuple( c.network_context ), [ sorted( d ) for d in c.bandwidth_tracker.GetSerialisableTuple()[2] ] ] for c in manager._tracker_container_names_to_tracker_containers.values() ], key = lambda x: ( x[0][0], x[0][1] or '' ) ),
            'saved' : sorted( manager._tracker_container_names ),
        },
    }
    

def main():

    fixture = {
        'trackers' : [ tracker_case() for _ in range( 150 ) ],
        'managers' : [ manager_case() for _ in range( 40 ) ],
    }
    
    fixture[ 'stored' ] = stored_case()

    json.dump( fixture, sys.stdout, indent = None, sort_keys = True )
    sys.stdout.write( '\n' )


if __name__ == '__main__':

    main()
