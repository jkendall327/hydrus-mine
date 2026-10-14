#!/usr/bin/env python3
"""Record how long the reference's downloaders wait after an error, for each
of the three "Delay time on a ..." options in options > downloading > misc.

The real objects run their real error handling with time held still:

- a `GalleryImport` working a search page whose fetch raises a
  `NetworkException` ('downloader_network_error_delay');
- a `WatcherImport` checking a thread whose fetch raises one (the same option);
- a `Subscription` syncing, whose sync raises a `NetworkException`
  ('subscription_network_error_delay');
- a `Subscription` whose location import options have no import destination,
  so that the real `CheckReadyToImport` raises ('subscription_other_error_delay').

Each is run with the option at its default and at a changed value. What is
recorded is the delay (`no_work_until` minus now) and the reason shown.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_error_delays.py
       (writes oracle/fixtures/error_delays.json)
"""
import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )
sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'error_delays.json' )

NOW = 1_000_000

OPTIONS = {
    'downloader_network_error_delay' : 12005,
    'subscription_network_error_delay' : 93660,
    'subscription_other_error_delay' : 176400,
}


def record( session ):

    c = session.controller

    def work():

        from hydrus.core import HydrusExceptions, HydrusTime
        from hydrus.client.importing import ClientImportGallery, ClientImportGallerySeeds, ClientImportWatchers, ClientImportSubscriptions
        from hydrus.client.importing.options import ImportOptionsContainer, LocationImportOptions
        from hydrus.client import ClientLocation

        options = c.new_options

        old = { key : options.GetInteger( key ) for key in OPTIONS }

        # (the recording client has all new network traffic paused, which would stop a subscription starting)
        old_paused = options.GetBoolean( 'pause_all_new_network_traffic' )
        options.SetBoolean( 'pause_all_new_network_traffic', False )

        old_now = HydrusTime.GetNow
        old_work_on_url = ClientImportGallerySeeds.GallerySeed.WorkOnURL

        HydrusTime.GetNow = lambda: NOW

        def refuse( *args, **kwargs ):

            raise HydrusExceptions.NetworkException( 'connection refused\nsecond line' )


        ClientImportGallerySeeds.GallerySeed.WorkOnURL = refuse

        out = { 'now' : NOW, 'cases' : [] }

        def gallery():

            g = ClientImportGallery.GalleryImport( query = 'blue_eyes', source_name = 'booru', initial_search_urls = [ 'https://booru.example/search/blue_eyes/1' ] )

            g._WorkOnGallery()

            return ( g._no_work_until - NOW, g._no_work_until_reason )


        def watcher():

            w = ClientImportWatchers.WatcherImport()

            w._url = 'https://booru.example/thread/1'

            w._CheckWatchableURL()

            return ( w._no_work_until - NOW, w._no_work_until_reason )


        def subscription_network():

            s = ClientImportSubscriptions.Subscription( 'sub' )

            s._SyncQueryLogContainersCanDoWork = lambda: False
            s._SyncQueriesCanDoWork = lambda: True
            s._WorkOnQueriesFilesCanDoWork = lambda: False

            def sync( job_status ):

                raise HydrusExceptions.NetworkException( 'connection refused\nsecond line' )

            s._SyncQueries = sync

            s.Sync()

            return ( s._no_work_until - NOW, s._no_work_until_reason )


        def subscription_other():

            s = ClientImportSubscriptions.Subscription( 'sub' )

            s._SyncQueryLogContainersCanDoWork = lambda: False
            s._WorkOnQueriesFilesCanDoWork = lambda: False

            calls = [ 0 ]

            def can_sync():

                calls[0] += 1

                return calls[0] == 1

            s._SyncQueriesCanDoWork = can_sync
            s._SyncQueries = lambda job_status: None

            location_import_options = LocationImportOptions.LocationImportOptions()
            location_import_options.SetDestinationLocationContext( ClientLocation.LocationContext() )

            container = ImportOptionsContainer.ImportOptionsContainer()
            container.SetImportOptions( location_import_options )

            manager = c.import_options_manager
            old_generate = manager.GenerateFullImportOptionsContainer
            manager.GenerateFullImportOptionsContainer = lambda *args, **kwargs: container

            try:

                s.Sync()

            finally:

                manager.GenerateFullImportOptionsContainer = old_generate


            return ( s._no_work_until - NOW, s._no_work_until_reason )


        runs = [
            ( 'gallery', 'downloader_network_error_delay', gallery ),
            ( 'watcher', 'downloader_network_error_delay', watcher ),
            ( 'subscription network', 'subscription_network_error_delay', subscription_network ),
            ( 'subscription other', 'subscription_other_error_delay', subscription_other ),
        ]

        try:

            for ( name, key, run ) in runs:

                for ( label, value ) in ( ( 'default', old[ key ] ), ( 'changed', OPTIONS[ key ] ) ):

                    options.SetInteger( key, value )

                    ( delay, reason ) = run()

                    out[ 'cases' ].append( { 'what' : name, 'option' : key, 'setting' : label, 'value' : value, 'delay' : delay, 'reason' : reason } )


        finally:

            HydrusTime.GetNow = old_now
            ClientImportGallerySeeds.GallerySeed.WorkOnURL = old_work_on_url

            for ( key, value ) in old.items():

                options.SetInteger( key, value )


            options.SetBoolean( 'pause_all_new_network_traffic', old_paused )


        return out

    return c.CallBlockingToQt( c.gui, work )


def main():

    import hydrus_driver
    import record_api

    if len( sys.argv ) > 1 and sys.argv[1] == '--child':

        output = sys.argv[2]

        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )

        with open( output, 'w' ) as f:

            json.dump( result, f, ensure_ascii = False, indent = 1 )


        return

    with tempfile.TemporaryDirectory() as work:

        output = os.path.join( work, 'result.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', output )

        result = json.load( open( output ) )

        with open( OUT, 'w' ) as f:

            json.dump( result, f, ensure_ascii = False, indent = 1 )
            f.write( '\n' )


        print( 'wrote', OUT )


if __name__ == '__main__':

    main()
