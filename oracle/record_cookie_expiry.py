#!/usr/bin/env python3
"""Record the reference's cookie editor's expiry ("set expires as a delta
from now") and its newline checks.

The real `EditCookiePanel` is made for a cookie with no expiry, one that
expires in two hours and one that expired an hour ago (the clock held at a
fixed time); its "set expires as a delta from now" is pressed with its
dialog replaced by one that records the time delta control it opens with
(its default, minimum and maximum) and then sets a given delta. Recorded
per case: the control's default, and for each given delta the expiry and the
two texts the panel then shows. And `GetValue` for fields with and without
newlines.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_cookie_expiry.py
       (writes fixtures/cookie_expiry.json)
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

import hydrus_driver

NOW = 1_800_000_000

DELTAS = [ 1, 1199, 1200, 3600, 86400 * 30, 86400 * 366 * 200, 86400 * 366 * 200 + 5 ]


def record( session ):
    
    controller = session.controller
    
    def qt():
        
        from qtpy import QtWidgets as QW
        
        from hydrus.core import HydrusExceptions
        from hydrus.core import HydrusTime
        from hydrus.client.gui import ClientGUITopLevelWindowsPanels as T
        from hydrus.client.gui.networking import ClientGUINetwork as G
        
        HydrusTime.GetNow = lambda: NOW
        
        script = { 'delta' : None }
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
                
                self._panel = p
                
                control = p._control
                
                seen[ 'default' ] = control.GetValue()
                
            
            def exec( self ):
                
                self._panel._control.SetValue( script[ 'delta' ] )
                
                seen[ 'chosen' ] = self._panel._control.GetValue()
                
                return QW.QDialog.DialogCode.Accepted
                
            
        
        real = T.DialogEdit
        
        T.DialogEdit = FakeDialog
        
        out = { 'now' : NOW, 'deltas' : DELTAS, 'cases' : [], 'values' : [] }
        
        try:
            
            for ( name, expires ) in [ ( 'no expiry', None ), ( 'expires in two hours', NOW + 7200 ), ( 'expired an hour ago', NOW - 3600 ) ]:
                
                panel = G.EditCookiePanel( controller.gui, 'sid', 'v', 'example.com', '/', expires )
                
                case = { 'name' : name, 'expires': expires, 'start_text' : panel._expires_st.text(), 'start_utc' : panel._expires_st_utc.text(), 'button' : panel._set_expires_button.text(), 'sets' : [] }
                
                for delta in DELTAS:
                    
                    script[ 'delta' ] = delta
                    
                    panel._SetExpires()
                    
                    case.setdefault( 'default', seen[ 'default' ] )
                    
                    case[ 'sets' ].append( { 'delta' : delta, 'control_value' : seen[ 'chosen' ], 'expires' : panel._expires, 'text' : panel._expires_st.text(), 'utc' : panel._expires_st_utc.text() } )
                    
                    # (each from the start, as a fresh panel would)
                    panel._expires = expires
                    
                
                case[ 'dialog_title' ] = seen[ 'title' ]
                
                out[ 'cases' ].append( case )
                
                panel.deleteLater()
                
            
        finally:
            
            T.DialogEdit = real
            
        
        for ( name, value, domain, path ) in [ ( ' sid ', ' v ', ' example.com ', ' / ' ), ( 'a\nb', 'v', 'example.com', '/' ), ( 'sid', 'a\nb', 'example.com', '/' ), ( 'sid', 'v', 'a\nb', '/' ), ( 'sid', 'v', 'example.com', 'a\nb' ), ( '', 'v', 'example.com', '/' ) ]:
            
            panel = G.EditCookiePanel( controller.gui, name, value, domain, path, None )
            
            entry = { 'fields' : [ name, value, domain, path ] }
            
            try:
                
                entry[ 'result' ] = list( panel.GetValue() )
                
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
            
        
    
    with open( os.path.join( HERE, 'fixtures', 'cookie_expiry.json' ), 'w' ) as f:
        
        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )
        
    
    print( 'wrote cookie_expiry.json' )
    

if __name__ == '__main__':
    
    main()
