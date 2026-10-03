#!/usr/bin/env python3
"""Record how the reference's "edit rules" dialog lists duplicates
auto-resolution rules (`EditDuplicatesAutoResolutionRulesPanel`), and
what its rule editor says of each rule's parts.

For the reference's suggested rules and the rules
`dump_auto_resolution.extra_rules` builds (every comparator type and
setting), this records each rule's stored tuple and:

* `row`: its row in the list: name, search, comparison, action, progress
  (as a rule not yet in a database has it) and operation;
* `comparators`: each of its comparators' summary (`GetSummary`, as the
  comparator list shows them) and whether it can tell A from B
  (`CanDetermineBetter`);
* `can_determine_better`: whether its pair selector can (an editor's
  "better/worse" action needs it to);

and the editor's choices (`choices`): the operation modes, the actions it
offers, the comparator types "add" offers (label and description), and
the visual duplicates confidences.

It also writes the suggested rules' stored tuples, in order, to
`crates/hydrus-store/src/duplicates/suggested_rules.json`, which
hydrus-store reads for "add suggested" (the reference builds them in
`GetDefaultRuleSuggestions`).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_auto_resolution_summaries.py
       (writes fixtures/auto_resolution_summaries.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'auto_resolution_summaries.json' )
SUGGESTED_OUT = os.path.join( HERE, '..', 'crates', 'hydrus-store', 'src', 'duplicates', 'suggested_rules.json' )


def more_comparators():
    """A rule with the comparator summaries the others don't show: each
    kind of relative file info's words, the action alternates, A and B
    looking at searches, many predicates, and empty AND and OR."""

    from hydrus.core import HydrusConstants as HC
    from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR
    from hydrus.client.duplicates import ClientDuplicatesAutoResolutionComparators as C
    from hydrus.client.files.images import ClientVisualData
    from hydrus.client.metadata import ClientMetadataConditional
    from hydrus.client.search import ClientNumberTest as N
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchPredicate as P

    def rel( predicate_type, operator, multiplier = 1.0, delta = 0, extra = None ):

        c = C.PairComparatorRelativeFileInfo()
        c.SetSystemPredicate( P.Predicate( predicate_type ) )
        c.SetNumberTest( N.NumberTest( operator = operator, value = 1, extra_value = extra ) )
        c.SetMultiplier( multiplier )
        c.SetDelta( delta )

        return c


    def looking( at, predicates ):

        mc = ClientMetadataConditional.MetadataConditional()
        mc.SetFileSearchContext( ClientSearchFileSearchContext.FileSearchContext( predicates = predicates ) )

        c = C.PairComparatorOneFileMetadataConditional()
        c.SetLookingAt( at )
        c.SetMetadataConditional( mc )

        return c


    hard = C.PairComparatorOneFileHardcoded()
    hard.SetLookingAt( C.LOOKING_AT_A )
    hard.SetComparatorType( C.HARDCODED_COMPARATOR_TYPE_ONE_FILE_JPEG_IS_PROGRESSIVE )

    comparators = [
        rel( P.PREDICATE_TYPE_SYSTEM_RATIO, N.NUMBER_TEST_OPERATOR_GREATER_THAN ),
        rel( P.PREDICATE_TYPE_SYSTEM_RATIO, N.NUMBER_TEST_OPERATOR_LESS_THAN_OR_EQUAL_TO, multiplier = 1.25 ),
        rel( P.PREDICATE_TYPE_SYSTEM_DURATION, N.NUMBER_TEST_OPERATOR_GREATER_THAN, delta = 2500 ),
        rel( P.PREDICATE_TYPE_SYSTEM_FRAMERATE, N.NUMBER_TEST_OPERATOR_APPROXIMATE_PERCENT, multiplier = 1.1, extra = 0.05 ),
        rel( P.PREDICATE_TYPE_SYSTEM_NUM_TAGS, N.NUMBER_TEST_OPERATOR_GREATER_THAN_OR_EQUAL_TO, delta = 3 ),
        rel( P.PREDICATE_TYPE_SYSTEM_NUM_PIXELS, N.NUMBER_TEST_OPERATOR_NOT_EQUAL ),
        rel( P.PREDICATE_TYPE_SYSTEM_MODIFIED_TIME, N.NUMBER_TEST_OPERATOR_GREATER_THAN, delta = -86400000 ),
        rel( P.PREDICATE_TYPE_SYSTEM_NUM_URLS, N.NUMBER_TEST_OPERATOR_LESS_THAN, multiplier = 0.5, delta = -2 ),
        looking( C.LOOKING_AT_B, [ P.Predicate( P.PREDICATE_TYPE_SYSTEM_INBOX ) ] ),
        looking( C.LOOKING_AT_A, [ P.Predicate( P.PREDICATE_TYPE_TAG, t ) for t in ( 'a', 'b', 'c', 'd' ) ] ),
        hard,
        C.PairComparatorRelativeVisualDuplicates( ClientVisualData.VISUAL_DUPLICATES_RESULT_NEAR_PERFECT ),
        C.PairComparatorRelativeHardcoded( C.HARDCODED_COMPARATOR_TYPE_TWO_FILES_FILETYPE_DIFFERS ),
        C.PairComparatorRelativeHardcoded( C.HARDCODED_COMPARATOR_TYPE_TWO_FILES_HAS_EXIF_SAME ),
        C.PairComparatorRelativeHardcoded( C.HARDCODED_COMPARATOR_TYPE_TWO_FILES_HAS_ICC_PROFILE_SAME ),
        C.PairComparatorAND( [] ),
        C.PairComparatorOR( [] ),
    ]

    selector = C.PairSelector()
    selector.SetComparators( comparators )

    rule = AR.DuplicatesAutoResolutionRule( 'more comparators' )
    rule.SetPairSelector( selector )
    rule.SetAction( HC.DUPLICATE_ALTERNATE )
    rule.SetDeleteInfo( True, False )
    rule.SetId( 9 )

    return rule


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.client.duplicates import ClientDuplicatesAutoResolution as AR
        from hydrus.client.gui.duplicates import ClientGUIDuplicatesAutoResolution as G

        import dump_auto_resolution

        rules = AR.GetDefaultRuleSuggestions()

        for ( i, rule ) in enumerate( rules ):

            rule.SetId( i + 1 )


        suggested = [ rule.GetName() for rule in rules ]

        rules.extend( dump_auto_resolution.extra_rules() )
        rules.append( more_comparators() )

        panel = G.EditDuplicatesAutoResolutionRulesPanel( gui, [] )

        out = []

        for rule in rules:

            selector = rule.GetPairSelector()

            out.append( {
                'stored' : json.loads( json.dumps( rule.GetSerialisableTuple() ) ),
                'row' : list( panel._ConvertRuleToDisplayTuple( rule ) ),
                'comparators' : [ [ c.GetSummary(), c.CanDetermineBetter() ] for c in selector.GetComparators() ],
                'can_determine_better' : selector.CanDetermineBetter(),
            } )


        # what a comparator list's "add" offers, its question answered by
        # cancelling
        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsQuick

        offered = []

        def select( win, title, choice_tuples, **kwargs ):

            offered.append( { 'title' : title, 'choices' : [ [ c[ 0 ], c[ 2 ] ] for c in choice_tuples ] } )

            raise HydrusExceptions.CancelledException()


        real_select = ClientGUIDialogsQuick.SelectFromListButtons
        ClientGUIDialogsQuick.SelectFromListButtons = select

        try:

            G.EditComparatorList( gui )._AddComparator()

        except HydrusExceptions.VetoException:

            pass

        finally:

            ClientGUIDialogsQuick.SelectFromListButtons = real_select


        choices = {
            'operation_modes' : { str( k ) : v for ( k, v ) in AR.duplicates_auto_resolution_rule_operation_mode_str_lookup.items() },
            'add_comparator' : offered[ 0 ],
        }

        return { 'suggested' : suggested, 'rules' : out, 'choices' : choices }


    return controller.CallBlockingToQt( gui, f )


def child( out ):

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( out, 'w' ) as f:

        json.dump( result, f, ensure_ascii = False )



def main():

    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':

        child( sys.argv[ 2 ] )

        return


    import tempfile

    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'auto_resolution_summaries.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    suggested = [ r[ 'stored' ] for r in result[ 'rules' ] if r[ 'row' ][ 0 ] in result[ 'suggested' ] ]

    with open( SUGGESTED_OUT, 'w' ) as f:

        json.dump( suggested, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "rules" ] )} rules; {SUGGESTED_OUT}: {len( suggested )}' )


if __name__ == '__main__':

    main()
