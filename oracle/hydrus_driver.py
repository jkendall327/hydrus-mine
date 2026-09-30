"""Drive the real (reference) hydrus client headlessly.

`run_client( db_dir, hook )` boots the unmodified Python client in this
process with an offscreen Qt platform, waits for boot to finish, then runs
`hook( session )` on a worker thread with access to the live controller and
(optionally) the client's own Client API over HTTP. When the hook returns, the
client is shut down cleanly through its normal SIGTERM path, so everything is
committed to disk.

Only one client can run per process (the reference uses module globals), so
callers that need several boots should run each in a subprocess; see
`run_in_subprocess`.
"""

import json
import os
import signal
import subprocess
import sys
import threading
import time
import traceback
import urllib.error
import urllib.parse
import urllib.request

REPO_ROOT = os.path.abspath( os.path.join( os.path.dirname( __file__ ), '..' ) )

if REPO_ROOT not in sys.path:

    sys.path.insert( 0, REPO_ROOT )


os.environ.setdefault( 'QT_QPA_PLATFORM', 'offscreen' )

# Deliberately fixed so recordings are reproducible. Only ever used on
# throwaway fixture databases bound to localhost.
ORACLE_ACCESS_KEY = bytes.fromhex( '0123456789abcdef' * 4 )
ORACLE_RESTRICTED_ACCESS_KEY = bytes.fromhex( 'fedcba9876543210' * 4 )


class ApiError( Exception ):

    def __init__( self, status, body ):

        super().__init__( f'HTTP {status}: {body[:500]!r}' )

        self.status = status
        self.body = body



class Api( object ):
    """A minimal Client API client (stdlib only, so it cannot disturb the
    reference's own networking stack)."""

    def __init__( self, port, access_key = ORACLE_ACCESS_KEY ):

        self.base = f'http://127.0.0.1:{port}'
        self.access_key = access_key


    def request( self, method, path, query = None, json_body = None, data = None, headers = None ):
        """Returns ( status, content_type, body_bytes ). Never raises on HTTP errors."""

        url = self.base + path

        if query:

            url += '?' + urllib.parse.urlencode( query, doseq = False )


        all_headers = { 'Hydrus-Client-API-Access-Key': self.access_key.hex() } if self.access_key is not None else {}

        if json_body is not None:

            data = json.dumps( json_body ).encode( 'utf-8' )
            all_headers[ 'Content-Type' ] = 'application/json'


        if headers:

            all_headers.update( headers )


        req = urllib.request.Request( url, data = data, method = method, headers = all_headers )

        try:

            with urllib.request.urlopen( req, timeout = 120 ) as response:

                return ( response.status, response.headers.get( 'Content-Type', '' ), response.read() )


        except urllib.error.HTTPError as e:

            return ( e.code, e.headers.get( 'Content-Type', '' ), e.read() )



    def get( self, path, **query ):

        return self._json_or_raise( *self.request( 'GET', path, query = query ) )


    def post( self, path, body ):

        return self._json_or_raise( *self.request( 'POST', path, json_body = body ) )


    def _json_or_raise( self, status, content_type, body ):

        if status != 200:

            raise ApiError( status, body )


        if 'json' in content_type:

            return json.loads( body )


        return body



class Session( object ):
    """Handed to hooks: the live controller plus helpers."""

    def __init__( self, controller, port ):

        self.controller = controller
        self.port = port
        self.api = Api( port ) if port is not None else None


    def write( self, action, *args, **kwargs ):

        return self.controller.WriteSynchronous( action, *args, **kwargs )


    def read( self, action, *args, **kwargs ):

        return self.controller.Read( action, *args, **kwargs )


    def wait_for_api( self, timeout = 30 ):

        deadline = time.time() + timeout

        while time.time() < deadline:

            try:

                ( status, _, _ ) = self.api.request( 'GET', '/api_version' )

                if status == 200:

                    return


            except OSError:

                pass


            time.sleep( 0.2 )


        raise Exception( 'Client API did not come up' )


    def sync_tag_display( self ):
        """Force sibling/parent display caches fully up to date. The real client
        does this lazily in idle time; recordings need it done now."""

        from hydrus.core import HydrusConstants as HC

        for service in self.controller.services_manager.GetServices( ( HC.LOCAL_TAG, HC.TAG_REPOSITORY ) ):

            while self.write( 'sync_tag_display_maintenance', service.GetServiceKey(), 1 ):

                pass





