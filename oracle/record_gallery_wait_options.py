#!/usr/bin/env python3
"""Record the reference's gallery token spacing for each of the three options.

`NetworkBandwidthManager.TryToConsumeAGalleryToken` takes its delay from
'gallery_page_wait_period_pages' (download pages), '..._subscriptions' and
'watcher_page_wait_period'. This sets the three real options to different
values on a real `ClientOptions`, holds time still, and records, for each kind,
a token taken, asked again one second before the wait is up, exactly when it is
up, and one second after; a second domain and the other two kinds stay
independent.

Output: oracle/fixtures/gallery_wait_options.json
"""
import json
import os
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.core import HydrusTime
from hydrus.client import ClientOptions
from hydrus.client.networking import ClientNetworkingBandwidth

WAITS = { 'download page' : ( 'gallery_page_wait_period_pages', 7 ), 'subscription' : ( 'gallery_page_wait_period_subscriptions', 11 ), 'watcher' : ( 'watcher_page_wait_period', 13 ) }

options = ClientOptions.ClientOptions()

for ( option, value ) in WAITS.values():
    
    options.SetInteger( option, value )
    

class FakeController( object ):
    
    new_options = options
    

ClientNetworkingBandwidth.CG.client_controller = FakeController()

clock = [ 1_000_000 ]

HydrusTime.GetNow = lambda: clock[0]

manager = ClientNetworkingBandwidth.NetworkBandwidthManager()

steps = []

def ask( kind, sld, at ):
    
    clock[0] = at
    
    ( consumed, next_timestamp ) = manager.TryToConsumeAGalleryToken( sld, kind )
    
    steps.append( { 'kind' : kind, 'sld' : sld, 'now' : at, 'consumed' : consumed, 'next' : next_timestamp } )
    

T = 1_000_000
START = T + 10_000

for kind in WAITS:
    
    delay = WAITS[ kind ][1]
    
    # (each kind starts well apart, so only its own history matters)
    t = START + { 'download page' : 0, 'subscription' : 1000, 'watcher' : 2000 }[ kind ]
    
    ask( kind, 'site.example', t )
    ask( kind, 'site.example', t + delay - 1 )
    ask( kind, 'site.example', t + delay )
    ask( kind, 'site.example', t + delay + 1 )
    ask( kind, 'other.example', t + delay + 1 )

# the kinds do not share history
ask( 'download page', 'shared.example', START + 5000 )
ask( 'subscription', 'shared.example', START + 5000 )
ask( 'watcher', 'shared.example', START + 5000 )
ask( 'download page', 'shared.example', START + 5001 )
ask( 'subscription', 'shared.example', START + 5001 )
ask( 'watcher', 'shared.example', START + 5001 )

out = { 'waits' : { kind : value for ( kind, ( option, value ) ) in WAITS.items() }, 'options' : { kind : option for ( kind, ( option, value ) ) in WAITS.items() }, 'steps' : steps }

path = os.path.join( os.path.dirname( __file__ ), 'fixtures', 'gallery_wait_options.json' )

with open( path, 'w' ) as f:
    
    json.dump( out, f, indent = 1 )
    f.write( '\n' )
    

print( 'wrote', path )
