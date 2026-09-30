#!/usr/bin/env python3
"""Record how the reference implementation parses search predicate text.

For a large corpus of `system:` predicate strings (every example in the
reference tests and API docs, plus systematic variations of every predicate:
operators, units, spacing, case and invalid inputs) this runs the reference
parser (`SystemPredicateParser` + `ClientSearchParseSystemPredicates`) and
records a normalised JSON description of the resulting `Predicate`, or the
fact that it was rejected. It does the same for Client API `tags` lists
(`ClientLocalServerCore.ConvertTagListToPredicates`).

Runtime state the parser consults (services looked up by name, URL classes,
the "interesting" file viewing canvases option) is stubbed with a small fixed
registry that is written into the fixture too, so the Rust test can resolve
names the same way. `dateparser` is pinned to a fixed "now" so relative times
are deterministic; each input is parsed a second time against another "now" to
record whether the result depends on the wall clock.

Usage:
    QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/dump_system_predicates.py \
        > oracle/fixtures/system_predicates.json
"""
import ast
import datetime
import json
import os
import re
import sys

os.environ.setdefault( 'QT_QPA_PLATFORM', 'offscreen' )

REPO_ROOT = os.path.join( os.path.dirname( os.path.abspath( __file__ ) ), '..' )

sys.path.insert( 0, REPO_ROOT )

import dateparser as real_dateparser

from hydrus.core import HydrusConstants as HC
from hydrus.core import HydrusExceptions

from hydrus.client import ClientConstants as CC
from hydrus.client import ClientGlobals as CG
from hydrus.client import ClientOptions
from hydrus.client import ClientServices
from hydrus.client.metadata import ClientTags
from hydrus.client.networking.api import ClientLocalServerCore
from hydrus.client.search import ClientNumberTest
from hydrus.client.search import ClientSearchParseSystemPredicates
from hydrus.client.search import ClientSearchPredicate
from hydrus.external import SystemPredicateParser

# ---------------------------------------------------------------------------
# Frozen clock for dateparser

BASE_NOW = datetime.datetime( 2026, 3, 31, 12, 0, 0 )
ALT_NOW = datetime.datetime( 2025, 7, 10, 6, 30, 0 )


class FrozenDateparser( object ):

    def __init__( self ):

        self.now = BASE_NOW


    def parse( self, text, settings = None ):

        settings = dict( settings or {} )
        settings[ 'RELATIVE_BASE' ] = self.now

        return real_dateparser.parse( text, settings = settings )



FROZEN_DATEPARSER = FrozenDateparser()

assert SystemPredicateParser.DATEPARSER_OK, 'the reference client uses dateparser; install it'

SystemPredicateParser.dateparser = FROZEN_DATEPARSER

# ---------------------------------------------------------------------------
# Stubbed runtime state

def stub_key( name ):

    return ( 'stub ' + name ).encode( 'utf-8' )


# ( name, service type, key, num_stars, allow_zero )
SERVICE_DEFINITIONS = [
    ( 'my files', HC.LOCAL_FILE_DOMAIN, CC.LOCAL_FILE_SERVICE_KEY, None, None ),
    ( 'Archive Box', HC.LOCAL_FILE_DOMAIN, stub_key( 'Archive Box' ), None, None ),
    ( 'trash', HC.LOCAL_FILE_TRASH_DOMAIN, CC.TRASH_SERVICE_KEY, None, None ),
    ( 'repository updates', HC.LOCAL_FILE_UPDATE_DOMAIN, CC.LOCAL_UPDATE_SERVICE_KEY, None, None ),
    ( 'combined local file domains', HC.COMBINED_LOCAL_FILE_DOMAINS, CC.COMBINED_LOCAL_FILE_DOMAINS_SERVICE_KEY, None, None ),
    ( 'hydrus local file storage', HC.HYDRUS_LOCAL_FILE_STORAGE, CC.HYDRUS_LOCAL_FILE_STORAGE_SERVICE_KEY, None, None ),
    ( 'deleted from anywhere', HC.COMBINED_DELETED_FILE, CC.COMBINED_DELETED_FILE_SERVICE_KEY, None, None ),
    ( 'all known files', HC.COMBINED_FILE, CC.COMBINED_FILE_SERVICE_KEY, None, None ),
    ( 'example file repo', HC.FILE_REPOSITORY, stub_key( 'example file repo' ), None, None ),
    ( 'example ipfs service', HC.IPFS, stub_key( 'example ipfs service' ), None, None ),
    ( 'my tags', HC.LOCAL_TAG, CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, None, None ),
    ( 'downloader tags', HC.LOCAL_TAG, CC.DEFAULT_LOCAL_DOWNLOADER_TAG_SERVICE_KEY, None, None ),
    ( 'example tag repo', HC.TAG_REPOSITORY, stub_key( 'example tag repo' ), None, None ),
    ( 'all known tags', HC.COMBINED_TAG, CC.COMBINED_TAG_SERVICE_KEY, None, None ),
    ( 'example local rating like service', HC.LOCAL_RATING_LIKE, stub_key( 'like' ), None, None ),
    ( 'favourites', HC.LOCAL_RATING_LIKE, stub_key( 'favourites' ), None, None ),
    ( 'example local rating numerical service', HC.LOCAL_RATING_NUMERICAL, stub_key( 'numerical' ), 5, True ),
    ( 'Ten Stars', HC.LOCAL_RATING_NUMERICAL, stub_key( 'Ten Stars' ), 10, False ),
    ( 'example local rating inc/dec service', HC.LOCAL_RATING_INCDEC, stub_key( 'incdec' ), None, None ),
    ( 'counter', HC.LOCAL_RATING_INCDEC, stub_key( 'counter' ), None, None ),
    # a like/dislike service named like a numerical rating, to show that the
    # service type (not the text) decides how a value is interpreted
    ( 'thumbs', HC.LOCAL_RATING_LIKE, stub_key( 'thumbs' ), None, None ),
]

URL_CLASS_NAMES = [ 'somebooru file page', 'otherbooru file page', 'Some Imageboard Thread' ]


class StubURLClass( object ):

    def __init__( self, name ):

        self._name = name


    def GetName( self ):

        return self._name



class StubDomainManager( object ):

    def __init__( self ):

        self._url_classes = [ StubURLClass( name ) for name in URL_CLASS_NAMES ]


    def GetURLClassFromName( self, name ):

        # same as NetworkDomainManager.GetURLClassFromName
        name_search = name.casefold()

        for url_class in self._url_classes:

            if url_class.GetName().casefold() == name_search:

                return url_class



        raise HydrusExceptions.DataMissing( 'Did not find URL Class called "{}"!'.format( name ) )



