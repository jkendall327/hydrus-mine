#!/usr/bin/env python3
"""Record the reference's HTTP header manager and its header editor.

The real `EditNetworkContextCustomHeadersPanel` (network > data > manage http
headers) is made over a few headers (global and per domain, the three
approval states). Recorded:

- `"rows"`: the list's rows as it shows them, in order;
- `"add_defaults"`: what its "add" opens the editor with (the real `_Add`,
  its dialog replaced by one that only reads the editor and cancels), and
  the editor's approval choices in order;
- `"values"`: the editor's `GetValue` for typed keys and values (trimmed;
  newlines vetoed with the reference's words; spaces and other odd keys
  taken), with the network context it was given.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_headers_fields.py
       (writes fixtures/headers_fields.json)
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

VALUES = [
    ( '  X-Test  ', '  spaced value  ' ),
    ( 'X Token', 'v' ),
    ( 'X-Test\nMore', 'v' ),
    ( 'X-Test', 'a\nb' ),
    ( '', '' ),
    ( 'Authorization', 'Basic dXNlcm5hbWU6cGFzc3dvcmQ=' ),
    ( 'X-Ünicode', 'välue' ),
]


def record( session ):
    
    controller = session.controller
    
    def qt():
        
        from qtpy import QtWidgets as QW
        
        from hydrus.client import ClientConstants as CC
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        from hydrus.client.gui.networking import ClientGUINetwork as G
        from hydrus.client.networking import ClientNetworkingContexts as C
        from hydrus.client.networking import ClientNetworkingDomain as D
        from hydrus.core import HydrusExceptions
        
        glob = C.GLOBAL_NETWORK_CONTEXT
        example = C.NetworkContext( CC.NETWORK_CONTEXT_DOMAIN, 'example.com' )
        other = C.NetworkContext( CC.NETWORK_CONTEXT_DOMAIN, 'other.example' )
        
        headers = {
            glob : { 'User-Agent' : ( 'Mozilla/5.0 (compatible; Hydrus Client)', D.VALID_APPROVED, 'This is the default User-Agent identifier.' ), 'X-Test' : ( 'global-value', D.VALID_UNKNOWN, '' ) },
            example : { 'Authorization' : ( 'token', D.VALID_DENIED, 'login' ), 'Accept' : ( 'image/png', D.VALID_APPROVED, '' ) },
            other : { 'Cookie' : ( 'a=b', D.VALID_UNKNOWN, 'needs a look' ) },
        }
        
        panel = G.EditNetworkContextCustomHeadersPanel( controller.gui, headers )
        
        out = {}
        
        out[ 'rows' ] = [ list( panel._ConvertDataToDisplayTuple( data ) ) for data in panel._list_ctrl.GetData() ]
        
        # the real "add", its dialog replaced
        seen = {}
        
        class FakeDialog( QW.QDialog ):
            
            def __init__( self, parent, title, *args, **kwargs ):
                
                super().__init__( parent )
                
                seen[ 'title' ] = title
                
            
            def __enter__( self ):
                
                return self
                
            
            def __exit__( self, *args ):
                
                return False
                
            
            def SetPanel( self, p ):
                
                seen[ 'panel' ] = p
                
                seen[ 'fields' ] = { 'context' : p._network_context.GetValue().ToString(), 'key' : p._key.text(), 'value' : p._value.text(), 'approved' : p._approved.GetValue(), 'reason' : p._reason.text(), 'reason_placeholder' : p._reason.placeholderText() }
                
                seen[ 'approvals' ] = [ [ p._approved.itemText( i ), p._approved.itemData( i ) ] for i in range( p._approved.count() ) ]
                
            
            def exec( self ):
                
                return QW.QDialog.DialogCode.Rejected
                
            
                
            
        
        real = T.DialogEdit
        
        T.DialogEdit = FakeDialog
        
        try:
            
            panel._Add()
            
        finally:
            
            T.DialogEdit = real
            
        
        out[ 'add_title' ] = seen[ 'title' ]
        out[ 'add_defaults' ] = seen[ 'fields' ]
        out[ 'approvals' ] = seen[ 'approvals' ]
        
        editor = seen[ 'panel' ]
        
        out[ 'values' ] = []
        
        for ( key, value ) in VALUES:
            
            editor._key.setText( key )
            editor._value.setText( value )
            editor._reason.setText( ' a reason ' )
            
            entry = { 'key' : key, 'value' : value }
            
            try:
                
                ( network_context, k, v, approved, reason ) = editor.GetValue()
                
                entry[ 'result' ] = { 'context' : network_context.ToString(), 'key' : k, 'value' : v, 'approved' : approved, 'reason' : reason }
                
            except HydrusExceptions.VetoException as e:
                
                entry[ 'veto' ] = str( e )
                
            
            out[ 'values' ].append( entry )
            
        
        panel.deleteLater()
        
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
            
        
    
    with open( os.path.join( HERE, 'fixtures', 'headers_fields.json' ), 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote headers_fields.json' )
    

if __name__ == '__main__':
    
    main()
