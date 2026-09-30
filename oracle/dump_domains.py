#!/usr/bin/env python3
"""Record the reference's domain helpers over a corpus built from the public
suffix list: the registrable ("second level") domain, every applicable domain
(most specific first), and www removal. These decide which network session a
cookie lives in and which domain rules apply to a URL.

Usage: python oracle/dump_domains.py > oracle/fixtures/domains.json
"""

import json
import os
import random
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.client.networking import ClientNetworkingFunctions as F

import idna

rng = random.Random( 688 )

text = open( os.path.join( os.path.dirname( __file__ ), '..', 'static', 'public_suffix_list.dat' ), encoding = 'utf-8' ).read()
rules = [ line.split()[0] for line in text.splitlines() if line.strip() and not line.startswith( '//' ) ]

def label():
    
    return rng.choice( [ 'a', 'www', 'x1', 'foo', 'city', 'Mixed', 'b-c', '食狮', 'xn--p1ai' ] )
    

inputs = [
    'example.com', 'www.example.com', 'a.b.example.co.uk', 'example.co.uk', 'co.uk', 'uk', 'foo.github.io', 'github.io',
    'localhost', '127.0.0.1', '256.1.1.1', '1.2.3', 'sub.city.kawasaki.jp', 'city.kawasaki.jp', 'x.kawasaki.jp', 'kawasaki.jp',
    'www.ck', 'a.www.ck', 'foo.bar.unknowntld', 'bar.unknowntld', 'unknowntld', 'xn--p1ai', 'пример.рф', 'xn--e1afmkfd.xn--p1ai',
    'a.b.c.d.e.f.com', '.example.com', 'EXAMPLE.COM', 'Www.Example.Co.Uk', 'example.com.', 'example.com:8080', 'www.www.example.com',
    'www2.example.com', 'wwwexample.com', 'www.com', 'www.co.uk', 'example。com', '', '.', 'a..b.com', '[::1]',
]

for rule in rng.sample( rules, 1500 ):
    
    base = rule.lstrip( '!' ).replace( '*', label() )
    
    for prefix in rng.sample( [ '', 'a.', 'www.', 'www.b.', 'x.y.z.' ], 2 ):
        
        domain = prefix + base
        
        if rng.random() < 0.2:
            
            try:
                
                domain = idna.encode( domain ).decode( 'ascii' )
                
            except Exception:
                
                pass
                
            
        
        if rng.random() < 0.1:
            
            domain = domain.upper()
            
        
        inputs.append( domain )
        
    

cases = []

for domain in dict.fromkeys( inputs ):
    
    cases.append( [ domain, F.ConvertDomainIntoSecondLevelDomain( domain ), F.ConvertDomainIntoAllApplicableDomains( domain ), F.RemoveWWWFromDomain( domain ) ] )
    

json.dump( { 'columns': [ 'domain', 'second_level_domain', 'all_applicable_domains', 'without_www' ], 'cases': cases }, sys.stdout, ensure_ascii = False )
sys.stdout.write( '\n' )