class StubServicesManager( object ):

    def __init__( self, services ):

        self._services_sorted = list( services )
        self._keys_to_services = { service.GetServiceKey() : service for service in services }


    def GetName( self, service_key ):

        return self._keys_to_services[ service_key ].GetName()


    def GetService( self, service_key ):

        return self._keys_to_services[ service_key ]


    def GetServiceKeyFromName( self, allowed_types, service_name ):

        # same as ServicesManager.GetServiceKeyFromName
        for service in self._services_sorted:

            if service.GetServiceType() in allowed_types and service.GetName() == service_name:

                return service.GetServiceKey()



        for service in self._services_sorted:

            if service.GetServiceType() in allowed_types and service.GetName().lower() == service_name.lower():

                return service.GetServiceKey()



        raise HydrusExceptions.DataMissing()


    def ServiceExists( self, service_key ):

        return service_key in self._keys_to_services



class StubNetworkEngine( object ):

    def __init__( self ):

        self.domain_manager = StubDomainManager()



class StubController( object ):

    def __init__( self ):

        self.new_options = ClientOptions.ClientOptions()

        services = []

        for ( name, service_type, key, num_stars, allow_zero ) in SERVICE_DEFINITIONS:

            service = ClientServices.GenerateService( key, service_type, name )

            if num_stars is not None:

                service._num_stars = num_stars
                service._allow_zero = allow_zero


            services.append( service )


        self.services_manager = StubServicesManager( services )
        self.network_engine = StubNetworkEngine()


    def IsBooted( self ):

        return True



CG.client_controller = StubController()

CANVAS_NAMES = { CC.CANVAS_MEDIA_VIEWER : 'media', CC.CANVAS_PREVIEW : 'preview', CC.CANVAS_CLIENT_API : 'client api' }

DEFAULT_VIEW_CANVASES = [ CANVAS_NAMES[ c ] for c in CG.client_controller.new_options.GetIntegerList( 'file_viewing_stats_interesting_canvas_types' ) ]

# ---------------------------------------------------------------------------
# Normalisation of reference predicates to JSON

PREDICATE_TYPE_NAMES = {
    getattr( ClientSearchPredicate, name ) : name[ len( 'PREDICATE_TYPE_' ): ].lower()
    for name in dir( ClientSearchPredicate )
    if name.startswith( 'PREDICATE_TYPE_' )
}

NUMBER_TEST_OPERATOR_NAMES = {
    ClientNumberTest.NUMBER_TEST_OPERATOR_LESS_THAN : 'less',
    ClientNumberTest.NUMBER_TEST_OPERATOR_LESS_THAN_OR_EQUAL_TO : 'less_or_equal',
    ClientNumberTest.NUMBER_TEST_OPERATOR_GREATER_THAN : 'greater',
    ClientNumberTest.NUMBER_TEST_OPERATOR_GREATER_THAN_OR_EQUAL_TO : 'greater_or_equal',
    ClientNumberTest.NUMBER_TEST_OPERATOR_EQUAL : 'equal',
    ClientNumberTest.NUMBER_TEST_OPERATOR_NOT_EQUAL : 'not_equal',
    ClientNumberTest.NUMBER_TEST_OPERATOR_APPROXIMATE_PERCENT : 'approx_percent',
    ClientNumberTest.NUMBER_TEST_OPERATOR_APPROXIMATE_ABSOLUTE : 'approx_absolute',
}

TAG_DISPLAY_NAMES = { ClientTags.TAG_DISPLAY_STORAGE : 'storage', ClientTags.TAG_DISPLAY_DISPLAY_ACTUAL : 'display' }


def norm_number_test( number_test ):

    return {
        'op' : NUMBER_TEST_OPERATOR_NAMES[ number_test.operator ],
        'value' : number_test.value,
        'extra' : number_test.extra_value,
    }


def norm_service_specifier( service_specifier ):

    return {
        'types' : sorted( service_specifier.GetServiceTypes() ),
        'keys' : sorted( key.hex() for key in service_specifier.GetServiceKeys() ),
    }


def norm_predicate( predicate ):

    t = predicate.GetType()
    v = predicate.GetValue()

    if t in ( ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_WIDTH, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_HEIGHT, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_NUM_NOTES, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_NUM_WORDS, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_NUM_URLS, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_NUM_FRAMES, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_DURATION, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_FRAMERATE ):

        value = norm_number_test( v )

    elif t == ClientSearchPredicate.PREDICATE_TYPE_OR_CONTAINER:

        value = sorted( ( norm_predicate( p ) for p in v ), key = lambda d: json.dumps( d, sort_keys = True ) )

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_MIME:

        value = sorted( v )

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_HASH:

        ( hashes, hash_type ) = v

        value = { 'hashes' : sorted( h.hex() for h in hashes ), 'hash_type' : hash_type }

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_SIMILAR_TO_FILES:

        ( hashes, distance ) = v

        value = { 'hashes' : sorted( h.hex() for h in hashes ), 'distance' : distance }

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_SIMILAR_TO_DATA:

        ( pixel_hashes, perceptual_hashes, distance ) = v

        value = { 'pixel_hashes' : sorted( h.hex() for h in pixel_hashes ), 'perceptual_hashes' : sorted( h.hex() for h in perceptual_hashes ), 'distance' : distance }

    elif t in ( ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_IMPORT_TIME, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_MODIFIED_TIME, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_LAST_VIEWED_TIME, ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_ARCHIVED_TIME ):

        ( operator, age_type, age_value ) = v

        value = { 'op' : operator, 'kind' : age_type, 'value' : list( age_value ) }

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_FILE_SERVICE:

        ( is_in, status, service_key ) = v

        value = { 'is_in' : is_in, 'status' : status, 'service' : service_key.hex() }

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_RATING:

        ( operator, rating, service_key ) = v

        value = { 'op' : operator, 'value' : rating, 'service' : service_key.hex() }

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_RATING_ADVANCED:

        ( logical_operator, primary, secondary, rated ) = v

        value = { 'logical_operator' : logical_operator, 'primary' : norm_service_specifier( primary ), 'secondary' : norm_service_specifier( secondary ), 'rated' : rated }

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_KNOWN_URLS:

        # the fourth member is a display string derived from the others
        ( include, rule_type, rule, description ) = v

        if rule_type == 'url_class':

            rule = rule.GetName()


        value = { 'include' : include, 'rule_type' : rule_type, 'rule' : rule }

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_TAG_ADVANCED:

        ( service_key, tag_display_type, statuses, tag ) = v

        value = { 'service' : None if service_key is None else service_key.hex(), 'display' : TAG_DISPLAY_NAMES[ tag_display_type ], 'statuses' : sorted( statuses ), 'tag' : tag }

    elif t == ClientSearchPredicate.PREDICATE_TYPE_SYSTEM_FILE_VIEWING_STATS:

        ( view_type, locations, operator, viewing_value ) = v

        value = { 'view_type' : view_type, 'canvases' : list( locations ), 'op' : operator, 'value' : viewing_value }

    elif isinstance( v, tuple ):

        value = list( v )

    else:

        value = v


    return { 'type' : PREDICATE_TYPE_NAMES[ t ], 'value' : value, 'inclusive' : predicate.IsInclusive() }


