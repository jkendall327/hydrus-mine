#!/usr/bin/env python3
"""Record the import folder editor's per-service filename tagging list and
its example path.

The real `EditImportFolderPanel` is made over an import folder; its
"metadata import" list is driven through the real `_AddFilenameTaggingOptions`,
`_EditFilenameTaggingOptions` and delete, with the service chooser, the
tagging dialog and the warning replaced by scripted ones. Recorded after
each step: the list's rows and the warnings shown, and for each dialog
opened the example path it was given. The example path comes from
`_PathChanged`: the first of a folder's first 25 entries that is a file
whose content is a filetype hydrus imports. Folders are given with a text
file, a one-byte `.jpg`, a png, a subfolder and a gif (in that order of
names), and with more than 25 entries before the first importable file.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_import_folder_tagging.py
       (writes fixtures/import_folder_tagging.json)
"""

import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

IMAGES = os.path.join( HERE, 'fixtures', 'auto_resolution' )


def record( session ):
    
    controller = session.controller
    
    def qt():
        
        from qtpy import QtWidgets as QW
        
        from hydrus.core import HydrusConstants as HC
        from hydrus.client.gui import ClientGUIDialogsMessage
        from hydrus.client.gui import ClientGUIDialogsQuick
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        from hydrus.client.gui.importing import ClientGUIImportFolders as G
        from hydrus.client.importing import ClientImportLocal
        
        services = controller.services_manager.GetServices( HC.REAL_TAG_SERVICES )
        
        keys = [ service.GetServiceKey() for service in services ]
        
        names = [ service.GetName() for service in services ]
        
        work = tempfile.mkdtemp( prefix = 'hydrus_tagging_' )
        
        out = { 'services' : names, 'steps' : [], 'examples' : [] }
        
        # the folder: names sorted so that a listing in name order is the same
        # as any other
        def make_folder( files ):
            
            path = tempfile.mkdtemp( dir = work )
            
            for ( name, source ) in files:
                
                full = os.path.join( path, name )
                
                if source == 'dir':
                    
                    os.makedirs( full )
                    
                elif source == 'one byte':
                    
                    with open( full, 'wb' ) as f:
                        
                        f.write( b'x' )
                        
                    
                elif source == 'text':
                    
                    with open( full, 'w' ) as f:
                        
                        f.write( 'just words' )
                        
                    
                else:
                    
                    shutil.copy( os.path.join( IMAGES, source ), full )
                    
                
            
            return path
            
        
        folder_path = make_folder( [ ( 'a.txt', 'text' ), ( 'b.jpg', 'one byte' ), ( 'c.png', 'p03_a.png' ), ( 'd', 'dir' ), ( 'e.gif', 'p00_h.gif' ) ] )
        
        import_folder = ClientImportLocal.ImportFolder( 'tagging', path = folder_path )
        
        panel = G.EditImportFolderPanel( controller.gui, import_folder )
        
        script = { 'service' : None }
        warnings = []
        dialogs = []
        
        ClientGUIDialogsQuick.SelectServiceKey = lambda *args, **kwargs: script[ 'service' ]
        ClientGUIDialogsMessage.ShowWarning = lambda parent, message, *args, **kwargs: warnings.append( message )
        
        class FakeDialog( QW.QDialog ):
            
            def __init__( self, parent, title, *args, **kwargs ):
                
                super().__init__( parent )
                
                self._title = title
                
            
            def __enter__( self ):
                
                return self
                
            
            def __exit__( self, *args ):
                
                return False
                
            
            def SetPanel( self, p ):
                
                dialogs.append( { 'title' : self._title, 'example_path' : p._example_path_input.text(), 'service' : p._service_key.hex() } )
                
            
            def exec( self ):
                
                return QW.QDialog.DialogCode.Accepted
                
            
        
        real = T.DialogEdit
        
        T.DialogEdit = FakeDialog
        
        def rows():
            
            return [ list( panel._ConvertFilenameTaggingOptionsToDisplayTuple( data ) ) for data in panel._filename_tagging_options.GetData() ]
            
        
        def step( name, action ):
            
            del warnings[ : ]
            del dialogs[ : ]
            
            action()
            
            out[ 'steps' ].append( { 'name' : name, 'rows' : rows(), 'warnings' : list( warnings ), 'dialogs' : list( dialogs ) } )
            
        
        try:
            
            panel._PathChanged()
            
            out[ 'start' ] = rows()
            
            def add( index ):
                
                script[ 'service' ] = keys[ index ]
                
                panel._AddFilenameTaggingOptions()
                
            
            step( 'add the first service', lambda: add( 0 ) )
            step( 'add it again', lambda: add( 0 ) )
            
            if len( keys ) > 1:
                
                step( 'add the second service', lambda: add( 1 ) )
                
            
            def cancel_choice():
                
                script[ 'service' ] = None
                
                panel._AddFilenameTaggingOptions()
                
            
            step( 'cancel the service choice', cancel_choice )
            
            def edit_first():
                
                panel._filename_tagging_options.SelectDatas( [ panel._filename_tagging_options.GetData()[ 0 ] ], deselect_others = True )
                
                panel._EditFilenameTaggingOptions()
                
            
            step( 'edit the first', edit_first )
            
            def delete_first():
                
                panel._filename_tagging_options.SelectDatas( [ panel._filename_tagging_options.GetData()[ 0 ] ], deselect_others = True )
                
                panel._filename_tagging_options.DeleteSelected()
                
            
            step( 'delete the first', delete_first )
            
            # the example path, for folders of different contents
            for ( name, files ) in [
                ( 'a text file, a one-byte jpeg, a png, a folder and a gif', [ ( 'a.txt', 'text' ), ( 'b.jpg', 'one byte' ), ( 'c.png', 'p03_a.png' ), ( 'd', 'dir' ), ( 'e.gif', 'p00_h.gif' ) ] ),
                ( 'nothing importable', [ ( 'a.txt', 'text' ), ( 'b.jpg', 'one byte' ), ( 'd', 'dir' ) ] ),
                ( 'twenty-six texts before the png', [ ( f'{i:02d}.txt', 'text' ) for i in range( 26 ) ] + [ ( 'zz.png', 'p03_a.png' ) ] ),
                ( 'an empty folder', [] ),
            ]:
                
                path = make_folder( files )
                
                panel._path.SetPath( path )
                
                panel._PathChanged()
                
                example_paths = panel._sidecar_test_context_factory.GetExampleFilePaths()
                
                # the dialog opened over this folder is given the first of them
                del dialogs[ : ]
                
                script[ 'service' ] = keys[ 0 ]
                
                # (the first service's entry exists: edit it)
                if len( panel._filename_tagging_options.GetData() ) == 0:
                    
                    panel._AddFilenameTaggingOptions()
                    
                
                del dialogs[ : ]
                
                panel._filename_tagging_options.SelectDatas( [ panel._filename_tagging_options.GetData()[ 0 ] ], deselect_others = True )
                
                panel._EditFilenameTaggingOptions()
                
                out[ 'examples' ].append( { 'name' : name, 'listing' : sorted( os.listdir( path ) ), 'example_paths' : sorted( [ os.path.relpath( p, path ) for p in example_paths ] ), 'dialog_example' : ( os.path.relpath( dialogs[ -1 ][ 'example_path' ], path ) if dialogs[ -1 ][ 'example_path' ] else '' ) } )
                
            
        finally:
            
            T.DialogEdit = real
            
        
        panel.deleteLater()
        
        shutil.rmtree( work, ignore_errors = True )
        
        return out
        
    
    return controller.CallBlockingToQt( controller.gui, qt )
    

def main():
    
    if len( sys.argv ) > 1 and sys.argv[ 1 ] == '--child':
        
        import record_api
        
        output_path = sys.argv[ 2 ]
        
        result = hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record )
        
        with open( output_path, 'w' ) as f:
            
            json.dump( result, f, ensure_ascii = False )
            
        
        return
        
    
    with tempfile.TemporaryDirectory() as tmp:
        
        path = os.path.join( tmp, 'result.json' )
        
        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )
        
        with open( path ) as f:
            
            result = json.load( f )
            
        
    
    with open( os.path.join( HERE, 'fixtures', 'import_folder_tagging.json' ), 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote import_folder_tagging.json' )
    

if __name__ == '__main__':
    
    main()
