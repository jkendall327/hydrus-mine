#!/usr/bin/env python3
"""Record duplicates auto-resolution rules as the reference stores them.

The reference's suggested rules, plus rules built here to use every
comparator type, custom merge options and every setting, each as its stored
tuple, with a plain description of what it holds (for the decoder test).
Rules are also written at older versions (v1, v2) by undoing the
reference's upgrades, to check those are read the same.

Also the owner's own rules, as their client stored them
(fixtures/user_auto_resolution_rules.txt: a line per rule, `name|version|
serialised info`), with what the reference reads from each.

Usage: QT_QPA_PLATFORM=offscreen python oracle/dump_auto_resolution.py
"""

import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

OUT = os.path.join( HERE, 'fixtures', 'auto_resolution.json' )
USER_RULES = os.path.join( HERE, 'fixtures', 'user_auto_resolution_rules.txt' )


def describe_search( fsc ):

    return {
        'location' : sorted( k.hex() for k in fsc.GetLocationContext().current_service_keys ),
        'predicates' : [ json.loads( json.dumps( p.GetSerialisableTuple() ) ) for p in fsc.GetPredicates() ],
    }


def describe_comparator( c ):

    from hydrus.client.duplicates import ClientDuplicatesAutoResolutionComparators as C

    if isinstance( c, C.PairComparatorOneFileMetadataConditional ):

        return { 'type' : 'one_file_metadata', 'looking_at' : c.GetLookingAt(), 'search' : describe_search( c.GetMetadataConditional().GetFileSearchContext() ) }

    elif isinstance( c, C.PairComparatorOneFileHardcoded ):

        return { 'type' : 'one_file_hardcoded', 'looking_at' : c.GetLookingAt(), 'test' : c.GetComparatorType() }

    elif isinstance( c, C.PairComparatorRelativeFileInfo ):

        nt = c.GetNumberTest()

        return { 'type' : 'relative_file_info', 'property' : c.GetSystemPredicate().GetType(), 'op' : nt.operator, 'value' : nt.value, 'extra' : nt.extra_value, 'multiplier' : c.GetMultiplier(), 'delta' : c.GetDelta() }

    elif isinstance( c, C.PairComparatorRelativeHardcoded ):

        return { 'type' : 'relative_hardcoded', 'test' : c._hardcoded_type }

    elif isinstance( c, C.PairComparatorRelativeVisualDuplicates ):

        return { 'type' : 'visual_duplicates', 'confidence' : c.GetAcceptableConfidence() }

    elif isinstance( c, C.PairComparatorOR ):

        return { 'type' : 'or', 'members' : [ describe_comparator( m ) for m in c.GetComparators() ] }

    elif isinstance( c, C.PairComparatorAND ):

        return { 'type' : 'and', 'members' : [ describe_comparator( m ) for m in c.GetComparators() ] }


    raise Exception( f'unknown comparator {c}' )


def describe( rule ):

    pdsc = rule.GetPotentialDuplicatesSearchContext()

    merge = rule.GetDuplicateContentMergeOptions()

    return {
        'name' : rule.GetName(),
        'id' : rule.GetId(),
        'paused' : rule.IsPaused(),
        'operation_mode' : rule.GetOperationMode(),
        'max_pending_pairs' : rule.GetMaxPendingPairs(),
        'search_1' : describe_search( pdsc.GetFileSearchContext1() ),
        'search_2' : describe_search( pdsc.GetFileSearchContext2() ),
        'dupe_search_type' : pdsc.GetDupeSearchType(),
        'pixel_dupes' : pdsc.GetPixelDupesPreference(),
        'max_hamming_distance' : pdsc.GetMaxHammingDistance(),
        'comparators' : [ describe_comparator( c ) for c in rule.GetPairSelector().GetComparators() ],
        'action' : rule.GetAction(),
        'delete' : list( rule.GetDeleteInfo() ),
        'custom_merge' : None if merge is None else json.loads( json.dumps( merge.GetSerialisableTuple() ) ),
    }


