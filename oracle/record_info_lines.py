#!/usr/bin/env python3
"""Record the reference's file info lines (`GetPrettyMediaResultInfoLines`).

On the `basic` fixture, at a fixed "now", each file's info lines: all of
them (with whether each is "interesting", and submenus' lines), and the
interesting ones the media viewer's top hover frame joins with " | ".
With a new client's options, with the file info line options
turned the other way (file services and their import times interesting,
archived and trash times not, the trash reason shown, modified times
always interesting, a different audio label, no nice resolutions), once for each of the eight booleans turned the other
way on its own ("only <name>"), and with file services and their add times
both on.

The phases' option values are recorded too, as hydrus-rs reads them.
Tooltips are not recorded (they show ISO times in local time).

Usage: python oracle/record_info_lines.py      (writes fixtures/info_lines.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'info_lines.json' )
MANIFEST = json.load( open( os.path.join( HERE, 'fixtures', 'legacy_db', 'basic.manifest.json' ) ) )

BOOLEANS = [
    'file_info_line_consider_archived_interesting',
    'file_info_line_consider_archived_time_interesting',
    'file_info_line_consider_file_services_interesting',
    'file_info_line_consider_file_services_import_times_interesting',
    'file_info_line_consider_trash_time_interesting',
    'file_info_line_consider_trash_reason_interesting',
    'hide_uninteresting_modified_time',
    'use_nice_resolution_strings',
]


def describe( lines ):

    out = []

    for line in lines:

        if line.IsSubmenu():

            out.append( { 'submenu': line.text, 'interesting': line.interesting, 'lines': describe( line.sublines ) } )

        else:

            out.append( { 'text': line.text, 'interesting': line.interesting } )


    return out


def record( session ):

    from hydrus.core import HydrusTime
    from hydrus.client.media import ClientMediaResultPrettyInfo

    controller = session.controller
    options = controller.new_options

    now = int( time.time() )

    phases = {}

    defaults = { name: options.GetBoolean( name ) for name in BOOLEANS }
    defaults_label = options.GetString( 'has_audio_label' )

    real = ( HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat )

    HydrusTime.GetNow = lambda: now
    HydrusTime.GetNowMS = lambda: now * 1000
    HydrusTime.GetNowFloat = lambda: float( now )

    try:

        hashes = [ bytes.fromhex( f[ 'hash' ] ) for f in MANIFEST[ 'files' ] ]

        media_results = controller.Read( 'media_results', hashes )

        # 'defaults', 'changed' (all eight turned the other way), and one phase
        # per boolean turned the other way alone
        for phase in ( 'defaults', 'changed' ) + tuple( 'only ' + name for name in BOOLEANS ) + ( 'services and their add times', ):

            if phase.startswith( 'only ' ):

                for name in BOOLEANS:

                    options.SetBoolean( name, defaults[ name ] )


                options.SetString( 'has_audio_label', defaults_label )

                name = phase[ len( 'only ' ): ]

                options.SetBoolean( name, not defaults[ name ] )

            elif phase == 'services and their add times':

                # (add times show only where services do)
                for name in BOOLEANS:

                    options.SetBoolean( name, defaults[ name ] )


                options.SetString( 'has_audio_label', defaults_label )

                for name in ( 'file_info_line_consider_file_services_interesting', 'file_info_line_consider_file_services_import_times_interesting' ):

                    options.SetBoolean( name, not defaults[ name ] )


            elif phase == 'defaults':

                for name in BOOLEANS:

                    options.SetBoolean( name, defaults[ name ] )


                options.SetString( 'has_audio_label', defaults_label )

            if phase == 'changed':

                for name in BOOLEANS:

                    options.SetBoolean( name, not options.GetBoolean( name ) )


                options.SetString( 'has_audio_label', 'with audio' )


            files = {}

            for media_result in media_results:

                every = ClientMediaResultPrettyInfo.GetPrettyMediaResultInfoLines( media_result )
                interesting = ClientMediaResultPrettyInfo.GetPrettyMediaResultInfoLines( media_result, only_interesting_lines = True )

                top = ' | '.join( line.text for line in interesting if not line.IsSubmenu() )

                files[ media_result.GetHash().hex() ] = { 'lines': describe( every ), 'top': top }


            phases[ phase ] = {
                'booleans': { name: options.GetBoolean( name ) for name in BOOLEANS },
                'has_audio_label': options.GetString( 'has_audio_label' ),
                'files': files,
            }


    finally:

        ( HydrusTime.GetNow, HydrusTime.GetNowMS, HydrusTime.GetNowFloat ) = real


    return { 'now': now, 'phases': phases }


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
