#!/usr/bin/env python3
"""Record the reference's default bandwidth rules.

`ClientDefaults.SetDefaultBandwidthManagerRules` is what the bandwidth
review's "reset to default rules" button applies. This runs it against a
manager that records each `SetRules` call, in order.

Usage: python oracle/dump_bandwidth_defaults.py > oracle/fixtures/bandwidth_defaults.json
"""

import json
import os
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.client import ClientDefaults

class Recorder( object ):
    
    def __init__( self ):
        
        self.calls = []
        
    
    def SetRules( self, network_context, rules ):
        
        self.calls.append( [ [ network_context.context_type, network_context.context_data ], sorted( [ list( r ) for r in rules.GetRules() ], key = lambda r: ( r[0], -1 if r[1] is None else r[1], r[2] ) ) ] )
        
    

recorder = Recorder()

ClientDefaults.SetDefaultBandwidthManagerRules( recorder )

print( json.dumps( { 'rules' : recorder.calls }, indent = 1 ) )