def extra_rules():

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.duplicates import ClientDuplicates
    from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR
    from hydrus.client.duplicates import ClientDuplicatesAutoResolutionComparators as C
    from hydrus.client.duplicates import ClientPotentialDuplicatesSearchContext as PDSC
    from hydrus.client.files.images import ClientVisualData
    from hydrus.client.metadata import ClientMetadataConditional
    from hydrus.client.search import ClientNumberTest
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchPredicate as P

    rules = []

    def conditional( predicates ):

        mc = ClientMetadataConditional.MetadataConditional()
        mc.SetFileSearchContext( ClientSearchFileSearchContext.FileSearchContext( predicates = predicates ) )

        return mc


    # every comparator type, nested
    rule = AR.DuplicatesAutoResolutionRule( 'everything' )

    pdsc = PDSC.PotentialDuplicatesSearchContext( location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY ) )
    pdsc.SetFileSearchContext1( ClientSearchFileSearchContext.FileSearchContext( location_context = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY ), predicates = [ P.Predicate( P.PREDICATE_TYPE_TAG, 'blue', False ), P.Predicate( P.PREDICATE_TYPE_SYSTEM_INBOX ) ] ) )
    pdsc.SetDupeSearchType( ClientDuplicates.DUPE_SEARCH_BOTH_FILES_MATCH_ONE_SEARCH )
    pdsc.SetPixelDupesPreference( ClientDuplicates.SIMILAR_FILES_PIXEL_DUPES_EXCLUDED )
    pdsc.SetMaxHammingDistance( 8 )

    rule.SetPotentialDuplicatesSearchContext( pdsc )

    one = C.PairComparatorOneFileMetadataConditional()
    one.SetLookingAt( C.LOOKING_AT_EITHER )
    one.SetMetadataConditional( conditional( [ P.Predicate( P.PREDICATE_TYPE_SYSTEM_MIME, value = ( HC.IMAGE_PNG, HC.GENERAL_VIDEO ) ), P.Predicate( P.PREDICATE_TYPE_SYSTEM_HAS_EXIF, True ) ] ) )

    hard = C.PairComparatorOneFileHardcoded()
    hard.SetLookingAt( C.LOOKING_AT_B )
    hard.SetComparatorType( C.HARDCODED_COMPARATOR_TYPE_ONE_FILE_JPEG_IS_NOT_PROGRESSIVE )

    rel = C.PairComparatorRelativeFileInfo()
    rel.SetSystemPredicate( P.Predicate( P.PREDICATE_TYPE_SYSTEM_IMPORT_TIME ) )
    rel.SetNumberTest( ClientNumberTest.NumberTest( operator = ClientNumberTest.NUMBER_TEST_OPERATOR_APPROXIMATE_ABSOLUTE, value = 1, extra_value = 60000 ) )
    rel.SetDelta( -5000 )

    rel2 = C.PairComparatorRelativeFileInfo()
    rel2.SetSystemPredicate( P.Predicate( P.PREDICATE_TYPE_SYSTEM_SIZE ) )
    rel2.SetNumberTest( ClientNumberTest.NumberTest( operator = ClientNumberTest.NUMBER_TEST_OPERATOR_APPROXIMATE_PERCENT, value = 1, extra_value = 0.2 ) )
    rel2.SetMultiplier( 1.5 )

    visual = C.PairComparatorRelativeVisualDuplicates( ClientVisualData.VISUAL_DUPLICATES_RESULT_VERY_PROBABLY )

    either = C.PairComparatorOR( [ C.PairComparatorRelativeHardcoded( C.HARDCODED_COMPARATOR_TYPE_TWO_FILES_A_HAS_CLEARLY_BETTER_JPEG_QUALITY ), C.PairComparatorAND( [ rel2, C.PairComparatorRelativeHardcoded( C.HARDCODED_COMPARATOR_TYPE_TWO_FILES_FILETYPE_SAME ) ] ) ] )

    selector = C.PairSelector()
    selector.SetComparators( [ one, hard, rel, visual, either ] )

    rule.SetPairSelector( selector )
    rule.SetAction( HC.DUPLICATE_SAME_QUALITY )
    rule.SetDeleteInfo( False, True )
    rule.SetOperationMode( AR.DUPLICATES_AUTO_RESOLUTION_RULE_OPERATION_MODE_FULLY_AUTOMATIC )
    rule.SetMaxPendingPairs( None )
    rule.SetPaused( True )

    merge = ClientDuplicates.DuplicateContentMergeOptions()
    merge.SetTagServiceActions( [ ( CC.DEFAULT_LOCAL_TAG_SERVICE_KEY, HC.CONTENT_MERGE_ACTION_COPY, __import__( 'hydrus.core.HydrusTags', fromlist = [ 'x' ] ).TagFilter() ) ] )
    rule.SetDuplicateContentMergeOptions( merge )

    rule.SetId( 7 )

    rules.append( rule )

    # the empty defaults
    rule = AR.DuplicatesAutoResolutionRule( 'defaults' )
    rule.SetId( 8 )

    rules.append( rule )

    return rules


def dump( session ):

    from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR

    rules = AR.GetDefaultRuleSuggestions()

    for ( i, rule ) in enumerate( rules ):

        rule.SetId( i + 1 )


    rules.extend( extra_rules() )

    out = []

    for rule in rules:

        stored = json.loads( json.dumps( rule.GetSerialisableTuple() ) )

        expected = describe( rule )

        out.append( { 'stored' : stored, 'expected' : expected } )

        # the same rule as v2 and v1 would have stored it
        ( kind, name, version, info ) = stored
        ( rule_id, paused, mode, max_pending, search, selector, action, delete_a, delete_b, merge ) = info

        if not paused:

            v2 = [ rule_id, mode, max_pending, search, selector, action, delete_a, delete_b, merge ]

            out.append( { 'stored' : [ kind, name, 2, v2 ], 'expected' : expected } )

            if max_pending == 500:

                v1 = [ rule_id, mode, search, selector, action, delete_a, delete_b, merge ]

                out.append( { 'stored' : [ kind, name, 1, v1 ], 'expected' : expected } )



        else:

            # v2 said "paused" with the paused operation mode
            v2 = [ rule_id, 0, max_pending, search, selector, action, delete_a, delete_b, merge ]

            v2_expected = dict( expected, operation_mode = 1 )

            out.append( { 'stored' : [ kind, name, 2, v2 ], 'expected' : v2_expected } )



    from hydrus.core import HydrusSerialisable

    with open( USER_RULES, encoding = 'utf-8' ) as f:

        for line in f:

            if line.strip() == '':

                continue


            ( name, version, info ) = line.rstrip( '\n' ).split( '|', 2 )

            stored = [ HydrusSerialisable.SERIALISABLE_TYPE_DUPLICATES_AUTO_RESOLUTION_RULE, name, int( version ), json.loads( info ) ]

            rule = HydrusSerialisable.CreateFromSerialisableTuple( stored )

            out.append( { 'stored' : stored, 'expected' : describe( rule ), 'source' : 'owner' } )



    return out


def main():

    work = tempfile.mkdtemp( prefix = 'hydrus_auto_resolution_' )

    try:

        rules = hydrus_driver.run_client( os.path.join( work, 'db' ), dump )

    finally:

        shutil.rmtree( work, ignore_errors = True )


    with open( OUT, 'w' ) as f:

        json.dump( { 'rules' : rules }, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( rules )} rules' )


if __name__ == '__main__':

    main()
