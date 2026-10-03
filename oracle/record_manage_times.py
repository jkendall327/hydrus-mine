#!/usr/bin/env python3
"""Record the reference's "manage times" panel (`EditFileTimestampsPanel`).

In the running client, on the `basic` fixture, with the time held still
(`NOW`) and the time zone UTC, for each case the real panel is made over
stand-in files (each with the times the case gives: `modified`,
`archived` (an inboxed file has none), `viewed` (the media viewer's last
viewed) and `preview` (the preview's), `domains` (a web domain's modified
time each), and file services' times `services` (each `[service name,
timestamp type, ms]`)), and driven step by step:

* `["edit_main", which, ms, step]`: one of the four times' buttons given
  a new time from its edit dialog (`ms`, with a cascading `step`), as the
  dialog returns it (the button's value with the new time and step);
* `["add_domain", domain, ms]`: "add" a web domain time (the domain
  typed, the time set);
* `["edit_domains", [domains], ms, step, answer]`: those domains
  selected and edited (answering "all files" or "only edit existing
  values" if asked);
* `["delete_domains", [domains], answer]`: those selected and deleted;
* `["edit_services", [[name, type]], ms, step]`: those file service rows
  selected and edited;
* `["copy", types]`: copy (all times, or only those types), recording what
  was copied;
* `["paste", text]` (`text` may be `"COPIED"`, the last copied text);
* `["ok", answer]`: "apply": what `UserIsOKToOK` asks (answered so), and
  the content updates it writes (`set` or `delete`, the files' hashes in
  order, the timestamp data's serialisable tuple), and the file modified
  update (the files, time and step, or null).

After each step `state` has the grid's rows (label, button text, enabled),
the file modified warning (or null if hidden), the web domain list (each
row's domain and time text, in the list's order), the file services list
(service name, type, time text), and whether copy is shown. Each step's
`said` has what was asked, warned or noted, and `copied` what was copied.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_manage_times.py
       (writes fixtures/manage_times.json)
"""

import json
import os
import sys
import time

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'manage_times.json' )

os.environ[ 'TZ' ] = 'UTC'
time.tzset()

NOW = 1700000000

DAY = 86400 * 1000

T = NOW * 1000