class HarnessError( Exception ):

    pass


def check_not_harness_bug( e ):

    # a gap in the stubbed controller would otherwise be recorded as a parse failure
    while e is not None:

        if isinstance( e, ( AttributeError, NameError ) ):

            raise HarnessError( repr( e ) )


        e = e.__context__



RESOLUTION_ERROR_MARKERS = ( 'did not find a service called', 'could not find the service', 'did not find url class called' )


def parse_one( text ):

    # mirrors ClientSearchParseSystemPredicates.ParseSystemPredicateStringsToPredicates, but keeps the
    # failing stage so the fixture can say whether the text was rejected or a name failed to resolve
    try:

        ( ext_pred_type, operator, value, unit ) = SystemPredicateParser.parse_system_predicate( text )

    except Exception as e:

        check_not_harness_bug( e )

        stage = 'resolve' if any( m in str( e ).lower() for m in RESOLUTION_ERROR_MARKERS ) else 'parse'

        return { 'ok' : False, 'stage' : stage, 'error' : str( e ) }


    try:

        predicate = ClientSearchParseSystemPredicates.pred_generators[ ext_pred_type ]( operator, value, unit )

    except Exception as e:

        check_not_harness_bug( e )

        stage = 'resolve' if any( m in str( e ).lower() for m in RESOLUTION_ERROR_MARKERS ) else 'parse'

        return { 'ok' : False, 'stage' : stage, 'error' : str( e ) }


    # the stored form, as hydrus keeps it in rules and favourite searches
    # (not for URL class predicates: the URL classes here are stubs)
    result = { 'ok' : True, 'predicate' : norm_predicate( predicate ) }

    if not isinstance( getattr( predicate, '_value', None ), tuple ) or 'url_class' not in predicate.GetValue():

        result[ 'serialised' ] = json.loads( json.dumps( predicate.GetSerialisableTuple() ) )


    return result


def parse_with_clock( text ):

    FROZEN_DATEPARSER.now = BASE_NOW

    result = parse_one( text )

    FROZEN_DATEPARSER.now = ALT_NOW

    alternate = parse_one( text )

    FROZEN_DATEPARSER.now = BASE_NOW

    result[ 'depends_on_now' ] = json.dumps( result, sort_keys = True ) != json.dumps( alternate, sort_keys = True )

    return result


def parse_api_tags( tags ):

    try:

        predicates = ClientLocalServerCore.ConvertTagListToPredicates( None, tags, do_permission_check = False )

    except Exception as e:

        check_not_harness_bug( e )

        stage = 'resolve' if any( m in str( e ).lower() for m in RESOLUTION_ERROR_MARKERS ) else 'parse'

        return { 'ok' : False, 'stage' : stage, 'error' : str( e ) }


    normalised = sorted( ( norm_predicate( p ) for p in predicates ), key = lambda d: json.dumps( d, sort_keys = True ) )

    return { 'ok' : True, 'predicates' : normalised }


# ---------------------------------------------------------------------------
# Corpus

def reference_test_examples():

    # every input string from TestClientSearch.test_system_predicate_parsing, read from the source
    # so this stays in sync with the reference tests
    path = os.path.join( REPO_ROOT, 'hydrus', 'test', 'TestClientSearch.py' )

    with open( path, encoding = 'utf-8' ) as f:

        tree = ast.parse( f.read() )


    examples = []

    for node in ast.walk( tree ):

        if isinstance( node, ast.FunctionDef ) and node.name == 'test_system_predicate_parsing':

            for sub in ast.walk( node ):

                if isinstance( sub, ast.For ) and isinstance( sub.iter, ast.List ):

                    for elt in sub.iter.elts:

                        input_node = elt.elts[1]

                        examples.append( eval( compile( ast.Expression( input_node ), path, 'eval' ), { 'HC' : HC } ) )






    assert len( examples ) > 200, 'did not find the reference examples'

    return examples


def api_doc_examples():

    # the "System Predicates" example list in the Client API docs
    path = os.path.join( REPO_ROOT, 'docs', 'developer_api.md' )

    with open( path, encoding = 'utf-8' ) as f:

        lines = f.read().splitlines()


    examples = []

    for line in lines:

        m = re.match( r'^\s+\*\s+(system:.*)$', line )

        if m is None:

            continue


        text = m.group( 1 ).rstrip()

        text = re.sub( r'\s*_\(.*\)_$', '', text )

        service_name = 'example local rating like service'

        if text.endswith( '(numerical services)' ):

            service_name = 'example local rating numerical service'

        elif text.endswith( '(inc/dec services)' ):

            service_name = 'example local rating inc/dec service'


        text = re.sub( r'\s*\((numerical|like/dislike|inc/dec) services\)$', '', text )

        text = text.replace( '`service_name`', service_name )

        examples.append( text )


    assert len( examples ) > 80, 'did not find the API doc examples'

    return examples


H64 = [
    'e2c1592ce2a3338767bb7990738ae06357cbdfe917669da9a0d04069f9759c08',
    'cf09faad262075f96bf9a30052b8ec224e096948a4f3a2776df5fa5a777bcfd8',
    'A1B0AB771D11D9A6D1F993EFEE9D253D3AA78914387A7C8CEAB520AF88AB3DE2',
]
H32 = [ 'ada7a31713ba24652c52e52c6f212e47', '546fd4b8c39fc53e77e2f28b59cd1b18' ]
H40 = [ '2496baf1ded134b5ff7e44f72155240b9561ab5a' ]
H128 = [ 'ab' * 64 ]
H16 = [ '0702790ffeae5c8a', '51ad07c228ab7469' ]

OPERATOR_SPELLINGS = [
    '=', '==', 'is', '≠', '!=', 'is not', 'isn\'t', '<', 'fewer than', 'less than', '>', 'more than', 'greater than',
    '≤', '<=', 'fewer than or equal to', 'less than or equal to', '≥', '>=', 'more than or equal to',
    'greater than or equal to', '≈', '~=', 'about', 'is about',
    # not operators
    '=<', '=>', '~', '!', 'equals', 'isnt', '<>', '',
]


