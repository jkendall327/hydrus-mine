#!/usr/bin/env python3
"""Record the reference's "Paste image!" of system:similar files > data.

In the running client, on the `basic` fixture, the real
`PanelPredicateSystemSimilarToData._Paste` runs with the clipboard scripted
(the controller's clipboard reads are replaced; the panel, its hashing and
its warnings run unchanged). For each case it records the two hash fields
after pasting and the warnings shown:

* an image file's path as clipboard text, and as a clipboard local path;
* a clipboard bitmap (a `QImage`) of the same pixels, preferred to a path
  also on the clipboard, and a translucent bitmap;
* the empty clipboard, text that is no file, and a file whose type has no
  perceptual hash;
* pasting twice (each hash kept once), then clear.

The images are in `fixtures/similar_data_paste/` (`pixels.png` is made here:
a 16 x 16 picture with structure); `bitmaps` holds each pasted bitmap's
pixels as RGBA hex, to hand to the port.

Usage: QT_QPA_PLATFORM=offscreen ~/pyenv/bin/python oracle/record_similar_data_paste.py
       (writes fixtures/similar_data_paste.json)
"""

import json
import sys
import tempfile
from pathlib import Path

HERE = Path( __file__ ).resolve().parent

sys.path.insert( 0, str( HERE ) )

IMAGES = HERE / 'fixtures' / 'similar_data_paste'


def make_images():

    from qtpy import QtGui as QG

    image = QG.QImage( 16, 16, QG.QImage.Format.Format_RGBA8888 )

    for y in range( 16 ):

        for x in range( 16 ):

            blue = 200 if ( x // 4 + y // 4 ) % 2 == 0 else 30

            image.setPixelColor( x, y, QG.QColor( x * 16, y * 16, blue, 255 ) )


    image.save( str( IMAGES / 'pixels.png' ) )

    translucent = QG.QImage( 16, 16, QG.QImage.Format.Format_RGBA8888 )

    for y in range( 16 ):

        for x in range( 16 ):

            blue = 200 if ( x // 4 + y // 4 ) % 2 == 0 else 30

            translucent.setPixelColor( x, y, QG.QColor( x * 16, y * 16, blue, 255 if x < 8 else 128 ) )


    return image, translucent


def rgba_hex( image ):

    from qtpy import QtGui as QG

    image = image.convertToFormat( QG.QImage.Format.Format_RGBA8888 )

    bits = image.constBits()

    return bytes( bits )[ : image.sizeInBytes() ].hex()


def record( session ):

    def work():

        from hydrus.core import HydrusExceptions
        from hydrus.client.gui import ClientGUIDialogsMessage as M
        from hydrus.client.gui.search import ClientGUIPredicatesSingle as S
        from hydrus.client.search import ClientSearchPredicate as P

        c = session.controller

        pixels, translucent = make_images()

        text_file = IMAGES / 'not an image.txt'

        text_file.write_text( 'plain text, which has no perceptual hash\n' )

        clipboard = { 'image' : None, 'paths' : [], 'text' : None }

        c.ClipboardHasImage = lambda: clipboard[ 'image' ] is not None
        c.GetClipboardImage = lambda: clipboard[ 'image' ]
        c.ClipboardHasLocalPaths = lambda: len( clipboard[ 'paths' ] ) > 0
        c.GetClipboardLocalPaths = lambda: list( clipboard[ 'paths' ] )

        def get_text():

            if clipboard[ 'text' ] is None:

                raise HydrusExceptions.DataMissing( 'no text' )


            return clipboard[ 'text' ]


        c.GetClipboardText = get_text

        warnings = []

        original_warning = M.ShowWarning

        M.ShowWarning = lambda owner, message: warnings.append( message )

        def fields( panel ):

            return { 'pixel' : panel._pixel_hashes.toPlainText(), 'perceptual' : panel._perceptual_hashes.toPlainText() }


        def run( name, image = None, paths = (), text = None, presses = 1 ):

            clipboard[ 'image' ] = image
            clipboard[ 'paths' ] = list( paths )
            clipboard[ 'text' ] = text

            warnings.clear()

            panel = S.PanelPredicateSystemSimilarToData( c.gui, P.Predicate( P.PREDICATE_TYPE_SYSTEM_SIMILAR_TO_DATA, ( (), (), 8 ) ) )

            for i in range( presses ):

                panel._Paste()


            case = { 'name' : name, 'fields' : fields( panel ), 'warnings' : list( warnings ) }

            panel._Clear()

            case[ 'cleared' ] = fields( panel )

            panel.deleteLater()

            return case


        out = { 'cases' : [], 'bitmaps' : { 'pixels' : rgba_hex( pixels ), 'translucent' : rgba_hex( translucent ) } }

        autores = HERE / 'fixtures' / 'auto_resolution'

        try:

            out[ 'cases' ].append( run( 'empty clipboard' ) )
            out[ 'cases' ].append( run( 'text that is no file', text = '/no/such/file.png' ) )
            out[ 'cases' ].append( run( 'text file path', text = str( text_file ) ) )
            out[ 'cases' ].append( run( 'png path as text', text = str( IMAGES / 'pixels.png' ) ) )
            out[ 'cases' ].append( run( 'png path as local path', paths = [ str( IMAGES / 'pixels.png' ) ] ) )
            out[ 'cases' ].append( run( 'larger png path', text = str( autores / 'p00_a.png' ) ) )
            out[ 'cases' ].append( run( 'jpeg path', text = str( autores / 'p00_f_exif.jpg' ) ) )
            out[ 'cases' ].append( run( 'gif path', text = str( autores / 'p00_h.gif' ) ) )
            out[ 'cases' ].append( run( 'bmp path', text = str( autores / 'p00_c.bmp' ) ) )
            out[ 'cases' ].append( run( 'bitmap', image = pixels ) )
            out[ 'cases' ].append( run( 'bitmap preferred to a path', image = pixels, text = str( autores / 'p00_a.png' ) ) )
            out[ 'cases' ].append( run( 'translucent bitmap', image = translucent ) )
            out[ 'cases' ].append( run( 'pasted twice', image = pixels, presses = 2 ) )

        finally:

            M.ShowWarning = original_warning


        return out

    return session.controller.CallBlockingToQt( session.controller.gui, work )


def main():

    import hydrus_driver

    if len( sys.argv ) > 1:

        import record_api

        Path( sys.argv[ 2 ] ).write_text( json.dumps( hydrus_driver.run_client( record_api.unpack_fixture( 'basic' ), record ) ) )

        return


    with tempfile.TemporaryDirectory() as folder:

        result = Path( folder ) / 'result.json'

        hydrus_driver.run_in_subprocess( str( Path( __file__ ).resolve() ), '--child', str( result ) )

        value = json.loads( result.read_text() )


    ( HERE / 'fixtures' / 'similar_data_paste.json' ).write_text( json.dumps( value, indent = 2 ) + '\n' )

    print( 'wrote similar_data_paste.json' )


if __name__ == '__main__':

    main()