CASES = [
    { 'name' : 'one file', 'files' : [
        { 'hash' : 'aa', 'modified' : T - 400 * DAY, 'archived' : T - 3 * DAY, 'viewed' : T - 2 * DAY + 1234,
          'domains' : { 'a.com' : T - 30 * DAY, 'b.com' : T - 20 * DAY },
          'services' : [ [ 'my files', 3, T - 10 * DAY ], [ 'hydrus local file storage', 3, T - 10 * DAY ], [ 'trash', 4, T - 5 * DAY ], [ 'trash', 7, T - 9 * DAY ] ] },
    ], 'steps' : [
        [ 'copy', None ],
        [ 'copy', [ 0 ] ],
        [ 'edit_main', 'modified', T - 100 * DAY, 0 ],
        [ 'edit_main', 'archived', T - DAY, 0 ],
        [ 'add_domain', 'c.com', T - 7 * DAY ],
        [ 'add_domain', 'a.com', T - 7 * DAY ],
        [ 'edit_domains', [ 'b.com' ], T - 15 * DAY, 0, None ],
        [ 'delete_domains', [ 'a.com' ], False ],
        [ 'delete_domains', [ 'a.com' ], True ],
        [ 'add_domain', 'd.com', T - 6 * DAY ],
        [ 'delete_domains', [ 'd.com' ], True ],
        [ 'edit_services', [ [ 'my files', 3 ] ], T - 50 * DAY, 0 ],
        [ 'paste', 'COPIED' ],
        [ 'paste', 'not json' ],
        [ 'paste', '[26, 1, [[121, 2, [0, "a.com", 1000]], [121, 2, [5, null, 2000]], [121, 2, [6, 1, 3000]], [121, 2, [6, 2, 4000]]]]' ],
        [ 'ok', True ],
    ] },
    { 'name' : 'three files', 'files' : [
        { 'hash' : 'aa', 'modified' : T - 400 * DAY, 'archived' : T - 3 * DAY, 'viewed' : T - 2 * DAY,
          'domains' : { 'a.com' : T - 30 * DAY },
          'services' : [ [ 'my files', 3, T - 10 * DAY ] ] },
        { 'hash' : 'bb', 'modified' : T - 300 * DAY, 'archived' : T - 4 * DAY,
          'domains' : { 'a.com' : T - 30 * DAY, 'b.com' : T - 20 * DAY },
          'services' : [ [ 'my files', 3, T - 11 * DAY ] ] },
        { 'hash' : 'cc', 'modified' : T - 300 * DAY, 'inbox' : True,
          'services' : [ [ 'my files', 3, T - 12 * DAY ] ] },
    ], 'steps' : [
        [ 'copy', None ],
        [ 'edit_main', 'modified', T - 500 * DAY, 1000 ],
        [ 'edit_domains', [ 'a.com' ], T - 40 * DAY, 7, False ],
        [ 'edit_domains', [ 'b.com' ], T - 25 * DAY, 0, True ],
        [ 'edit_services', [ [ 'my files', 3 ] ], T - 60 * DAY, -5 ],
        [ 'paste', '[26, 1, [[121, 2, [0, "a.com", 1000]]]]' ],
        [ 'ok', True ],
    ] },
    { 'name' : 'unknown modified time', 'files' : [
        { 'hash' : 'aa', 'inbox' : True },
        { 'hash' : 'bb', 'inbox' : True },
    ], 'steps' : [
        [ 'add_domain', 'z.org', T - DAY ],
        [ 'edit_main', 'modified', T - DAY, 0 ],
        [ 'ok', True ],
    ] },
    { 'name' : 'old modified time', 'files' : [
        { 'hash' : 'aa', 'modified' : T - 400 * DAY },
    ], 'steps' : [
        [ 'edit_main', 'modified', 1000, 0 ],
        [ 'ok', True ],
    ] },
    { 'name' : 'many files', 'files' : [
        { 'hash' : '%04x' % i, 'modified' : T - ( 400 + i ) * DAY } for i in range( 101 )
    ], 'steps' : [
        [ 'edit_main', 'modified', T - DAY, 0 ],
        [ 'ok', False ],
        [ 'ok', True ],
    ] },
]