def systematic_examples():

    out = []

    def add( *texts ):

        out.extend( texts )


    # --- simple flags, and how names are matched

    flag_names = [
        'everything', 'inbox', 'archive', 'archived', 'has duration', 'no duration', 'has framerate', 'no framerate',
        'has frames', 'no frames', 'has width', 'no width', 'has height', 'no height', 'has notes', 'no notes',
        'has urls', 'no urls', 'has words', 'no words', 'has audio', 'no audio', 'has transparency', 'no transparency',
        'has alpha', 'no alpha', 'has exif', 'no exif', 'has xmp', 'no xmp', 'has iptc', 'no iptc',
        'has embedded metadata', 'no embedded metadata', 'has human-readable metadata', 'no human-readable metadata',
        'has software/source metadata', 'no software/source metadata', 'has icc profile', 'no icc profile',
        'has forced filetype', 'no forced filetype', 'has tags', 'no tags', 'untagged',
        'best quality of group', 'is the best quality file of its duplicate group', 'is best quality of group',
        'not best quality of group', 'isn\'t the best quality file of its group', 'is not the best quality file of its duplicate group',
        'has note', 'no note', 'does not have notes', 'doesn\'t have notes', 'does not have a note', 'has no notes',
    ]

    for name in flag_names:

        add( 'system:' + name )
        add( 'system:' + name.replace( ' ', '_' ) )
        add( 'SYSTEM:' + name.upper() )
        add( '  system:' + name.replace( ' ', '  ' ) + '  ' )
        add( 'system:' + name + ':' )
        add( 'system:' + name + ' x' )
        add( 'system: ' + name )
        add( '-system:' + name )


    add( 'system:', 'system', 'inbox', 'system:foo', 'system:everything else', 'system:inboxes', 'system:archive date', 'system:archived x',
         'system:has something', 'system:no', 'system:has', 'system:no_notes_', 'system:has_ notes', 'system:\tinbox', 'system:inbox\t',
         'system: inbox', ' system:inbox ', 'system:inbox\x1f', '\x1csystem:archive', 'system:has\x1fnotes',
         'system:has　audio', 'system:everything\n', 'system:everything\ninbox', 'System:Everything', 'system::inbox', 'system:: everything',
         'system:embedded metadata', 'system:has lots of embedded metadata', 'system:no metadata', 'system:has foo embedded bar metadata',
         'system:has human-readable embedded metadata', 'system:no human-readable foo metadata',
         '', '   ', 'system:has url', 'system:no url', 'system:has urls x', 'system:no audio please' )

    # --- relational operators on every numeric predicate

    numeric_names = [
        'width', 'height', 'num words', 'number of words', 'num frames', 'number of frames', 'framerate', 'num urls', 'number of urls',
        'num notes', 'number of notes', 'num tags', 'number of tags', 'num of tags', 'limit', 'filesize', 'file size', 'num pixels',
        'number of pixels', 'media views', 'preview views', 'all views', 'views', 'views in media', 'views in client api, preview',
        'number of character tags', 'num unnamespaced tags', 'number of * tags', 'num file relationships', 'tag as number page',
        'tag as number any namespace', 'tag as number unnamespaced', 'duration', 'media viewtime', 'viewtime in preview',
    ]

    units_for_name = {
        'filesize' : ' kb', 'file size' : ' mb', 'num pixels' : ' px', 'number of pixels' : ' megapixels',
        'num file relationships' : ' alternates', 'duration' : ' seconds', 'media viewtime' : ' hours', 'viewtime in preview' : ' days',
    }

    for name in numeric_names:

        unit = units_for_name.get( name, '' )

        for op in OPERATOR_SPELLINGS:

            add( 'system:{} {} 5{}'.format( name, op, unit ) )


        for value in [ '0', '1,000', '-5', '5.5', 'abc', '', '007', '1e3', '99999999999999999999', ' 5 5', '5,', ',', '-,', '5x' ]:

            add( 'system:{} > {}{}'.format( name, value, unit ) )
            add( 'system:{} = {}{}'.format( name, value, unit ) )


        add( 'system:{}:>5{}'.format( name, unit ) )
        add( 'system:{}>5{}'.format( name, unit ) )
        add( 'system:{}: has {}'.format( name, name ) )
        add( 'system:{}: no {}'.format( name, name ) )
        add( 'system:{} has'.format( name ) )
        add( 'system:{} nothing'.format( name ) )
        add( 'system:{} > has'.format( name ) )
        add( 'system:{}'.format( name ) )
        add( 'system:{} is 5{}'.format( name.replace( ' ', '_' ), unit ) )
        add( 'SYSTEM:{} IS 5{}'.format( name.upper(), unit.upper() ) )


    # --- units

    for unit in [ 'b', 'B', 'byte', 'bytes', 'kb', 'KB', 'kilobyte', 'kilobytes', 'mb', 'megabyte', 'megabytes', 'gb', 'gigabyte', 'gigabytes', 'tb', 'kib', '', 'kbx', 'k' ]:

        add( 'system:filesize < 5 {}'.format( unit ), 'system:filesize < 5{}'.format( unit ) )


    for unit in [ 'px', 'pixel', 'pixels', 'kpx', 'kilopixel', 'kilopixels', 'mpx', 'megapixel', 'megapixels', 'gpx', '', 'pixelsx' ]:

        add( 'system:num pixels ~= 5 {}'.format( unit ), 'system:number of pixels > 5{}'.format( unit ) )


    for unit in [ '', 'px', 'pixel', 'pixels', 'pixelsx', 'kpx', 'mm' ]:

        add( 'system:width = 5 {}'.format( unit ), 'system:height < 5{}'.format( unit ) )


    for unit in [ '', 'fps', 'FPS', 'hz', 'fpsx' ]:

        add( 'system:framerate = 30 {}'.format( unit ), 'system:framerate ~= 30{}'.format( unit ) )


    for unit in [ 'duplicates', 'alternates', 'not related/false positive', 'not related/false positives', 'not related', 'false positive',
                  'false positives', 'potential duplicates', 'potentials', '', 'duplicate', 'alternates x' ]:

        add( 'system:num file relationships < 3 {}'.format( unit ), 'system:number of file relationships = 3{}'.format( unit ) )


    # --- durations and view times

    for value in [ '5 seconds', '5 second', '5 secs', '5 sec', '5s', '5 s', '5 milliseconds', '5 millisecond', '5 msecs', '5 msec', '5ms',
                   '5 minutes', '5 minute', '5 mins', '5m', '5 m', '5m30s', '5m 30s', '1 hour', '2 hours', '1hr', '1h', '1h30m', '1 h 30 m 10 s 5 ms',
                   '300 seconds', '5 sec 6000 msecs', '0 seconds', '05 seconds', '1.5 seconds', 'seconds', '5', '5 x', '5 days', '1h 5', '1 hour 1',
                   '5 mins 5 secs', '2m5', 'has duration', 'no duration', '' ]:

        add( 'system:duration < {}'.format( value ), 'system:duration ~= {}'.format( value ) )


    for value in [ '1 day', '1 days 1 hour 0 minutes', '1 hours 100 seconds', '1 day 30 hours 100 minutes 90s', '5 minutes', '5 mins', '5 min',
                   '5 seconds', '5 secs', '5 sec', '5 s', '5s', '100 hours', '1h', '1 h', '0 days', '5', '5 weeks', '1.5 days', '',
                   '10 days 25 hours 61 minutes 61 seconds', '1 day x', '05 days' ]:

        add( 'system:media viewtime < {}'.format( value ), 'system:viewtime in client api > {}'.format( value ) )


    for canvases in [ 'media', 'preview', 'client api', 'media, preview', 'media, client api, preview', 'client api,media', '', 'media media', 'mediapreview' ]:

        add( 'system:views in {} > 5'.format( canvases ), 'system:viewtime in {} < 1 day'.format( canvases ) )
        add( 'system:views {} = 3'.format( canvases ) )


    add( 'system:views > 5 media', 'system:views in media ≠ 5', 'system:viewtime > 1 hour', 'system:views_in_media > 3', 'system:views in', 'system:views',
         'system:all views ~= 10', 'system:preview viewtime ≈ 1 hour', 'system:all viewtime < 1 hour', 'system:view in media > 3' )

    # --- times

    time_names = [
        'import time', 'imported time', 'import date', 'imported date', 'time imported', 'date imported', 'imported', 'import_time',
        'modified time', 'modified date', 'time modified', 'date modified', 'modified',
        'last viewed time', 'last view time', 'last viewed date', 'last view date', 'time last viewed', 'date last viewed', 'last viewed',
        'archived time', 'archive time', 'archived date', 'archive date', 'time archived', 'date archived', 'archived',
    ]

    for name in time_names:

        for rest in [ '< 7 days', '> 2011-06-04', '~= 1 month', ': since 3 days ago' ]:

            add( 'system:{} {}'.format( name, rest ) )



    time_values = [
        '60 days', '60 days 4 hours', '1 day', '7 days ago', '3 days ago', '1 month', '1 year', '2 years', '7 years 45 days 7h',
        '7 years 2 months', '0 years 1 month 1 day 1 hour', '1 month ago', '13 months', '2 weeks', '2 weeks ago', '1 week 2 days',
        '1 week', '3 hours', '3 hours ago', '1 hour', '5 hours', '30 hours', '23 hours 30 minutes', '30 minutes', '30 minutes ago',
        '45 minutes ago', '90 minutes', '1 day 1 hour 30 minutes', '10 seconds', '10 seconds ago', '1 second ago', '1d', '1h', '7h',
        '2 d', '2days', '2 day', '1 yr', '2 mo', 'yesterday', 'today', 'now', 'tomorrow', 'last week', 'in 2 days', 'a day', 'an hour',
        'one day', 'two days', '1.5 days', '2 days and 3 hours', '4 hours 60 days', '1 decade', '100 years', 'day', 'days', 'ago',
        '2011-06-04', '2011-1-3', '1996-05-2', '2011-06-04 13:45', '2011-06-04 13:45:30', '2011/06/04', '2011.06.04', '2020-02-29',
        '2021-02-29', '2011-13-45', '2011-02-30', '0001-01-01', '9999-12-31', '03/04/2020', '02-03-2011', '4 march 2020',
        'march 2020', 'june 4 2011', '2020', '20110604', 'x', '', '2011-06-04x', '2011-06-04 ago', '2011-6-4',
        '60 days x', '1 day, 2 hours', '1 day and 2 hours', '5 days ago ago',
        # unit spellings, repeated units, spacing and case
        '3 yrs', '3 mos', '3 wks', '3 w', '3w', '1y', '2mo', '3wk', '4hr', '5hrs', '6min', '7mins', '8sec', '9secs', '1 day 3 d',
        '3 days, 1 day', '1 year 1 year', '7 DAYS', '7\tdays', '7  days', '1 day 25 hours', '0 days', '0 hours', '1,000 days',
        'a week', '1 hour ago 1 day', 'ago 1 day', '1000 years', '999999999999 days',
        # date spellings
        '2011-06-04T13:45', '2011-6-4 1:05', '2011-06-04 25:00', '2011/6/4', '2011-06/04', '2011-06-04 13:45:99',
    ]

    time_ops = [ '<', '>', '=', '~=', '≈', '!=', '≠', 'is', '==', 'since', 'before', 'around', 'the day of', 'the month of', 'on the day of',
                 'a month either side of', 'since the day of', '', 'after', 'isn\'t', 'is not', '<=', '>=' ]

    for value in time_values:

        add( 'system:import time < {}'.format( value ), 'system:import time > {}'.format( value ), 'system:import time ~= {}'.format( value ),
             'system:import time = {}'.format( value ) )


    for op in time_ops:

        for value in [ '7 days', '7 days ago', '2011-06-04', '2 weeks', '30 minutes', 'yesterday', '1 month' ]:

            add( 'system:modified time {} {}'.format( op, value ) )



    add( 'system:import time: the month of 2020-01-03', 'system:import time: the day of 2020-01-03', 'system:date imported around 7 days ago',
         'system:archived time < 7 days', 'system:last viewed time ≈ 2 months', 'system:modified time since 2011-06-04',
         'system:import time since the month of 2 days', 'system:import time month 2020-01-01', 'system:import time day 5 days',
         'system:import time before yesterday', 'system:import time since yesterday', 'system:import time:< 7 days' )

    # --- hashes and similar files

    for sep in [ ' ', ', ', ',', '  ,  ', '\n' ]:

        add( 'system:hash = ' + sep.join( H64[ :2 ] ) )
        add( 'system:similar to ' + sep.join( H64[ :2 ] ) )


    for algo_text in [ '', ' md5', ' sha1', ' sha512', ' sha256', ' MD5' ]:

        for hashes in [ H64, H32, H40, H128 ]:

            add( 'system:hash = {}{}'.format( ' '.join( hashes ), algo_text ) )



    for prefix in [ 'hash (md5)', 'hash md5', 'hash (sha1)', 'hash sha512', 'hash (sha512)', 'hash (sha256)', 'hash', 'Hash' ]:

        for op in [ '=', '==', 'is', 'is not', '!=', '≠', 'isn\'t', '<', '' ]:

            add( 'system:{} {} {}'.format( prefix, op, H32[ 0 ] ) )



    add( 'system:hash = abcdef01 abcdef02 abcdef03', 'system:hash = abcdef012', 'system:hash = abcdef0123', 'system:hash = xyz',
         'system:hash = ', 'system:hash = garbage ' + H64[ 0 ] + ' more garbage', 'system:hash = ' + H64[ 0 ] + 'z', 'system:hash = ' + H64[ 0 ][ :-1 ],
         'system:hash = ' + H64[ 0 ] + H64[ 1 ], 'system:hash = ' + H32[ 0 ] + ' ' + H64[ 0 ] + ' md5', 'system:hash = ' + H64[ 0 ] + ' ' + H32[ 0 ],
         'system:hash = ' + H64[ 0 ] + ' sha1 md5', 'system:hash md5 = ' + H64[ 0 ], 'system:hash = ' + H40[ 0 ] + ' sha1',
         'system:hash=' + H64[ 0 ], 'system:hash:' + H64[ 0 ], 'system:hash ' + H64[ 0 ] )

    for distance_text in [ '', ' 3', ' distance 3', ' with distance 3', ' with distance of 3', ' distance of 3', ' with 3', ' of 3', ' distance 0',
                           ' distance 03', ' distance', ' distance x', ' distance -1', ' with  distance   of   12', ',distance 3', ' distance 99999999999' ]:

        add( 'system:similar to {}{}'.format( H64[ 0 ], distance_text ) )
        add( 'system:similar to files {}{}'.format( H64[ 0 ], distance_text ) )
        add( 'system:similar to data {} {}{}'.format( H64[ 1 ], H16[ 0 ], distance_text ) )


    add( 'system:similar to abcdef distance 5', 'system:similar to abcd', 'system:similar to abcde', 'system:similar to ' + H32[ 0 ],
         'system:similar to ' + H64[ 0 ][ :-1 ], 'system:similar to data ' + H32[ 0 ], 'system:similar to data ' + ' '.join( H16 ),
         'system:similar to data ' + ' '.join( H16 + H64[ :2 ] ) + ' distance 4', 'system:similar to data', 'system:similar to', 'system:similar to data abcde',
         'system:similar to data ' + H64[ 0 ][ :-1 ], 'system:similar_to ' + H64[ 0 ], 'system:similar to data ' + H16[ 0 ] + ' ' + H16[ 0 ],
         'system:similar to ' + H64[ 0 ] + ' ' + H64[ 0 ], 'system:similar to data files ' + H16[ 0 ], 'system:similar to files data ' + H64[ 0 ] )

    # --- file types

    for word in SystemPredicateParser.FILETYPES.keys():

        add( 'system:filetype = ' + word )


    add( 'system:filetype = image/jpg, image/png, apng', 'system:filetype is not jpeg', 'system:filetype = jpeg png',
         'system:filetype = jpeg, png', 'system:filetype = jpeg,png', 'system:filetype = jpeg , png', 'system:filetype = jpeg, ',
         'system:filetype = image, video', 'system:filetype = gif', 'system:filetype = static gif, animated gif',
         'system:filetype = image/svg+xml', 'system:filetype = image/svgxml', 'system:filetype = application/vnd.rar',
         'system:filetype = application/vndxrar', 'system:filetype = nonsense', 'system:filetype = ', 'system:filetype = jpegx',
         'system:filetype == jpeg', 'system:filetype != jpeg', 'system:filetype ≠ jpeg', 'system:filetype isn\'t jpeg', 'system:filetype < jpeg',
         'system:file type = jpeg', 'system:file_type = jpeg', 'system:filetype: jpeg', 'system:filetype jpeg', 'system:filetype = JPEG',
         'system:filetype = all files', 'system:filetype = image, animation, video, audio, application, archive, image project file',
         'system:filetype = jpeg, png, webp, bitmap, icon, tiff, qoi, heif, heic, avif, jxl, static gif',
         'system:filetype = jpeg, jpeg', 'system:filetype = image, jpeg', 'system:filetype = mp4,  webm , mkv',
         'system:filetype = application/x-shockwave-flash', 'system:filetype = flash', 'system:filetype = png or apng' )

    # --- file services

    for op in [ 'is currently in', 'currently in', 'is not currently in', 'not currently in', 'isn\'t currently in', 'is pending to', 'pending to',
                'is not pending to', 'not pending to', 'isn\'t pending to', 'is deleted from', 'is petitioned from', 'is in', '' ]:

        for service in [ 'my files', 'MY FILES', 'trash', 'archive box', 'Archive Box', 'example file repo', 'all known files', 'my tags', 'nonexistent', '' ]:

            add( 'system:file service {} {}'.format( op, service ) )



    add( 'system:file service:is currently in my files', 'system:file_service is currently in my files', 'system:file service  is  currently  in  my files' )

    # --- ratio

    for op in [ '=', '==', 'is', 'wider than', 'is wider than', 'taller than', 'is taller than', '~=', '≈', '!=', '≠', '<', '>', '' ]:

        for value in [ '16:9', '1:1', '0:0', '16 : 9', '16:9x', '16/9', '16', '4:3:2', '01:1' ]:

            add( 'system:ratio {} {}'.format( op, value ) )



    for special in [ 'is square', 'square', 'is portrait', 'portrait', 'is landscape', 'landscape', 'is squarish', 'is round', '', 'blahsquareblah', 'portrait square' ]:

        add( 'system:ratio {}'.format( special ) )


    # --- urls

    for prefix in [ 'has url matching regex', 'has a url matching regex', 'does not have url matching regex', 'does not have a url matching regex',
                    'doesn\'t have url matching regex', 'doesn\'t have a url matching regex' ]:

        for value in [ 'index\\.php', 'Index\\.PHP', '^https?://', '', '[a-z]+', 'CASE' ]:

            add( 'system:{} {}'.format( prefix, value ) )



    for prefix in [ 'has url', 'has url:', 'has_url', 'does not have url', 'doesn\'t have url', 'doesn\'t have url:' ]:

        for value in [ 'https://somebooru.com/posts/123456', 'HTTPS://SomeBooru.com/Posts/ABC', 'http://x', 'ftp://example.com', 'www.example.com', '' ]:

            add( 'system:{} {}'.format( prefix, value ) )



    for prefix in [ 'has domain', 'has a domain', 'has an domain', 'has url with domain', 'has a url with domain', 'does not have domain',
                    'doesn\'t have domain', 'does not have a url with domain', 'doesn\'t have an url with domain' ]:

        for value in [ 'somebooru.com', 'SomeBooru.COM', '', 'x' ]:

            add( 'system:{} {}'.format( prefix, value ) )



    for prefix in [ 'has url with class', 'has a url with class', 'has url with url class', 'has an url with url class', 'does not have url with class',
                    'doesn\'t have a url with url class', 'does not have a url with class' ]:

        for value in [ 'somebooru file page', 'SomeBooru File Page', 'some imageboard thread', 'nonexistent class', '' ]:

            add( 'system:{} {}'.format( prefix, value ) )



    # --- tag as number

    for namespace in [ 'page', 'page_underscore', 'any namespace', 'unnamespaced', '', 'volume number', 'series:sub' ]:

        for op in [ '<', '>', '=', '~=', '≈', '!=', 'is', 'is not', 'less than', 'more than', 'about', '≤', '>=', 'equals' ]:

            add( 'system:tag as number {} {} 5'.format( namespace, op ) )



    add( 'system:tag as number: page less than 5', 'system:tag as number page < -5', 'system:tag as number page < 1,000', 'system:tag as number page <',
         'system:tag as number page < x', 'system:tag as number page', 'system:tag as number < 5', 'system:tag as number page  <  5',
         'system:tag as number page is about 5', 'system:tag as number page is page < 5', 'system:tag as number page < 5 x',
         'system:tag as number page has', 'system:tag as number page < has', 'system:tag as number page < no', 'system:tag as number page < -',
         'system:tag as number page < 5.5', 'system:tag as number page\t< 5' )

    # --- notes

    for prefix in [ 'has note with name', 'has a note with name', 'note with name', 'has note named', 'note named', 'no note with name',
                    'has no note with name', 'does not have note with name', 'does not have a note with name', 'doesn\'t have note with name',
                    'doesn\'t have a note with name', 'no note named' ]:

        for name in [ 'test', '"test"', '\'test\'', 'Comment', '"', '""', '"a"', 'two words', '"quoted" thing', '' ]:

            add( 'system:{} {}'.format( prefix, name ) )



    add( 'system:num notes is 5', 'system:num notes > 1', 'system:number of notes < 3', 'system:num notes ~= 3', 'system:num notes != 3',
         'system:num notes is not 3', 'system:num notes <= 3', 'system:num note = 3', 'system:number of note > 0', 'system:has notes: yes' )

    # --- ratings

    rating_services = [ 'example local rating like service', 'favourites', 'example local rating numerical service', 'ten stars',
                        'example local rating inc/dec service', 'counter', 'thumbs', 'nonexistent service', 'my files', 'Ten Stars', 'COUNTER' ]

    # the full operator x value matrix on one service of each kind, plus the odd ones out
    rating_matrix_services = [ 'example local rating like service', 'example local rating numerical service', 'ten stars', 'counter',
                               'thumbs', 'nonexistent service', 'Ten Stars' ]

    for service in rating_services:

        for prefix in [ 'has rating', 'has a rating', 'has rating for', 'has a rating for', 'has count for', 'no rating', 'no rating for',
                        'has no rating for', 'does not have a rating for', 'doesn\'t have a rating for', 'does not have rating', 'no count for' ]:

            add( 'system:{} {}'.format( prefix, service ) )


        if service in rating_matrix_services:

            for op in [ '=', '==', 'is', '<', '>', '~=', '≈', 'about', 'is about', 'less than', 'more than', '≤', '>=', '!=', '≠', 'is not', '' ]:

                for value in [ '3/5', '0/5', '5/5', '6/5', '10/10', '3/10', '1', '123', '0', 'like', 'dislike', 'rated', '3/', '/5', '-1' ]:

                    add( 'system:rating for {} {} {}'.format( service, op, value ) )



        else:

            for value in [ '3/5', '123', 'like' ]:

                add( 'system:rating for {} = {}'.format( service, value ) )



        add( 'system:rating {} = 3/5'.format( service ), 'system:count for {} > 3'.format( service ), 'system:rating for {}: like'.format( service ),
             'system:rating for {} like'.format( service ), 'system:rating for {}=3/5'.format( service ), 'system:rating_for_{} = 3/5'.format( service ) )


    # non-ASCII digits, which Python's \\d and int() accept
    add( 'system:width > \u0663', 'system:limit = \uff15', 'system:rating for counter = \u0661\u0662', 'system:rating for ten stars = \u0663/\u0665',
         'system:number of character tags > \u0663', 'system:import time < \u0663 days', 'system:ratio = \u0661\u0666:\u0669' )

    # from the reference's Client API tests
    add( 'system:width<400', 'system:untagged', 'system:wew', 'system:bad_system_pred' )

    add( 'system:rating for this 3/5', 'system:rating for this = 3/5', 'system:rating for x 123', 'system:rating for unlike', 'system:rating for favourites unlike',
         'system:rating for 3/5', 'system:rating for = 3/5', 'system:rating for  counter  >  5', 'system:rating', 'system:rating for', 'system:has rating',
         'system:has rating for', 'system:no rating', 'system:rating for favourites = likes', 'system:rating for favourites about like',
         'system:rating for ten stars = 3/5/5', 'system:rating for ten stars = 3 / 5', 'system:rating for counter = 1,000' )

    # --- advanced tag

    for prefix in [ 'has tag', 'does not have tag' ]:

        for gubbins in [ '', ' in "my tags"', ' "my tags"', ' in "all known tags"', ' in "MY TAGS"', ' in "nonexistent"', ' ignoring siblings/parents',
                         ' ignoring siblings', ' ignoring parents', ', ignoring siblings/parents', ' status current, pending', ' with status deleted',
                         ' with status in current, pending, deleted, petitioned', ' in "my tags", ignoring siblings/parents, status current, pending',
                         ' "all known tags", status deleted', ' "all known tags", deleted, current, pending, petitioned', ' in "downloader tags", with status petitioned',
                         ' in my tags', ' with status nothing', ' in "my tags", in "downloader tags"', ' in "example tag repo", status pending' ]:

            for tag in [ '"skirt"', '"filename:blarg"', '"Skirt With Spaces"', 'skirt', '""', '"has:colon"', '" "', '"system:inbox"', '"-skirt"',
                         '"quote"inside"', '"*"', '"series:*"' ]:

                add( 'system:{}{}: {}'.format( prefix, gubbins, tag ) )




    add( 'system:has tag "skirt"', 'system:has tag skirt', 'system:has tag', 'system:has tag:', 'system:has tag ,: "skirt"', 'system:has tag "a", "b"',
         'system:has tag a, b', 'system:has tag in "my tags"', 'system:has_tag: "skirt"', 'system:has tag : "skirt"', 'system:has tag:"skirt"',
         'system:has tag in "my tags" "skirt"', 'system:has tag in "my tags": skirt: "blue"' )

    # --- advanced ratings

    for logic in [ 'all', 'any', 'only', 'ALL', 'none' ]:

        for spec in [ 'ratings', 'rating', 'inc/dec ratings', 'numerical ratings', 'like/dislike ratings', 'inc/dec ratings, numerical ratings',
                      'example local rating numerical service', 'example local rating numerical service, example local rating like service',
                      'favourites', 'ten stars', 'nonexistent service', 'tag repository', 'numerical ratings, favourites', 'trash', '', 'my files' ]:

            for rated in [ 'rated', 'not rated' ]:

                add( 'system:{} {} {}'.format( logic, spec, rated ) )




    for amongst in [ '(amongst inc/dec ratings, numerical ratings)', '(amongst favourites)', '(amongst favourites, counter)', '(amongst nonexistent)',
                     '(amongst ratings)', '(amongst numerical ratings', '(amongst)', 'amongst favourites' ]:

        add( 'system:only counter {} rated'.format( amongst ), 'system:only inc/dec ratings {} not rated'.format( amongst ) )


    add( 'system:all ratings rated x', 'system:any rated', 'system:all  ratings  rated', 'system:all ratings unrated', 'system:allratings rated',
         'system:only rated', 'system:all ratings', 'system:all_ratings_rated', 'system:any favourites,counter rated', 'system:any favourites , counter rated' )

    # --- number of tags with namespaces

    for namespace in [ 'character', 'unnamespaced', '*', 'creator:studio', 'two words', 'with tags word', 'char*' ]:

        for op in [ '>', '<', '=', '~=', '≈', '!=', '≠', '>=', '<=', 'is', 'is not', 'less than', 'about', '' ]:

            add( 'system:number of {} tags {} 5'.format( namespace, op ) )



    add( 'system:num character tags > 5', 'system:number of character tags>5', 'system:number of character tags > 5 ', 'system:number of character tags > x',
         'system:number of character tags > 5x', 'system:number of character tags', 'system:number of  character tags > 5', 'system:num of character tags = 0',
         'system:number of character tags tags > 5', 'system:number of character_tags > 5', 'system:number_of_character_tags > 5' )

    return out


