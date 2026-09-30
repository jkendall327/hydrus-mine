#!/usr/bin/env python3
"""Pickle cookie jars the way the reference stores network sessions, and
record what they hold, for hydrus-legacy's pickle reader.

Each case is a jar (pickled at protocols 2-5, as installs from different
Python versions may have) or a whole pickled `requests.Session` (session
containers before version 2), with the cookies Python reads back from it.

Usage: python oracle/dump_cookie_jars.py > oracle/fixtures/cookie_jars.json
"""

import json
import os
import pickle
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

import requests

from hydrus.client.networking import ClientNetworkingSessions as S

def jar_with( cookies ):
    
    session = requests.Session()
    
    for ( name, value, domain, path, expires, secure, rest ) in cookies:
        
        S.AddCookieToSession( session, name, value, domain, path, expires, secure = secure, rest = rest )
        
    
    return session
    

sets = [
    [],
    [ ( 'sid', 'abc', '.example.com', '/', 1900000000, False, None ) ],
    [
        ( 'sid', 'abc', '.example.com', '/', 1900000000, True, { 'HttpOnly': None } ),
        ( 'pref', 'x y', 'example.com', '/a', None, False, None ),
        ( 'empty', '', 'example.com', '/', 0, False, None ),
        ( 'unicode', 'värde ✓', 'sub.example.co.uk', '/path/deeper', 4102444800, False, { 'SameSite': 'Lax' } ),
        ( 'big', 'b' * 300, 'another.org', '/', 2 ** 40, False, None ),
        ( 'neg', '1', 'another.org', '/', -5, False, None ),
    ],
]

cases = []

for cookies in sets:
    
    session = jar_with( cookies )
    
    jar = S.GetRequestsSessionCookieJar( session )
    
    expected = [
        { 'name': c.name, 'value': c.value, 'domain': c.domain, 'path': c.path, 'expires': c.expires, 'secure': c.secure, 'discard': c.discard, 'rest': c._rest }
        for c in jar
    ]
    
    for protocol in ( 2, 3, 4, 5 ):
        
        cases.append( { 'what': 'jar', 'protocol': protocol, 'pickle': pickle.dumps( jar, protocol = protocol ).hex(), 'cookies': expected } )
        
    
    cases.append( { 'what': 'session', 'protocol': 4, 'pickle': pickle.dumps( session, protocol = 4 ).hex(), 'cookies': expected } )
    

json.dump( { 'cases': cases }, sys.stdout, ensure_ascii = False )
sys.stdout.write( '\n' )