def record( session ):

    controller = session.controller
    gui = controller.gui

    from hydrus.core import HydrusTime

    HydrusTime.GetNow = lambda: NOW

    def f():

        from qtpy import QtWidgets as QW

        from hydrus.core import HydrusConstants as HC
        from hydrus.client import ClientConstants as CC
        from hydrus.client import ClientTime
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui import ClientGUIFunctions
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels
        from hydrus.client.gui.metadata import ClientGUIEditTimestamps as M
        from hydrus.client.gui.metadata import ClientGUITime
        from hydrus.client.gui.panels import ClientGUIScrolledPanels
        from hydrus.client.media import ClientMediaManagers

        services = { s.GetName() : s.GetServiceKey() for s in controller.services_manager.GetServices() }

        said = []
        clipboard = []
        answers = []
        pasted = []
        edits = []

        real_pub = controller.pub

        def pub( topic, *args, **kwargs ):

            if topic == 'clipboard':

                clipboard.append( args[ 1 ] )

                return


            real_pub( topic, *args, **kwargs )


        controller.pub = pub
        controller.GetClipboardText = lambda: pasted[ -1 ]

        def yes_no( win, message, **kwargs ):

            said.append( { 'asked' : message, 'yes' : kwargs.get( 'yes_label', 'yes' ), 'no' : kwargs.get( 'no_label', 'no' ) } )

            answer = answers.pop( 0 ) if len( answers ) > 0 else True

            return QW.QDialog.DialogCode.Accepted if answer else QW.QDialog.DialogCode.Rejected


        ClientGUIDialogsQuick.GetYesNo = yes_no
        ClientGUIDialogsQuick.EnterText = lambda win, message, **kwargs: ( said.append( { 'entered' : message } ), answers.pop( 0 ) )[ 1 ]
        ClientGUIDialogsMessage.ShowWarning = lambda win, message: said.append( { 'warning' : message } )
        ClientGUIDialogsMessage.ShowCritical = lambda win, title, message: said.append( { 'critical' : title, 'message' : message } )
        ClientGUIFunctions.ShowMicroNotification = lambda win, text: said.append( { 'notice' : text } )

        class Dialog:

            # (the edit dialog, accepted)
            def __init__( self, *args, **kwargs ):

                pass


            def __enter__( self ):

                return self


            def __exit__( self, *args ):

                return False


            def SetPanel( self, panel ):

                pass


            def exec( self ):

                return QW.QDialog.DialogCode.Accepted



        class Panel:

            def __init__( self, *args, **kwargs ):

                pass


            def SetControl( self, control, **kwargs ):

                pass



        class Ctrl:

            # (the date-time editor: given the range, it returns it with
            # the edit's new time and step)
            def __init__( self, *args, **kwargs ):

                self._original = None


            def SetValue( self, value ):

                if self._original is None:

                    self._original = value


                said.append( { 'editing' : value.ToString() } )


            def GetValue( self ):

                ( ms, step ) = edits[ -1 ]

                value = self._original.DuplicateWithNewTimestampMS( ms )

                value.SetStepMS( step )

                return value


            def HasChanges( self ):

                return self.GetValue() != self._original



        # ("now", for a new web domain time, held still)
        import types

        from qtpy import QtCore as QC

        held = types.SimpleNamespace( **{ k : getattr( QC, k ) for k in dir( QC ) if not k.startswith( '__' ) } )

        class HeldDateTime:

            currentDateTime = staticmethod( lambda: QC.QDateTime.fromMSecsSinceEpoch( NOW * 1000, QC.QTimeZone.systemTimeZone() ) )


        held.QDateTime = HeldDateTime
        M.QC = held

        ClientGUITopLevelWindowsPanels.DialogEdit = Dialog
        ClientGUIScrolledPanels.EditSingleCtrlPanel = Panel
        ClientGUITime.DateTimesCtrl = Ctrl

        class Media:

            # (a stand-in file: its hash, inbox and times)
            def __init__( self, spec ):

                self._hash = bytes.fromhex( spec[ 'hash' ] )
                self._inbox = spec.get( 'inbox', False )

                times = ClientMediaManagers.TimesManager()

                if 'modified' in spec:

                    times.SetFileModifiedTimestampMS( spec[ 'modified' ] )


                if 'archived' in spec:

                    times.SetArchivedTimestampMS( spec[ 'archived' ] )


                if 'viewed' in spec:

                    times.SetLastViewedTimestampMS( CC.CANVAS_MEDIA_VIEWER, spec[ 'viewed' ] )


                if 'preview' in spec:

                    times.SetLastViewedTimestampMS( CC.CANVAS_PREVIEW, spec[ 'preview' ] )


                for ( domain, ms ) in spec.get( 'domains', {} ).items():

                    times.SetDomainModifiedTimestampMS( domain, ms )


                for ( name, timestamp_type, ms ) in spec.get( 'services', [] ):

                    key = services[ name ]

                    if timestamp_type == HC.TIMESTAMP_TYPE_IMPORTED:

                        times.SetImportedTimestampMS( key, ms )

                    elif timestamp_type == HC.TIMESTAMP_TYPE_DELETED:

                        times.SetDeletedTimestampMS( key, ms )

                    else:

                        times.SetPreviouslyImportedTimestampMS( key, ms )



                self._times = times


            def GetHash( self ):

                return self._hash


            def GetHashes( self ):

                return { self._hash }


            def GetTimesManager( self ):

                return self._times


            def HasInbox( self ):

                return self._inbox



        def select( ctrl, datas ):

            ctrl.clearSelection()

            ctrl.SelectDatas( datas )


        out = []

        for case in CASES:

            medias = [ Media( spec ) for spec in case[ 'files' ] ]

            panel = M.EditFileTimestampsPanel( gui, medias )

            # (shown, as the reference's paste only sets times shown)
            panel.show()

            buttons = {
                'modified' : ( 'file modified time: ', panel._file_modified_time ),
                'archived' : ( 'archived time: ', panel._archived_time ),
                'viewed' : ( 'last viewed in media viewer: ', panel._last_viewed_media_viewer_time ),
                'preview' : ( 'last viewed in preview viewer: ', panel._last_viewed_preview_viewer_time ),
            }

            def state():

                rows = []

                for ( key, ( label, button ) ) in buttons.items():

                    if key != 'modified' and not button.isVisibleTo( panel ):

                        continue


                    rows.append( [ label, button._datetimes_button.text(), button.isEnabled() ] )


                warning = panel._file_modified_time_warning_st

                domains = [ list( panel._ConvertDomainToDomainModifiedDisplayTuple( d ) ) for d in panel._domain_modified_list_ctrl.GetData() ]
                file_services = [ list( panel._ConvertDataRowToFileServiceDisplayTuple( r ) ) for r in panel._file_services_list_ctrl.GetData() ]

                return {
                    'rows' : rows,
                    'warning' : warning.text() if warning.isVisibleTo( panel ) else None,
                    'domains' : domains,
                    'file_services' : file_services,
                    'copy_shown' : not panel._copy_button.isHidden(),
                }


            steps = [ { 'state' : state() } ]
            last_copied = None

            for step in case[ 'steps' ]:

                del said[:]
                del clipboard[:]
                del answers[:]

                kind = step[ 0 ]
                written = None

                if kind == 'edit_main':

                    ( label, button ) = buttons[ step[ 1 ] ]

                    value = button.GetValue().DuplicateWithNewTimestampMS( step[ 2 ] )

                    value.SetStepMS( step[ 3 ] )

                    button.SetValue( value, from_user = True )

                elif kind == 'add_domain':

                    answers.append( step[ 1 ] )

                    edits.append( ( step[ 2 ], 0 ) )

                    panel._AddDomainModifiedTimestamp()

                elif kind == 'edit_domains':

                    select( panel._domain_modified_list_ctrl, step[ 1 ] )

                    edits.append( ( step[ 2 ], step[ 3 ] ) )

                    if step[ 4 ] is not None:

                        answers.append( step[ 4 ] )


                    panel._EditDomainModifiedTimestamp()

                elif kind == 'delete_domains':

                    select( panel._domain_modified_list_ctrl, step[ 1 ] )

                    answers.append( step[ 2 ] )

                    panel._domain_modified_list_ctrl.ProcessDeleteAction()

                elif kind == 'edit_services':

                    rows = [ ( services[ name ], timestamp_type ) for ( name, timestamp_type ) in step[ 1 ] ]

                    select( panel._file_services_list_ctrl, rows )

                    edits.append( ( step[ 2 ], step[ 3 ] ) )

                    panel._EditFileServiceTimestamp()

                elif kind == 'copy':

                    if step[ 1 ] is None:

                        panel._Copy()

                    else:

                        panel._Copy( allowed_timestamp_types = tuple( step[ 1 ] ) )


                    if len( clipboard ) > 0:

                        last_copied = clipboard[ -1 ]


                elif kind == 'paste':

                    pasted.append( last_copied if step[ 1 ] == 'COPIED' else step[ 1 ] )

                    panel._Paste()

                elif kind == 'ok':

                    answers.append( step[ 1 ] )

                    ok = panel.UserIsOKToOK()

                    updates = []

                    for ( service_key, content_updates ) in panel.GetContentUpdatePackage().IterateContentUpdates():

                        for cu in content_updates:

                            ( hashes, timestamp_data ) = cu.GetRow()

                            updates.append( [ 'set' if cu.GetAction() == HC.CONTENT_UPDATE_SET else 'delete', [ h.hex() for h in hashes ], timestamp_data.GetSerialisableTuple() ] )



                    modified = panel.GetFileModifiedUpdateData()

                    if modified is not None:

                        ( hashes, ms, step_ms ) = modified

                        modified = [ [ h.hex() for h in hashes ], ms, step_ms ]


                    written = { 'ok' : ok, 'updates' : updates, 'file_modified' : modified }


                entry = { 'do' : step, 'said' : list( said ), 'copied' : list( clipboard ), 'state' : state() }

                if written is not None:

                    entry[ 'written' ] = written


                steps.append( entry )


            panel.hide()
            panel.deleteLater()

            out.append( { 'name' : case[ 'name' ], 'files' : case[ 'files' ], 'steps' : steps } )


        return {
            'now' : NOW,
            'services' : { name : key.hex() for ( name, key ) in services.items() if name in ( 'my files', 'hydrus local file storage', 'trash' ) },
            'cases' : out,
        }


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

        path = os.path.join( work, 'manage_times.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "cases" ] )} cases' )


if __name__ == '__main__':

    main()