API_SEARCHES = [
    [ 'blue eyes', 'blonde hair', 'кино', 'system:inbox', 'system:limit=16' ],
    [ 'skirt', [ 'space bounty hunter', 'jane raider' ], 'system:height > 1000' ],
    [ 'character:sam*', 'series:*' ],
    [ '-green eyes', 'blue eyes' ],
    [ '-green eyes' ],
    [ '--double negative' ],
    [ '-' ],
    [ '' ],
    [ '   ' ],
    [ 'series:' ],
    [ ':' ],
    [ '*' ],
    [ '**' ],
    [ '*:*' ],
    [ '-*' ],
    [ '-series:*' ],
    [ '*sam*', 'character:*sam', 'Character:SAM*' ],
    [ 'Blue Eyes', 'blue eyes', 'BLUE EYES' ],
    [ 'blue  eyes ', ' blue eyes' ],
    [ ':)', '::)', ':weird:stuff' ],
    [ 'system:everything', 'System:inbox', ' system:inbox', 'system: inbox' ],
    [ 'system:not a predicate' ],
    [ 'system:inbox', 'system:inbox' ],
    [ '-system:inbox' ],
    [ 'system:rating for favourites = like', 'system:file service is currently in my files' ],
    [ 'system:rating for nonexistent = like' ],
    [ [ 'a', 'b' ], [ 'c', [ 'd', 'e' ] ] ],
    [ [] ],
    [ [ 'a' ] ],
    [ [ 'a', '-b', 'system:inbox', 'x:*', 'y*' ] ],
    [ [ 'a', '' ] ],
    [ [ 'a', 'system:bad' ] ],
    [ 'a', 5, None, True, { 'x' : 1 } ],
    [ 5 ],
    [],
    [ 'tag with ​ zero width', 'system:inbox​' ],
    [ 'creator:青い桜' ],
    [ '-creator:*', '-creator:sam*' ],
    [ 'a' * 1100 ],
    [ 'system:hash = ' + H64[ 0 ], 'system:similar to ' + H64[ 1 ] + ' distance 2' ],
    [ 'system:import time < 7 days', 'system:filetype = image, video' ],
    [ 'ㅤ', '‌' ],
    [ 'Tag', '-tag' ],
    [ 'system:inbox', [ 'system:archive', 'system:has audio' ] ],
    [ [ '-a', '-b' ] ],
    # from the reference's Client API tests
    [ 'kino' ],
    [ 'kino', 'green' ],
    [ '-green' ],
    [ 'green*' ],
    [ '*r:green' ],
    [ 'green', '-kino' ],
    [ 'green', 'system:archive' ],
    [ 'green', [ 'red', 'blue' ], 'system:archive' ],
    [ 'bad_tag:' ],
    [ '-bad_tag:' ],
    [ 'system:bad_system_pred' ],
    [ 'skirt', 'system:width<400' ],
    [ 'system:untagged' ],
    [ ' bikini ', 'blue    eyes', ' character : space bounty hunter ', ':)', '   ', '', '10', '11', '9', 'system:wew', '-flower' ],
    [ ' bikini ', 'blue    eyes', ' character : space bounty hunter ', ':)', '10', '11', '9', '-flower' ],
]


