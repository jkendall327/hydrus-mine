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
- `{"noneable_text": text or null, "none_phrase": text}` for text that may
  be none (`NoneableTextCtrl`);
- `{"duration": seconds, "units": [...], "min": seconds}` for a time delta
  (`TimeDeltaWidget`), its units those it shows of "days", "hours",
  "minutes", "seconds" and "milliseconds"; and for one behind a button
  (`TimeDeltaButton`) the same with the button's `"button_text"`;
- `{"velocity": [number, seconds], "number_min": n, "number_max": n,
  "per": text, "units": [...], "min": seconds}` for a number per time
  (`VelocityCtrl`), `"per"` the text between them;
- `{"tabs": [{"tab": name, "items": [...]}]}` for tabs;
- `{"widget": class name}`, with its `"value"` if it has one (`GetValue`)
  or the `"items"` laid out in it, for anything else.

A control explicitly hidden has `"hidden": true`, and one with a tooltip
its `"tooltip"`. `"search"` is the options search box's placeholder and
what it offers (`"suggestions"`, in order: each label, box title and
dropdown's text as "text (page)"). The recording's `"facts"` are the options the driver sets
as it boots the client that hydrus-rs can't read from the fixture (the
similar-files search is switched off, so it can't add pairs mid-recording). hydrus-rs's options window is checked against this: its
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


def units( w ):

    names = [ 'days', 'hours', 'minutes', 'seconds', 'milliseconds' ]

    return [ name for name in names if getattr( w, '_show_' + name, False ) ]


def describe( w ):

    from qtpy import QtWidgets as QW

    from hydrus.client.gui.metadata import ClientGUITime
    from hydrus.client.gui.widgets import ClientGUICommon

    if isinstance( w, ClientGUITime.VelocityCtrl ):

        per = [ c.text() for c in w.findChildren( QW.QLabel ) if c.parentWidget() is w ]

        ( number, seconds ) = w.GetValue()

        out = {
            'velocity' : [ number, seconds ],
            'number_min' : w._num.minimum(),
            'number_max' : w._num.maximum(),
            'per' : per[ 0 ] if len( per ) > 0 else '',
            'units' : units( w._times ),
            'min' : w._times._min,
        }

    elif isinstance( w, ClientGUITime.TimeDeltaWidget ):

        out = { 'duration' : w.GetValue(), 'units' : units( w ), 'min' : w._min }

    elif isinstance( w, ClientGUITime.TimeDeltaButton ):

        out = { 'duration' : w.GetValue(), 'units' : units( w ), 'min' : w._min, 'button_text' : w.text() }

    elif isinstance( w, ClientGUICommon.NoneableTextCtrl ):

        out = { 'noneable_text' : w.GetValue(), 'none_phrase' : w._checkbox.text() }

    elif isinstance( w, ClientGUICommon.StaticBox ):

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

        search = {
            'placeholder' : panel._options_search.placeholderText(),
            'max_visible' : panel._completer.maxVisibleItems(),
            'suggestions' : panel._completer.model().stringList(),
        }

        panel.deleteLater()

        return { 'opens_on' : current, 'search' : search, 'pages' : pages }


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



    result = {
        'facts' : {
            'maintain_similar_files_duplicate_pairs_during_active' : False,
            'maintain_similar_files_duplicate_pairs_during_idle' : False,
        },
        **result,
    }

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}: {len( result[ "pages" ] )} pages' )


if __name__ == '__main__':

    main()
