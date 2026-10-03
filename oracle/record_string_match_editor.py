#!/usr/bin/env python3
"""Record the reference's string match editor (`EditStringMatchPanel`).

The editor has a match type ("any characters", "fixed characters",
"character set", "regex") and, by type, the fixed text, the character set
or the regex, the minimum and maximum numbers of characters ("no limit"),
and an example string; under them, whether the example matches ("Example
matches ok!", or "Example does not match - " and why). A fixed text or
regex left empty takes the example when its type is chosen; a fixed match
has no limits and its text as its example. "ok" refuses a match its
example fails ("Please enter an example text that matches the given
rules!").

On the `basic` fixture, for each case of `CASES` (a match to start from,
as `[type, value, min, max, example]`, and steps), this makes the panel
and records, after each step, the type, which rows show, the fields, the
test result and whether it shows as valid, and what "ok" gives (or says).
Steps: `["type", label]`, `["fixed", text]`, `["regex", text]`,
`["flexible", label]`, `["min", n]`, `["max", n]` (`null` for no limit)
and `["example", text]`.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_string_match_editor.py
       (writes fixtures/string_match_editor.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'string_match_editor.json' )

ANY = 3
FIXED = 0
FLEXIBLE = 1
REGEX = 2

CASES = [
    { 'start' : [ ANY, '', None, None, 'example string' ], 'do' : [
        [ 'min', 3 ],
        [ 'example', 'ab' ],
        [ 'max', 5 ],
        [ 'example', 'abcdefg' ],
        [ 'example', 'abcd' ],
        [ 'type', 'fixed characters' ],
        [ 'fixed', 'other' ],
        [ 'type', 'regex' ],
        [ 'regex', r'\d+' ],
        [ 'example', 'ab12' ],
        [ 'regex', '(' ],
        [ 'type', 'character set' ],
        [ 'flexible', 'numeric characters (0-9)' ],
        [ 'example', '123' ],
        [ 'flexible', 'hexadecimal characters (0-9a-fA-F)' ],
        [ 'example', 'beefy' ],
        [ 'flexible', 'base64 characters (url encoded) (a-zA-z0-9 %2B %2F with %3D padding)' ],
        [ 'min', None ],
        [ 'max', None ],
        [ 'type', 'any characters' ],
    ] },
    { 'start' : [ FIXED, 'hello', None, None, 'hello' ], 'do' : [
        [ 'type', 'regex' ],
        [ 'example', 'say hello' ],
        [ 'type', 'fixed characters' ],
        [ 'fixed', '' ],
        [ 'type', 'any characters' ],
        [ 'type', 'fixed characters' ],
    ] },
    { 'start' : [ REGEX, '^a', 2, None, 'abc' ], 'do' : [
        [ 'example', 'a' ],
        [ 'example', 'ba' ],
        [ 'example', 'x\n a' ],
        [ 'type', 'character set' ],
        [ 'flexible', 'alphabetic characters (a-zA-Z)' ],
        [ 'example', 'abc' ],
        [ 'flexible', 'base64url characters (a-zA-z0-9-_ optionally with = padding)' ],
        [ 'example', 'ab-_==' ],
        [ 'flexible', 'base64 characters (a-zA-z0-9+/ with = padding)' ],
        [ 'flexible', 'alphanumeric characters (a-zA-Z0-9)' ],
    ] },
    { 'start' : [ FLEXIBLE, 2, 1, 4, '1234' ], 'do' : [
        [ 'max', 3 ],
        [ 'example', '12345' ],
    ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.client import ClientStrings
        from hydrus.client.gui import ClientGUIStringPanels as M
        from hydrus.core import HydrusExceptions

        out = []

        for case in CASES:

            ( match_type, match_value, min_chars, max_chars, example ) = case[ 'start' ]

            string_match = ClientStrings.StringMatch( match_type = match_type, match_value = match_value, min_chars = min_chars, max_chars = max_chars, example_string = example )

            panel = M.EditStringMatchPanel( gui, string_match )

            def state():

                s = {
                    'type' : panel._match_type.currentText(),
                    'shown' : {
                        'fixed' : not panel._match_value_fixed_input.isHidden(),
                        'regex' : not panel._match_value_regex_input.isHidden(),
                        'flexible' : not panel._match_value_flexible_input.isHidden(),
                        'limits' : not panel._min_chars.isHidden(),
                    },
                    'fixed' : panel._match_value_fixed_input.text(),
                    'regex' : panel._match_value_regex_input.GetValue(),
                    'flexible' : panel._match_value_flexible_input.currentText(),
                    'min' : panel._min_chars.GetValue(),
                    'max' : panel._max_chars.GetValue(),
                    'example' : panel._example_string.text(),
                    'test' : panel._example_string_matches.text(),
                    'valid' : panel._example_string_matches.objectName(),
                }

                try:

                    v = panel.GetValue()

                    s[ 'value' ] = list( v.ToTuple() )

                except HydrusExceptions.VetoException as e:

                    s[ 'veto' ] = str( e )


                return s


            steps = [ { 'state' : state() } ]

            for step in case[ 'do' ]:

                ( kind, v ) = step

                if kind == 'type':

                    i = [ panel._match_type.itemText( i ) for i in range( panel._match_type.count() ) ].index( v )

                    panel._match_type.setCurrentIndex( i )

                elif kind == 'fixed':

                    panel._match_value_fixed_input.setText( v )

                elif kind == 'regex':

                    panel._match_value_regex_input.SetValue( v )

                elif kind == 'flexible':

                    i = [ panel._match_value_flexible_input.itemText( i ) for i in range( panel._match_value_flexible_input.count() ) ].index( v )

                    panel._match_value_flexible_input.setCurrentIndex( i )

                elif kind == 'min':

                    panel._min_chars.SetValue( v )

                elif kind == 'max':

                    panel._max_chars.SetValue( v )

                elif kind == 'example':

                    panel._example_string.setText( v )


                steps.append( { 'do' : step, 'state' : state() } )


            out.append( { 'start' : case[ 'start' ], 'steps' : steps } )


        return { 'cases' : out }


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

        path = os.path.join( work, 'string_match_editor.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()
