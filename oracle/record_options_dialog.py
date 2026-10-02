#!/usr/bin/env python3
"""Record the reference's options dialog (file > options).

In the running client, on the `basic` fixture, the options dialog's panel
(`ManageOptionsPanel`) is made as file > options makes it, and recorded:
its pages' names in the order listed (sorted, "advanced" last), the page
it opens on, and each page's controls in the order they are laid out:

- `{"box": title, "items": [...]}` for a titled box (`StaticBox`);
- `{"label": text}` for text, such as a grid row's label before its control;
- `{"check": text, "value": bool}` for a checkbox;
- `{"int": value, "min": n, "max": n}` (and `"suffix"`), `{"float": ...}`
  likewise, for a number;
- `{"noneable": value or null, "none_phrase": text, "min": n, "max": n}`
  (and `"unit"`) for a number that may be "none" (`NoneableSpinCtrl`);
- `{"choice": current text, "items": [texts]}` for a dropdown;
- `{"text": text}` for a text box, `{"button": text}` for a button;
- `{"tabs": [{"tab": name, "items": [...]}]}` for tabs;
- `{"widget": class name}`, with its `"value"` if it has one (`GetValue`)
  or the `"items"` laid out in it, for anything else.

A control explicitly hidden has `"hidden": true`, and one with a tooltip
its `"tooltip"`. hydrus-rs's options window is checked against this: its
pages, their order and labels, and the values it shows for the fixture's
options.

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_options_dialog.py
       (writes fixtures/options_dialog.json)
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'options_dialog.json' )


def jsonable( value ):

    try:

        json.dumps( value )

        return value

    except ( TypeError, ValueError ):

        if isinstance( value, ( tuple, list ) ):

            return [ jsonable( v ) for v in value ]


        return repr( value )



def walk( layout ):

    from qtpy import QtWidgets as QW

    out = []

    if layout is None:

        return out


    for i in range( layout.count() ):

        item = layout.itemAt( i )

        if item.widget() is not None:

            out.append( describe( item.widget() ) )

        elif item.layout() is not None:

            out.extend( walk( item.layout() ) )



    return out


def describe( w ):

    from qtpy import QtWidgets as QW

    from hydrus.client.gui.widgets import ClientGUICommon

    if isinstance( w, ClientGUICommon.StaticBox ):

        out = { 'box' : w._title_st.text(), 'items' : walk( w._sizer ) }

    elif isinstance( w, ClientGUICommon.NoneableSpinCtrl ):

        out = {
            'noneable' : w.GetValue(),
            'none_phrase' : w._checkbox.text(),
            'min' : w._number_value.minimum(),
            'max' : w._number_value.maximum(),
        }

        if w._unit is not None:

            out[ 'unit' ] = w._unit


    elif isinstance( w, QW.QCheckBox ):

        out = { 'check' : w.text(), 'value' : w.isChecked() }

    elif isinstance( w, QW.QLabel ):

        out = { 'label' : w.text() }

    elif isinstance( w, QW.QSpinBox ):

        out = { 'int' : w.value(), 'min' : w.minimum(), 'max' : w.maximum() }

        if w.suffix():

            out[ 'suffix' ] = w.suffix()


    elif isinstance( w, QW.QDoubleSpinBox ):

        out = { 'float' : w.value(), 'min' : w.minimum(), 'max' : w.maximum() }

        if w.suffix():

            out[ 'suffix' ] = w.suffix()


    elif isinstance( w, QW.QComboBox ):

        out = { 'choice' : w.currentText(), 'items' : [ w.itemText( i ) for i in range( w.count() ) ] }

    elif isinstance( w, QW.QLineEdit ):

        out = { 'text' : w.text() }

    elif isinstance( w, QW.QAbstractButton ):

        out = { 'button' : w.text() }

    elif isinstance( w, QW.QTabWidget ):

        out = { 'tabs' : [ { 'tab' : w.tabText( i ), 'items' : walk( w.widget( i ).layout() ) } for i in range( w.count() ) ] }

    else:

        out = { 'widget' : type( w ).__name__ }

        get_value = getattr( w, 'GetValue', None )

        if callable( get_value ):

            try:

                out[ 'value' ] = jsonable( get_value() )

            except Exception as e:

                out[ 'value' ] = f'(GetValue failed: {e})'


        else:

            items = walk( w.layout() )

            if len( items ) > 0:

                out[ 'items' ] = items




    if w.isHidden():

        out[ 'hidden' ] = True


    if w.toolTip():

        out[ 'tooltip' ] = w.toolTip()


    return out


def record( session ):

    controller = session.controller
    gui = controller.gui

    def f():

        from hydrus.client.gui.panels.options import ClientGUIManageOptionsPanel

        panel = ClientGUIManageOptionsPanel.ManageOptionsPanel( gui )

        book = panel._listbook

        pages = []

        for i in range( book.count() ):

            pages.append( { 'page' : book.tabText( i ), 'items' : walk( book.widget( i ).layout() ) } )


        current = book.tabText( book.GetCurrentPageIndex() )

        panel.deleteLater()

        return { 'opens_on' : current, 'pages' : pages }


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


    import hydrus_driver

    with tempfile.TemporaryDirectory() as work:

        path = os.path.join( work, 'options.json' )

        hydrus_driver.run_in_subprocess( os.path.abspath( __file__ ), '--child', path )

        with open( path ) as f:

            result = json.load( f )



    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "pages" ] )} pages' )


if __name__ == '__main__':

    main()