def main():

    corpus = []
    seen = set()

    for source, texts in [ ( 'reference_tests', reference_test_examples() ), ( 'api_docs', api_doc_examples() ), ( 'systematic', systematic_examples() ) ]:

        for text in texts:

            if text in seen:

                continue


            seen.add( text )
            corpus.append( ( source, text ) )



    system_predicates = []

    for ( source, text ) in corpus:

        result = parse_with_clock( text )

        entry = { 'input' : text, 'source' : source }
        entry.update( result )

        system_predicates.append( entry )


    api_searches = []

    for tags in API_SEARCHES:

        entry = { 'tags' : tags }
        entry.update( parse_api_tags( tags ) )

        api_searches.append( entry )


    services = []

    for ( name, service_type, key, num_stars, allow_zero ) in SERVICE_DEFINITIONS:

        services.append( { 'name' : name, 'type' : service_type, 'key' : key.hex(), 'num_stars' : num_stars, 'allow_zero' : allow_zero } )


    out = {
        'software_version' : HC.SOFTWARE_VERSION,
        'base_now' : BASE_NOW.isoformat(),
        'alt_now' : ALT_NOW.isoformat(),
        'services' : services,
        'url_classes' : URL_CLASS_NAMES,
        'default_view_canvases' : DEFAULT_VIEW_CANVASES,
        'service_groups' : {
            'real_file_services' : sorted( HC.REAL_FILE_SERVICES ),
            'ratings_services' : sorted( HC.RATINGS_SERVICES ),
            'local_ratings_services' : sorted( HC.LOCAL_RATINGS_SERVICES ),
            'all_tag_services' : sorted( HC.ALL_TAG_SERVICES ),
        },
        'filetype_words' : [ [ word, list( enums ) ] for ( word, enums ) in SystemPredicateParser.FILETYPES.items() ],
        'system_predicates' : system_predicates,
        'api_searches' : api_searches,
    }

    # one corpus entry per line, so re-recording gives a readable diff
    lines = [ '{' ]

    for ( i, ( key, value ) ) in enumerate( out.items() ):

        comma = ',' if i < len( out ) - 1 else ''

        if isinstance( value, list ) and key in ( 'system_predicates', 'api_searches', 'filetype_words', 'services' ):

            lines.append( ' {}: ['.format( json.dumps( key ) ) )
            lines.extend( '  {}{}'.format( json.dumps( item, ensure_ascii = False, sort_keys = True ), ',' if j < len( value ) - 1 else '' ) for ( j, item ) in enumerate( value ) )
            lines.append( ' ]' + comma )

        else:

            lines.append( ' {}: {}{}'.format( json.dumps( key ), json.dumps( value, ensure_ascii = False, sort_keys = True ), comma ) )



    lines.append( '}' )

    sys.stdout.write( '\n'.join( lines ) + '\n' )


if __name__ == '__main__':

    main()
