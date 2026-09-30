#!/usr/bin/env python3
"""Record the reference's in-memory predicate tests on real files.

Duplicates auto-resolution tests the two files of a pair against predicates
without the database (`Predicate.TestMediaResult`) and compares their
properties (`Predicate.ExtractComparableValueFromMediaResult`). This boots
the reference on the `basic` fixture database, loads every file's media
result, and records:

- for a corpus of predicates the reference can test in memory (system
  predicates parsed from text, plus tag, namespace and wildcard predicates
  built from the files' own tags), each predicate's stored form and which
  files match;
- for every comparable property, each file's value.

"Now" and the time zone are recorded, since ages are relative to now.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_media_tests.py
"""

import json
import os
import shutil
import sys
import tarfile
import tempfile
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

OUT = os.path.join( HERE, 'fixtures', 'media_tests.json' )

NUMBER_OPS = [ '<', '>', '=', '~=', '!=', '<=', '>=' ]


def system_texts( tag_services ):

    texts = [ 'system:inbox', 'system:archive' ]

    for ft in [ 'image', 'jpeg', 'png', 'gif', 'video', 'audio', 'image, video', 'webm', 'pdf', 'application' ]:

        texts.append( f'system:filetype is {ft}' )
        texts.append( f'system:filetype is not {ft}' )


    for ( name, values ) in [
        ( 'width', [ 0, 1, 50, 64, 100, 500 ] ),
        ( 'height', [ 0, 1, 50, 64, 100, 500 ] ),
        ( 'duration', [ '0 seconds', '1 second', '5 seconds', '500 milliseconds' ] ),
        ( 'framerate', [ '0fps', '1fps', '10fps', '25fps', '30fps' ] ),
        ( 'number of frames', [ 0, 1, 2, 10, 50 ] ),
        ( 'number of urls', [ 0, 1, 2 ] ),
    ]:

        for op in NUMBER_OPS:

            for value in values:

                texts.append( f'system:{name} {op} {value}' )



        texts.append( f'system:has {name}' )
        texts.append( f'system:no {name}' )


    for op in [ '<', '>', '=', '~=', '!=' ]:

        for value in [ '100 pixels', '1 kilopixels', '4096 pixels', '10 kilopixels', '1 megapixels' ]:

            texts.append( f'system:num pixels {op} {value}' )


        for count in [ 0, 1, 2, 3, 5 ]:

            texts.append( f'system:number of tags {op} {count}' )
            texts.append( f'system:number of creator tags {op} {count}' )
            texts.append( f'system:number of unnamespaced tags {op} {count}' )
            texts.append( f'system:number of c*r tags {op} {count}' )



    for op in [ '=', 'wider than', 'taller than', '~=' ]:

        for ratio in [ '1:1', '4:3', '16:9', '3:4' ]:

            texts.append( f'system:ratio {op} {ratio}' )



    for flag in [ 'audio', 'exif', 'icc profile', 'human readable embedded metadata', 'transparency', 'forced filetype', 'xmp', 'iptc', 'software/source metadata' ]:

        texts.append( f'system:has {flag}' )
        texts.append( f'system:no {flag}' )


    for when in [ 'import time', 'modified time', 'archived time', 'last viewed time' ]:

        for op in [ '<', '>', '~=' ]:

            for age in [ '1 hour', '7 days', '1 year', '30 years' ]:

                texts.append( f'system:{when} {op} {age}' )



        for op in [ '<', '>', '=', '~=' ]:

            for date in [ '2000-01-01', '2020-06-15', '2030-01-01' ]:

                texts.append( f'system:{when} {op} {date}' )




    for url in [ 'example.com', 'safebooru.org', 'www.example.com' ]:

        texts.append( f'system:has url with domain {url}' )
        texts.append( f'system:does not have url with domain {url}' )


    for regex in [ r'post', r'\d+', r'^https://' ]:

        texts.append( f'system:has url matching regex {regex}' )
        texts.append( f'system:does not have url matching regex {regex}' )


    for op in [ '<', '>', '=' ]:

        texts.append( f'system:number of urls {op} 1' )


    for service in tag_services:

        for tag in [ 'creator:somebody', 'blue', 'series:example' ]:

            texts.append( f'system:has tag in "{service}", ignoring siblings/parents, with status in current: "{tag}"' )
            texts.append( f'system:has tag in "{service}", with status in current, pending: "{tag}"' )
            texts.append( f'system:does not have tag in "{service}", with status in current: "{tag}"' )



    return texts


