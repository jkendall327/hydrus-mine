#!/usr/bin/env python3
"""Record the reference's tag summaries (`TagSummaryGenerator`): the
three default generators and random ones, as a database stores them, and
the summary each makes of random sets of tags, with the reference's
default tag presentation, and with underscores shown as spaces.

    QT_QPA_PLATFORM=offscreen python oracle/dump_tag_summaries.py
    # writes oracle/fixtures/tag_summaries.json
"""
import json
import os
import random
import sys

ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )
sys.path.insert( 0, ROOT )

JSON_PATH = os.path.join( ROOT, 'oracle', 'fixtures', 'tag_summaries.json' )

rng = random.Random( 61 )

TAGS = [
    'creator:someone', 'creator:another artist', 'creator:under_score',
    'series:metroid', 'series:the legend of zelda', 'title:a test image: with colons',
    'character:samus aran', 'blue eyes', 'solo',
    'page:1', 'page:2', 'page:3', 'page:10', 'page:11', 'page:2a', 'page:010', 'page:cover',
    'chapter:3', 'chapter:12', 'volume:1', 'volume:2',
]

NAMESPACES = [ 'creator', 'series', 'title', 'character', 'page', 'chapter', 'volume', '' ]


def generator_facts( g ):

    ( background, text, namespace_info, separator, example_tags, show ) = g.ToTuple()

    return {
        'background' : [ background.red(), background.green(), background.blue(), background.alpha() ],
        'text' : [ text.red(), text.green(), text.blue(), text.alpha() ],
        'namespace_info' : [ list( row ) for row in namespace_info ],
        'separator' : separator,
        'example_tags' : list( example_tags ),
        'show' : show,
    }


def main():

    from qtpy import QtGui as QG

    from hydrus.client import ClientGlobals as CG
    from hydrus.client import ClientOptions
    from hydrus.client.gui.metadata import ClientGUITagSummaryGenerator

    class ControllerShim( object ):
        pass

    controller = ControllerShim()
    CG.client_controller = controller
    options = ClientOptions.ClientOptions()
    controller.new_options = options

    TSG = ClientGUITagSummaryGenerator.TagSummaryGenerator

    generators = [
        ( 'thumbnail_top', options.GetTagSummaryGenerator( 'thumbnail_top' ) ),
        ( 'thumbnail_bottom_right', options.GetTagSummaryGenerator( 'thumbnail_bottom_right' ) ),
        ( 'media_viewer_top', options.GetTagSummaryGenerator( 'media_viewer_top' ) ),
    ]

    for i in range( 8 ):

        namespace_info = [ ( ns, rng.choice( [ '', 'p', 'c.', '#' ] ), rng.choice( [ ', ', '-', '/' ] ) ) for ns in rng.sample( NAMESPACES, rng.randint( 1, 4 ) ) ]

        g = TSG(
            background_colour = QG.QColor( rng.randrange( 256 ), rng.randrange( 256 ), rng.randrange( 256 ), rng.randrange( 256 ) ),
            text_colour = QG.QColor( rng.randrange( 256 ), rng.randrange( 256 ), rng.randrange( 256 ), 255 ),
            namespace_info = namespace_info,
            separator = rng.choice( [ ' - ', '|', '' ] ),
            example_tags = rng.sample( TAGS, 3 ),
            show = rng.random() > 0.15,
        )

        generators.append( ( f'random {i}', g ) )


    tag_sets = [ rng.sample( TAGS, rng.randint( 0, 8 ) ) for _ in range( 12 ) ]
    tag_sets.append( [ 'page:1', 'page:2', 'page:3', 'page:10' ] )
    tag_sets.append( [ 'page:3', 'page:1', 'page:2' ] )
    tag_sets.append( [ 'page:1', 'page:2' ] )
    tag_sets.append( [ 'page:1', 'page:2', 'page:2a' ] )
    tag_sets.append( list( TAGS ) )

    out = { 'tag_sets' : tag_sets, 'generators' : [] }

    for ( name, g ) in generators:

        out[ 'generators' ].append( {
            'name' : name,
            'stored' : json.loads( json.dumps( g.GetSerialisableTuple() ) ),
            'facts' : generator_facts( g ),
            'example' : g.GenerateExampleSummary(),
            'summaries' : [ g.GenerateSummary( tags ) for tags in tag_sets ],
        } )


    # and with underscores shown as spaces, as the user may have tags shown
    options.SetBoolean( 'replace_tag_underscores_with_spaces', True )

    for ( ( name, g ), case ) in zip( generators, out[ 'generators' ] ):

        case[ 'summaries_underscores_replaced' ] = [ g.GenerateSummary( tags ) for tags in tag_sets ]


    with open( JSON_PATH, 'w' ) as f:

        json.dump( out, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {JSON_PATH}' )


if __name__ == '__main__':

    main()