def enable_client_api( controller, port ):
    """Point the Client API service at `port`, grant the oracle access keys,
    and (re)start the server."""

    from hydrus.client import ClientAPI
    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientServices
    from hydrus.core import HydrusConstants as HC
    from hydrus.core import HydrusTags

    services = controller.services_manager.GetServices()

    new_services = []

    for service in services:

        if service.GetServiceKey() == CC.CLIENT_API_SERVICE_KEY:

            ( service_key, service_type, name, dictionary ) = service.ToTuple()

            dictionary[ 'port' ] = port
            dictionary[ 'allow_non_local_connections' ] = False
            dictionary[ 'use_https' ] = False

            service = ClientServices.GenerateService( service_key, service_type, name, dictionary )


        new_services.append( service )


    controller.WriteSynchronous( 'update_services', new_services )

    # the services manager refreshes via pubsub; wait until it has
    deadline = time.time() + 30

    while controller.services_manager.GetService( CC.CLIENT_API_SERVICE_KEY ).GetPort() != port:

        if time.time() > deadline:

            raise Exception( 'services manager never saw the new port' )


        time.sleep( 0.1 )


    manager = controller.client_api_manager

    if not manager.HasAccessKey( ORACLE_ACCESS_KEY ):

        manager.AddAccess( ClientAPI.APIPermissions( name = 'oracle', access_key = ORACLE_ACCESS_KEY, permits_everything = True ) )


    if not manager.HasAccessKey( ORACLE_RESTRICTED_ACCESS_KEY ):

        # search-only key whose searches are limited to files tagged "safe"
        tag_filter = HydrusTags.TagFilter()
        tag_filter.SetRule( '', HC.FILTER_BLACKLIST )
        tag_filter.SetRule( ':', HC.FILTER_BLACKLIST )
        tag_filter.SetRule( 'safe', HC.FILTER_WHITELIST )

        permissions = ClientAPI.APIPermissions(
            name = 'oracle restricted',
            access_key = ORACLE_RESTRICTED_ACCESS_KEY,
            permits_everything = False,
            basic_permissions = { ClientAPI.CLIENT_API_PERMISSION_SEARCH_FILES },
            search_tag_filter = tag_filter
        )

        manager.AddAccess( permissions )


    controller.RestartClientServerServices()


def run_client( db_dir, hook, port = None, network = False ):
    """Boot the reference client on `db_dir`, run `hook( Session )`, shut down.

    Network traffic is paused unless `network` (for recordings against a local
    test site; nothing else should ever be contacted).

    Returns whatever the hook returned. Re-raises the hook's exception after
    shutdown if it failed."""

    os.makedirs( db_dir, exist_ok = True )

    sys.argv = [ 'hydrus_client.py', '-d', db_dir, '--non_interactive_update' ]

    if not network:

        sys.argv.append( '--pause_network_traffic' )


    from hydrus import hydrus_client_boot
    from hydrus.client import ClientController

    outcome = {}

    original_boot = ClientController.Controller.THREADBootEverything

    # the reference picks a random member to stand for a duplicate group whose
    # king is outside the searched domain; pin that to the lowest file id (as
    # hydrus-rs picks) so recordings are reproducible
    from hydrus.client.db import ClientDBFilesDuplicatesStorage

    class LowestChoice( object ):

        @staticmethod
        def choice( seq ):

            return min( seq )



    ClientDBFilesDuplicatesStorage.random = LowestChoice

    def patched_boot( controller ):

        original_boot( controller )

        if not controller._is_booted:

            outcome[ 'error' ] = Exception( 'client failed to boot; see client log in the db dir' )

            return


        def work():

            try:

                # the similar-files search runs in the background and adds
                # potential pairs at unpredictable moments (e.g. after a
                # relationship change resets a file's search); recordings must
                # not depend on its timing
                for name in ( 'maintain_similar_files_duplicate_pairs_during_active', 'maintain_similar_files_duplicate_pairs_during_idle' ):
                    
                    controller.new_options.SetBoolean( name, False )
                    
                
                # let the gui finish its post-boot CallAfters before we start poking
                time.sleep( 1.0 )

                if port is not None:

                    enable_client_api( controller, port )


                session = Session( controller, port )

                if port is not None:

                    session.wait_for_api()


                outcome[ 'result' ] = hook( session )

            except BaseException as e:

                traceback.print_exc()
                outcome[ 'error' ] = e

            finally:

                os.kill( os.getpid(), signal.SIGTERM )



        threading.Thread( target = work, name = 'oracle hook', daemon = True ).start()


    ClientController.Controller.THREADBootEverything = patched_boot

    hydrus_client_boot.boot()

    # boot() returns once the gui has gone; the db connection closes a moment
    # later, at which point sqlite removes the WAL sidecars
    def sidecars():

        return [ name for name in os.listdir( db_dir ) if name.endswith( '-wal' ) or name.endswith( '-shm' ) ]


    deadline = time.time() + 120

    while sidecars():

        if time.time() > deadline:

            raise Exception( f'database did not close cleanly: {sidecars()}' )


        time.sleep( 0.1 )


    if 'error' in outcome:

        raise outcome[ 'error' ]


    return outcome.get( 'result' )


def run_in_subprocess( script_path, *args ):
    """Run an oracle script in a fresh interpreter (one client per process)."""

    env = dict( os.environ )
    env.setdefault( 'QT_QPA_PLATFORM', 'offscreen' )

    subprocess.run( [ sys.executable, script_path, *args ], check = True, env = env, cwd = REPO_ROOT )

