#!/usr/bin/env python3
"""Record which volume and mute the reference's preview plays at, what its
volume control moves, and which kinds of file its preview plays.

In the running client, on the `basic` fixture: for every combination of
`preview_uses_its_own_audio_volume` and the three volumes and mutes,
`ClientGUIMediaVolume.GetCorrectCurrentVolume` / `GetCorrectCurrentMute` for
the preview canvas (and, as the contrast, the media viewer's); the real
`VolumeControl` popup of the preview canvas, with the volume type its slider
is made with and the option each of its controls (the slider, the preview's
mute, the global mute) changes; and `GetPreviewShowAction` for the kinds of
file that play.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_preview_audio.py
       (writes fixtures/preview_audio.json)
"""

import itertools
import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'preview_audio.json' )

GLOBAL_VOLUME = 30
PREVIEW_VOLUME = 60
VIEWER_VOLUME = 90

MUTE_OPTIONS = [ 'global_audio_mute', 'preview_audio_mute', 'media_viewer_audio_mute' ]


def record( session ):

    controller = session.controller
    gui = controller.gui

    from qtpy import QtWidgets as QW

    from hydrus.core import HydrusConstants as HC
    from hydrus.client import ClientConstants as CC
    from hydrus.client.gui.media import ClientGUIMediaControls
    from hydrus.client.gui.media import ClientGUIMediaVolume

    options = controller.new_options

    result = {
        'volumes' : { 'global' : GLOBAL_VOLUME, 'preview' : PREVIEW_VOLUME, 'viewer' : VIEWER_VOLUME },
        'defaults' : {},
        'cases' : [],
        'controls' : [],
        'show_actions' : [],
    }

    # what a fresh client has
    for name in [ 'preview_uses_its_own_audio_volume', 'media_viewer_uses_its_own_audio_volume', 'global_audio_mute', 'preview_audio_mute', 'media_viewer_audio_mute' ]:

        result[ 'defaults' ][ name ] = options.GetBoolean( name )

    for name in [ 'global_audio_volume', 'preview_audio_volume', 'media_viewer_audio_volume' ]:

        result[ 'defaults' ][ name ] = options.GetInteger( name )

    def work():

        for ( preview_own, viewer_own ) in itertools.product( [ False, True ], repeat = 2 ):

            for mutes in itertools.product( [ False, True ], repeat = 3 ):

                options.SetBoolean( 'preview_uses_its_own_audio_volume', preview_own )
                options.SetBoolean( 'media_viewer_uses_its_own_audio_volume', viewer_own )

                options.SetInteger( 'global_audio_volume', GLOBAL_VOLUME )
                options.SetInteger( 'preview_audio_volume', PREVIEW_VOLUME )
                options.SetInteger( 'media_viewer_audio_volume', VIEWER_VOLUME )

                for ( name, mute ) in zip( MUTE_OPTIONS, mutes ):

                    options.SetBoolean( name, mute )

                result[ 'cases' ].append( {
                    'preview_uses_its_own_volume' : preview_own,
                    'viewer_uses_its_own_volume' : viewer_own,
                    'global_mute' : mutes[0],
                    'preview_mute' : mutes[1],
                    'viewer_mute' : mutes[2],
                    'preview_volume' : ClientGUIMediaVolume.GetCorrectCurrentVolume( CC.CANVAS_PREVIEW ),
                    'preview_muted' : ClientGUIMediaVolume.GetCorrectCurrentMute( CC.CANVAS_PREVIEW ),
                    'viewer_volume' : ClientGUIMediaVolume.GetCorrectCurrentVolume( CC.CANVAS_MEDIA_VIEWER ),
                    'viewer_muted' : ClientGUIMediaVolume.GetCorrectCurrentMute( CC.CANVAS_MEDIA_VIEWER ),
                } )

        # the real preview control: which volume its slider is made for, and
        # what each control changes
        for preview_own in [ False, True ]:

            options.SetBoolean( 'preview_uses_its_own_audio_volume', preview_own )
            options.SetInteger( 'global_audio_volume', GLOBAL_VOLUME )
            options.SetInteger( 'preview_audio_volume', PREVIEW_VOLUME )

            for name in MUTE_OPTIONS:

                options.SetBoolean( name, False )

            control = ClientGUIMediaControls.VolumeControl( gui, CC.CANVAS_PREVIEW, direction = 'up' )

            popup = control._popup_window

            slider = popup._volume

            slider_type = slider.GetVolumeType()
            slider_shows = slider.value()

            slider.setValue( 17 )

            QW.QApplication.processEvents()

            after_slider = {
                'global_audio_volume' : options.GetInteger( 'global_audio_volume' ),
                'preview_audio_volume' : options.GetInteger( 'preview_audio_volume' ),
                'media_viewer_audio_volume' : options.GetInteger( 'media_viewer_audio_volume' ),
            }

            popup._specific_mute.click()

            QW.QApplication.processEvents()

            after_preview_mute = { name : options.GetBoolean( name ) for name in MUTE_OPTIONS }

            control._global_mute.click()

            QW.QApplication.processEvents()

            after_global_mute = { name : options.GetBoolean( name ) for name in MUTE_OPTIONS }

            result[ 'controls' ].append( {
                'preview_uses_its_own_volume' : preview_own,
                'slider_type' : ClientGUIMediaControls.volume_types_str_lookup[ slider_type ],
                'slider_shows' : slider_shows,
                'after_slider_set_to_17' : after_slider,
                'after_preview_mute_clicked' : after_preview_mute,
                'after_global_mute_clicked' : after_global_mute,
            } )

            control.deleteLater()

        # which kinds of file the preview plays, by default
        for mime in [ HC.VIDEO_MP4, HC.VIDEO_WEBM, HC.VIDEO_MKV, HC.AUDIO_MP3, HC.AUDIO_FLAC, HC.ANIMATION_GIF, HC.ANIMATION_APNG, HC.ANIMATION_UGOIRA, HC.IMAGE_PNG, HC.IMAGE_JPEG, HC.APPLICATION_PDF ]:

            ( show_action, start_paused, start_with_embed ) = options.GetPreviewShowAction( mime )

            result[ 'show_actions' ].append( {
                'mime' : HC.mime_string_lookup[ mime ],
                'mime_id' : mime,
                'show_action' : show_action,
                'show_action_is_mpv' : show_action == CC.MEDIA_VIEWER_ACTION_SHOW_WITH_MPV,
                'start_paused' : start_paused,
                'start_with_embed' : start_with_embed,
            } )

    controller.CallBlockingToQt( gui, work )

    return result


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

        path = os.path.join( work, 'preview_audio.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )

    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()