def record( session, manifest ):

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.metadata import ClientTags
    from hydrus.client.search import ClientSearchParseSystemPredicates
    from hydrus.client.search import ClientSearchPredicate
    from hydrus.core import HydrusTags

    hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in manifest[ 'files' ] ]

    media_results = session.read( 'media_results', hashes )

    tag_services = [ s.GetName() for s in session.controller.services_manager.GetServices( HC.REAL_TAG_SERVICES ) ]

    predicates = []

    for text in system_texts( tag_services ):

        try:

            [ predicate ] = ClientSearchParseSystemPredicates.ParseSystemPredicateStringsToPredicates( [ text ] )

        except Exception as e:

            continue


        if predicate.CanTestMediaResult():

            predicates.append( ( text, predicate ) )



    # tag predicates from the files' own tags
    all_tags = set()

    for mr in media_results:

        all_tags.update( mr.GetTagsManager().GetCurrentAndPending( CC.COMBINED_TAG_SERVICE_KEY, ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL ) )


    namespaces = sorted( { HydrusTags.SplitTag( t )[0] for t in all_tags } )

    for tag in sorted( all_tags )[ : 40 ]:

        for inclusive in ( True, False ):

            predicates.append( ( f'tag {tag}', ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_TAG, tag, inclusive ) ) )



    for namespace in namespaces + [ 'nonexistent', 'c*r' ]:

        for inclusive in ( True, False ):

            predicates.append( ( f'namespace {namespace}', ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_NAMESPACE, namespace, inclusive ) ) )



    for wildcard in [ '*', 'b*', '*e*', 'creator:*', '*:*', 'x*z' ]:

        for inclusive in ( True, False ):

            predicates.append( ( f'wildcard {wildcard}', ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_WILDCARD, wildcard, inclusive ) ) )



    # an OR of two
    ors = [ p for ( t, p ) in predicates if t in ( 'system:filetype is png', 'system:has audio' ) ]

    if len( ors ) == 2:

        predicates.append( ( 'png or audio', ClientSearchPredicate.Predicate( ClientSearchPredicate.PREDICATE_TYPE_OR_CONTAINER, ors ) ) )


    now_ms = int( time.time() * 1000 )

    tests = []

    for ( text, predicate ) in predicates:

        matching = sorted( mr.GetHash().hex() for mr in media_results if predicate.TestMediaResult( mr ) )

        tests.append( {
            'text' : text,
            'serialised' : json.loads( json.dumps( predicate.GetSerialisableTuple() ) ),
            'matching' : matching
        } )


    values = {}

    for predicate_type in ClientSearchPredicate.PREDICATE_TYPES_WE_CAN_EXTRACT_FROM_MEDIA_RESULTS:

        predicate = ClientSearchPredicate.Predicate( predicate_type )

        values[ str( predicate_type ) ] = { mr.GetHash().hex() : predicate.ExtractComparableValueFromMediaResult( mr ) for mr in media_results }


    return {
        'now_ms' : now_ms,
        'utc_offset_seconds' : -time.timezone if time.localtime().tm_isdst == 0 else -time.altzone,
        'tests' : tests,
        'values' : values,
    }


def main():

    manifest = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

    work = tempfile.mkdtemp( prefix = 'hydrus_media_tests_' )

    try:

        db_dir = os.path.join( work, 'db' )

        with tarfile.open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.tar.gz' ) ) as tar:

            tar.extractall( db_dir, filter = 'data' )


        result = hydrus_driver.run_client( db_dir, lambda s: record( s, manifest ) )

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "tests" ] )} predicates' )


if __name__ == '__main__':

    main()
