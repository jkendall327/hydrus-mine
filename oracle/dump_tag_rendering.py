#!/usr/bin/env python3
"""Record how the reference shows tags to the user (ClientTags.RenderTag
with render_for_user) under several presentation options.

    python oracle/dump_tag_rendering.py   # writes oracle/fixtures/tag_rendering.json

Each case gives the options in hydrus-rs's `TagPresentation` names and the
reference's rendering of every tag in the corpus.
"""
import json
import os
import sys

ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )
sys.path.insert( 0, ROOT )

JSON_PATH = os.path.join( ROOT, 'oracle', 'fixtures', 'tag_rendering.json' )

TAGS = [
    'blue eyes',
    'blue_eyes',
    'character:samus aran',
    'series:metroid_prime',
    'page:1',
    '1:page',
    'chapter:٣',
    'volume:１２',
    'page:²',
    'page:½',
    'namespace:sub:tag',
    ':smile:',
    'meta:☀️ sunny',
    'emotion:\U0001F600\U0001F600️!',
    'x:a、b',
    'creator:_under_score_',
    '2024',
]

# ( our names, the reference's )
OPTIONS = [
    ( 'show_namespaces', 'show_namespaces' ),
    ( 'show_number_namespaces', 'show_number_namespaces' ),
    ( 'show_subtag_number_namespaces', 'show_subtag_number_namespaces' ),
    ( 'replace_underscores', 'replace_tag_underscores_with_spaces' ),
    ( 'replace_emojis', 'replace_tag_emojis_with_boxes' ),
]

CASES = [
    {},
    { 'show_namespaces': False },
    { 'show_namespaces': False, 'show_number_namespaces': False },
    { 'show_namespaces': False, 'show_subtag_number_namespaces': False },
    { 'show_namespaces': False, 'show_number_namespaces': False, 'show_subtag_number_namespaces': False },
    { 'namespace_connector': ' - ' },
    { 'replace_underscores': True },
    { 'replace_emojis': True },
    { 'replace_underscores': True, 'replace_emojis': True, 'show_namespaces': False, 'namespace_connector': '→' },
]


def main():
    from hydrus.client import ClientGlobals as CG
    from hydrus.client import ClientOptions
    from hydrus.client.metadata import ClientTags

    class ControllerShim( object ):
        pass

    controller = ControllerShim()
    CG.client_controller = controller

    cases = []
    for case in CASES:
        options = ClientOptions.ClientOptions()
        controller.new_options = options
        ours = {}
        for ( name, theirs ) in OPTIONS:
            value = case.get( name, options.GetBoolean( theirs ) )
            options.SetBoolean( theirs, value )
            ours[ name ] = value
        connector = case.get( 'namespace_connector', options.GetString( 'namespace_connector' ) )
        options.SetString( 'namespace_connector', connector )
        ours[ 'namespace_connector' ] = connector
        rendered = { tag: ClientTags.RenderTag( tag, True ) for tag in TAGS }
        cases.append( { 'options': ours, 'rendered': rendered } )
    with open( JSON_PATH, 'w' ) as f:
        json.dump( { 'cases': cases }, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


if __name__ == '__main__':
    main()
