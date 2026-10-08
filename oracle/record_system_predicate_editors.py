#!/usr/bin/env python3
"""Record the reference's system predicate editors.

In the running client, on the `basic` fixture, for a search page on "my
files" with nothing typed:

* `offered`: the system predicates the autocomplete lists
  (`file_system_predicates`), in its order, each by its text (with its
  count) and whether choosing it opens an editor (it has no value yet);
* `editors`: for each that opens one, its "input predicate" dialog
  (`FleshOutPredicatePanel`): its pages (by their tab names, one unnamed
  page if there is no notebook), and on each page its ready-made buttons
  (their labels and the predicates each adds) and its editable panels:
  each panel's class, its widgets in order (labels, choices with the one
  chosen, numbers, text, tick boxes) and the predicates it adds as it
  opens, with its defaults; and what it adds with each of its widgets
  changed in turn (`changes`);
* `scenarios`: some panels with several widgets set in turn, and what
  they add then;
* `stored`: every predicate the panels made, by its text, as the reference
  stores it (with the client's `services`).

Usage: QT_QPA_PLATFORM=offscreen python oracle/record_system_predicate_editors.py
       (writes fixtures/system_predicate_editors.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, HERE )

OUT = os.path.join( HERE, 'fixtures', 'system_predicate_editors.json' )


def input_widgets( widget ):
    """The widgets under a panel that show or take something, in order, each
    with what it shows: labels, choices (with the one chosen), radio buttons
    (with the texts of their group), numbers, dates, times, text and tick
    boxes."""

    from qtpy import QtWidgets as QW

    out = []

    def add( w, fact ):

        fact[ 'class' ] = type( w ).__name__

        out.append( ( w, fact ) )


    def walk( w ):

        # (hidden widgets aren't part of what is offered)
        if w.isHidden():

            return


        name = type( w ).__name__

        if isinstance( w, QW.QTreeWidget ):

            from qtpy import QtCore as QC

            groups = []

            for i in range( w.topLevelItemCount() ):

                group = w.topLevelItem( i )

                children = [ group.child( j ) for j in range( group.childCount() ) ]

                groups.append( {
                    'text' : group.text( 0 ),
                    'children' : [ c.text( 0 ) for c in children ],
                    'ticked' : [ c.checkState( 0 ) == QC.Qt.CheckState.Checked for c in children ],
                } )


            add( w, { 'kind' : 'tree', 'groups' : groups } )

        elif name == 'RatingLikeDialog':

            from hydrus.client.metadata import ClientRatings

            state = w.GetRatingState()

            add( w, { 'kind' : 'like', 'state' : 'like' if state == ClientRatings.LIKE else 'dislike' if state == ClientRatings.DISLIKE else 'none' } )

        elif name == 'RatingNumericalDialog':

            from hydrus.client.metadata import ClientRatings

            add( w, { 'kind' : 'stars', 'num_stars' : w._num_stars, 'allow_zero' : w._allow_zero, 'rating' : w.GetRating() if w.GetRatingState() == ClientRatings.SET else None } )

        elif name == 'ServiceSpecifierButton':

            add( w, { 'kind' : 'services', 'text' : w.text() } )

        elif isinstance( w, QW.QListWidget ):

            from qtpy import QtCore as QC

            items = [ w.item( i ) for i in range( w.count() ) ]

            add( w, { 'kind' : 'ticks', 'options' : [ item.text() for item in items ], 'ticked' : [ item.checkState() == QC.Qt.CheckState.Checked for item in items ] } )

        elif isinstance( w, QW.QComboBox ):

            add( w, { 'kind' : 'choice', 'choices' : [ w.itemText( i ) for i in range( w.count() ) ], 'chosen' : w.currentText() } )

        elif isinstance( w, QW.QSpinBox ) or isinstance( w, QW.QDoubleSpinBox ):

            add( w, { 'kind' : 'number', 'value' : w.value(), 'min' : w.minimum(), 'max' : w.maximum(), 'suffix' : w.suffix() } )

        elif isinstance( w, QW.QTimeEdit ):

            add( w, { 'kind' : 'time', 'value' : w.time().toString( 'HH:mm:ss' ) } )

        elif isinstance( w, QW.QCalendarWidget ):

            add( w, { 'kind' : 'date', 'value' : w.selectedDate().toString( 'yyyy-MM-dd' ) } )

        elif isinstance( w, QW.QCheckBox ):

            add( w, { 'kind' : 'tick', 'text' : w.text(), 'checked' : w.isChecked() } )

        elif isinstance( w, QW.QRadioButton ):

            group = [ b.text() for b in w.parentWidget().findChildren( QW.QRadioButton ) if not b.isHidden() ]

            add( w, { 'kind' : 'radio', 'text' : w.text(), 'checked' : w.isChecked(), 'group' : group } )

        elif isinstance( w, QW.QLineEdit ):

            add( w, { 'kind' : 'text', 'text' : w.text() } )

        elif isinstance( w, QW.QPlainTextEdit ) or isinstance( w, QW.QTextEdit ):

            add( w, { 'kind' : 'text', 'text' : w.toPlainText() } )

        elif isinstance( w, QW.QLabel ):

            if w.text() != '':

                add( w, { 'kind' : 'label', 'text' : w.text() } )


        elif isinstance( w, QW.QAbstractButton ):

            add( w, { 'kind' : 'button', 'text' : w.text() } )

        else:

            for child in w.children():

                if isinstance( child, QW.QWidget ):

                    walk( child )





    for child in widget.children():

        if isinstance( child, QW.QWidget ):

            walk( child )



    return out


def widget_facts( widget ):

    return [ fact for ( w, fact ) in input_widgets( widget ) ]


# each predicate an editor made, by its text: as the reference stores it
STORED = {}


def error_of( panel ):
    """Why the panel can't make its predicates, if it can't."""

    try:

        panel.GetPredicates()

        return None

    except Exception as e:

        return str( e )



def predicates_of( panel ):

    try:

        predicates = panel.GetPredicates()

        for p in predicates:

            STORED[ p.ToString() ] = json.loads( json.dumps( p.GetSerialisableTuple() ) )


        return [ p.ToString() for p in predicates ]

    except Exception as e:

        # (the url class panel, with no url class to choose; a hash panel
        # with no hashes)
        return None



def changed( w, fact ):
    """Change one input widget as a user would: each other choice, another
    number, some text, the tick flipped. Yields what was set."""

    if fact[ 'kind' ] == 'radio':

        if not fact[ 'checked' ]:

            w.click()

            yield fact[ 'text' ]


    elif fact[ 'kind' ] == 'choice':

        for i in range( w.count() ):

            if w.itemText( i ) != fact[ 'chosen' ]:

                w.setCurrentIndex( i )

                yield w.itemText( i )



    elif fact[ 'kind' ] == 'number':

        value = fact[ 'value' ] + 3 if fact[ 'value' ] + 3 <= fact[ 'max' ] else max( fact[ 'min' ], fact[ 'value' ] - 3 )

        w.setValue( value )

        yield value

    elif fact[ 'kind' ] == 'tick':

        w.click()

        yield not fact[ 'checked' ]

    elif fact[ 'kind' ] == 'tree':

        from qtpy import QtCore as QC

        # each group ticked, then each group's first filetype alone
        for i in range( w.topLevelItemCount() ):

            group = w.topLevelItem( i )

            group.setCheckState( 0, QC.Qt.CheckState.Checked )

            yield group.text( 0 )

            group.setCheckState( 0, QC.Qt.CheckState.Unchecked )


        for i in range( w.topLevelItemCount() ):

            child = w.topLevelItem( i ).child( 0 )

            child.setCheckState( 0, QC.Qt.CheckState.Checked )

            yield w.topLevelItem( i ).text( 0 ) + '/' + child.text( 0 )

            child.setCheckState( 0, QC.Qt.CheckState.Unchecked )


    elif fact[ 'kind' ] == 'like':

        from hydrus.client.metadata import ClientRatings

        for ( text, state ) in ( ( 'like', ClientRatings.LIKE ), ( 'dislike', ClientRatings.DISLIKE ) ):

            w.SetRatingState( state )
            w.valueChanged.emit()

            yield text


    elif fact[ 'kind' ] == 'stars':

        for i in range( 0 if fact[ 'allow_zero' ] else 1, fact[ 'num_stars' ] + 1 ):

            w.SetRating( i / fact[ 'num_stars' ] )
            w.valueChanged.emit()

            yield i / fact[ 'num_stars' ]


    elif fact[ 'kind' ] == 'ticks':

        from qtpy import QtCore as QC

        # each tick box flipped (one per fresh panel: the caller takes the
        # nth)
        state = lambda on: QC.Qt.CheckState.Checked if on else QC.Qt.CheckState.Unchecked

        for ( i, ticked ) in enumerate( fact[ 'ticked' ] ):

            w.item( i ).setCheckState( state( not ticked ) )

            yield fact[ 'options' ][ i ]

            w.item( i ).setCheckState( state( ticked ) )

    elif fact[ 'kind' ] == 'text' and fact[ 'class' ] == 'QLineEdit':

        w.setText( 'abc' )

        w.textChanged.emit( 'abc' )

        yield 'abc'



def record( session ):

    from hydrus.client import ClientConstants as CC
    from hydrus.client import ClientLocation
    from hydrus.client.gui.search import ClientGUISearch
    from hydrus.client.gui.search import ClientGUIPredicatesSingle
    from hydrus.client.search import ClientSearchFileSearchContext
    from hydrus.client.search import ClientSearchTagContext

    controller = session.controller
    gui = controller.gui

    # warnings noted rather than shown, and questions answered yes
    from qtpy import QtWidgets as QW
    from hydrus.client.gui import ClientGUIDialogsMessage
    from hydrus.client.gui import ClientGUIDialogsQuick

    warnings = []

    ClientGUIDialogsMessage.ShowWarning = lambda parent, message: warnings.append( message )
    ClientGUIDialogsQuick.GetYesNo = lambda *args, **kwargs: QW.QDialog.DialogCode.Accepted

    def qt( f ):

        return controller.CallBlockingToQt( gui, f )


    location = ClientLocation.LocationContext.STATICCreateSimple( CC.LOCAL_FILE_SERVICE_KEY )
    file_search_context = ClientSearchFileSearchContext.FileSearchContext( location_context = location, tag_context = ClientSearchTagContext.TagContext() )

    offered = controller.Read( 'file_system_predicates', file_search_context )

    from qtpy import QtCore as QC

    out = {
        'today' : QC.QDate.currentDate().toString( 'yyyy-MM-dd' ),
        'offered' : [
            {
                'text' : p.ToString(),
                'opens_editor' : p.GetValue() is None and p.GetType() in ClientGUISearch.FLESH_OUT_SYSTEM_PRED_TYPES,
            }
            for p in offered
        ],
        'editors' : [],
    }

    def pages_of( panel ):

        if hasattr( panel, '_notebook' ):

            return [ ( panel._notebook.tabText( i ), panel._notebook.widget( i ) ) for i in range( panel._notebook.count() ) ]

        else:

            return [ ( '', panel ) ]



    def panels_of( page ):

        return [ w._predicate_panel for w in page.findChildren( ClientGUISearch.FleshOutPredicatePanel._PredOKPanel ) ]


    def editor( predicate ):

        panel = ClientGUISearch.FleshOutPredicatePanel( gui, predicate )

        # (shown, so what each panel hides at first is hidden)
        panel.show()

        recorded_pages = []

        for ( page_i, ( name, page ) ) in enumerate( pages_of( panel ) ):

            buttons = []
            panels = []

            for w in page.findChildren( ClientGUIPredicatesSingle.StaticSystemPredicateButton ):

                for p in w._predicates:

                    STORED[ p.ToString() ] = json.loads( json.dumps( p.GetSerialisableTuple() ) )


                buttons.append( {
                    'label' : w._predicates_button.text(),
                    'predicates' : [ p.ToString() for p in w._predicates ],
                } )


            for ( panel_i, inner ) in enumerate( panels_of( page ) ):

                widgets = widget_facts( inner )

                panels.append( {
                    'class' : type( inner ).__name__,
                    'widgets' : widgets,
                    'predicates' : predicates_of( inner ),
                    'changes' : changes( predicate, page_i, panel_i, widgets ),
                } )


            recorded_pages.append( { 'name' : name, 'buttons' : buttons, 'panels' : panels } )


        panel.hide()
        panel.deleteLater()

        return recorded_pages


    def changes( predicate, page_i, panel_i, facts ):
        """Each widget of a fresh panel changed in turn (each other choice
        of a choice), and what the panel then adds."""

        out = []

        for ( widget_i, fact ) in enumerate( facts ):

            if fact[ 'kind' ] == 'radio':

                count = 0 if fact[ 'checked' ] else 1

            elif fact[ 'kind' ] == 'choice':

                count = len( [ c for c in fact[ 'choices' ] if c != fact[ 'chosen' ] ] )

            elif fact[ 'kind' ] == 'ticks':

                count = len( fact[ 'options' ] )

            elif fact[ 'kind' ] == 'tree':

                count = 2 * len( fact[ 'groups' ] )

            elif fact[ 'kind' ] == 'like':

                count = 2

            elif fact[ 'kind' ] == 'stars':

                count = fact[ 'num_stars' ] + ( 1 if fact[ 'allow_zero' ] else 0 )

            elif fact[ 'kind' ] in ( 'number', 'tick' ) or ( fact[ 'kind' ] == 'text' and fact[ 'class' ] == 'QLineEdit' ):

                count = 1

            else:

                count = 0


            for k in range( count ):

                panel = ClientGUISearch.FleshOutPredicatePanel( gui, predicate )

                panel.show()

                inner = panels_of( pages_of( panel )[ page_i ][1] )[ panel_i ]

                ( w, _ ) = input_widgets( inner )[ widget_i ]

                gen = changed( w, fact )

                for _ in range( k + 1 ):

                    value = next( gen )


                out.append( { 'widget' : widget_i, 'set' : value, 'predicates' : predicates_of( inner ), 'widgets' : widget_facts( inner ) } )

                panel.hide()
                panel.deleteLater()



        return out


    def apply( w, fact, value ):

        from qtpy import QtCore as QC

        if fact[ 'kind' ] in ( 'radio', 'tick', 'button' ):

            w.click()

        elif fact[ 'kind' ] == 'tree':

            ( group_text, _, child_text ) = value.partition( '/' )

            group = [ w.topLevelItem( i ) for i in range( w.topLevelItemCount() ) if w.topLevelItem( i ).text( 0 ) == group_text ][0]

            item = group if child_text == '' else [ group.child( j ) for j in range( group.childCount() ) if group.child( j ).text( 0 ) == child_text ][0]

            item.setCheckState( 0, QC.Qt.CheckState.Unchecked if item.checkState( 0 ) == QC.Qt.CheckState.Checked else QC.Qt.CheckState.Checked )

        elif fact[ 'kind' ] == 'like':

            from hydrus.client.metadata import ClientRatings

            w.SetRatingState( { 'like' : ClientRatings.LIKE, 'dislike' : ClientRatings.DISLIKE, 'none' : ClientRatings.NULL }[ value ] )
            w.valueChanged.emit()

        elif fact[ 'kind' ] == 'stars':

            w.SetRating( value )
            w.valueChanged.emit()

        elif fact[ 'kind' ] == 'services':

            from hydrus.client import ClientServices

            if 'types' in value:

                specifier = ClientServices.ServiceSpecifier( service_types = value[ 'types' ] )

            else:

                keys = [ s.GetServiceKey() for s in controller.services_manager.GetServices() if s.GetName() in value[ 'services' ] ]

                specifier = ClientServices.ServiceSpecifier( service_keys = keys )


            w.SetValue( specifier )
            w.valueChanged.emit()

        elif fact[ 'kind' ] == 'choice':

            w.setCurrentIndex( w.findText( value ) )

        elif fact[ 'kind' ] == 'number':

            w.setValue( value )

        elif fact[ 'kind' ] == 'text':

            if hasattr( w, 'setPlainText' ):

                w.setPlainText( value )

            else:

                w.setText( value )


        elif fact[ 'kind' ] == 'ticks':

            item = w.item( fact[ 'options' ].index( value ) )

            item.setCheckState( QC.Qt.CheckState.Unchecked if item.checkState() == QC.Qt.CheckState.Checked else QC.Qt.CheckState.Checked )

        elif fact[ 'kind' ] == 'date':

            w.setSelectedDate( QC.QDate.fromString( value, 'yyyy-MM-dd' ) )

        elif fact[ 'kind' ] == 'time':

            w.setTime( QC.QTime.fromString( value, 'HH:mm' ) )



    def scenario( text, page_i, panel_i, steps ):
        """Several widgets of a fresh panel set in turn (each step: the
        widget's place among those shown then, and what it is set to), with
        the widgets shown before each step, and what the panel adds after."""

        predicate = [ p for p in offered if p.ToString() == text ][0]

        def run():

            panel = ClientGUISearch.FleshOutPredicatePanel( gui, predicate )

            panel.show()

            inner = panels_of( pages_of( panel )[ page_i ][1] )[ panel_i ]

            done = []

            warnings.clear()

            for ( widget_i, value ) in steps:

                widgets = input_widgets( inner )

                ( w, fact ) = widgets[ widget_i ]

                done.append( { 'widget' : widget_i, 'set' : value, 'widgets' : [ f for ( _, f ) in widgets ] } )

                apply( w, fact, value )


            result = {
                'editor' : text,
                'page' : page_i,
                'panel' : panel_i,
                'steps' : done,
                'predicates' : predicates_of( inner ),
                'error' : error_of( inner ),
                'warnings' : list( warnings ),
                'widgets' : widget_facts( inner ),
            }

            panel.hide()
            panel.deleteLater()

            return result


        return qt( run )


    # what single changes don't show: the number of tags swapped for a
    # namespace's own predicate, typed dates and times, tags cleaned, and
    # the amounts either side of "≈"
    out[ 'scenarios' ] = [
        scenario( 'system:number of tags', 0, 0, [ ( 2, None ), ( 3, 'character' ), ( 7, None ), ( 8, 0 ) ] ),
        scenario( 'system:number of tags', 0, 0, [ ( 2, None ), ( 3, 'character' ), ( 6, None ), ( 8, 0 ) ] ),
        scenario( 'system:number of tags', 0, 0, [ ( 2, None ), ( 3, 'character' ), ( 4, None ), ( 8, 1 ) ] ),
        scenario( 'system:number of tags', 0, 0, [ ( 2, None ), ( 3, 'character' ), ( 7, None ), ( 8, 2 ) ] ),
        scenario( 'system:number of tags', 0, 0, [ ( 1, None ), ( 6, None ), ( 8, 0 ) ] ),
        scenario( 'system:time', 0, 1, [ ( 1, None ), ( 4, '2011-06-04' ), ( 5, '13:05' ) ] ),
        scenario( 'system:time', 2, 1, [ ( 3, None ), ( 4, '1999-12-31' ) ] ),
        scenario( 'system:tag (advanced)', 0, 0, [ ( 2, 'my tags' ), ( 3, None ), ( 6, ' Blue Eyes ' ) ] ),
        scenario( 'system:tag (advanced)', 0, 0, [ ( 6, 'series:metroid' ) ] ),
        scenario( 'system:dimensions', 0, 0, [ ( 2, None ), ( 9, 37 ) ] ),
        scenario( 'system:dimensions', 0, 0, [ ( 3, None ), ( 9, 4 ) ] ),
        scenario( 'system:duration', 0, 0, [ ( 2, None ), ( 8, 1 ), ( 10, 2 ), ( 12, 3 ), ( 14, 4 ), ( 17, 5 ), ( 19, 6 ), ( 21, 7 ) ] ),
        scenario( 'system:duration', 0, 0, [ ( 3, None ), ( 8, 2 ), ( 16, 25 ) ] ),
        scenario( 'system:duration', 0, 1, [ ( 1, None ), ( 5, 10 ) ] ),
        scenario( 'system:notes', 0, 1, [ ( 1, 'comment' ) ] ),
        scenario( 'system:urls', 0, 1, [ ( 0, 'does not have' ), ( 1, 'Example.COM' ) ] ),
        scenario( 'system:file viewing statistics', 0, 0, [ ( 0, 'preview views' ), ( 0, 'client api views' ) ] ),
        scenario( 'system:file viewing statistics', 0, 0, [ ( 0, 'media views' ) ] ),
        scenario( 'system:tag (advanced)', 0, 0, [ ( 5, 'current' ), ( 5, 'pending' ), ( 5, 'deleted' ), ( 6, 'blue eyes' ) ] ),
        scenario( 'system:tag (advanced)', 0, 0, [ ( 5, 'current' ), ( 5, 'pending' ), ( 6, 'blue eyes' ) ] ),
        # the four editors with their own controls
        scenario( 'system:filetype', 0, 0, [ ( 0, 'image' ), ( 0, 'video' ) ] ),
        scenario( 'system:filetype', 0, 0, [ ( 0, 'image' ), ( 0, 'image/png' ) ] ),
        scenario( 'system:filetype', 0, 0, [ ( 0, 'image/png' ), ( 0, 'image/jpeg' ), ( 0, 'video/webm' ) ] ),
        scenario( 'system:filetype', 0, 0, [ ( 2, None ), ( 0, 'audio' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, '03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 1, None ), ( 6, '03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b\n5ec0ffee5ec0ffee5ec0ffee5ec0ffee5ec0ffee5ec0ffee5ec0ffee5ec0ffee\n03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, 'sha256:03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, ' 0x03D67E1677D7723A590C345FB438C585CC818FFDAD77CD8F2824F8C9E85E276B ' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, 'd41d8cd98f00b204e9800998ecf8427e' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, 'd41d8cd98f00b204e9800998ecf8427e' ), ( 7, None ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 3, None ), ( 6, 'md5:d41d8cd98f00b204e9800998ecf8427e' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 4, None ), ( 6, 'da39a3ee5e6b4b0d3255bfef95601890afd80709' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, 'not a hash' ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, 'not a hash\nd41d8cd98f00b204e9800998ecf8427e' ), ( 7, None ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, 'not a hash\nd41d8cd98f00b204e9800998ecf8427e' ), ( 8, None ) ] ),
        scenario( 'system:hash', 0, 0, [ ( 6, 'md5:03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b' ) ] ),
        scenario( 'system:rating', 0, 0, [ ( 2, None ), ( 3, { 'services' : [ 'favourites' ] } ) ] ),
        scenario( 'system:rating', 0, 0, [ ( 0, None ), ( 3, { 'types' : [ 7 ] } ), ( 5, None ) ] ),
        scenario( 'system:rating', 0, 0, [ ( 3, { 'services' : [ 'stars', 'counter' ] } ) ] ),
        scenario( 'system:rating', 0, 0, [ ( 3, { 'types' : [ 7, 6 ] } ) ] ),
        scenario( 'system:rating', 0, 0, [ ( 2, None ), ( 5, { 'services' : [ 'stars' ] } ), ( 3, { 'services' : [ 'favourites' ] } ) ] ),
        scenario( 'system:rating', 0, 0, [ ( 2, None ), ( 5, { 'services' : [ 'stars', 'favourites' ] } ), ( 3, { 'services' : [ 'favourites' ] } ) ] ),
        scenario( 'system:rating', 0, 0, [ ( 2, None ), ( 5, { 'types' : [ 7 ] } ), ( 3, { 'types' : [ 7 ] } ) ] ),
        scenario( 'system:rating', 0, 1, [ ( 3, None ), ( 4, 'like' ) ] ),
        scenario( 'system:rating', 0, 1, [ ( 4, 'dislike' ) ] ),
        scenario( 'system:rating', 0, 1, [ ( 4, 'like' ), ( 1, None ) ] ),
        scenario( 'system:rating', 0, 2, [ ( 3, None ), ( 7, 0.6 ) ] ),
        scenario( 'system:rating', 0, 2, [ ( 6, None ), ( 7, 0.4 ) ] ),
        scenario( 'system:rating', 0, 2, [ ( 4, None ), ( 7, 0.2 ) ] ),
        scenario( 'system:rating', 0, 2, [ ( 7, 1.0 ) ] ),
        scenario( 'system:rating', 0, 2, [ ( 7, 1.0 ), ( 2, None ) ] ),
        scenario( 'system:rating', 0, 3, [ ( 3, None ), ( 7, 5 ) ] ),
        scenario( 'system:rating', 0, 3, [ ( 5, None ), ( 7, 3 ) ] ),
        scenario( 'system:rating', 0, 3, [ ( 6, None ), ( 7, 10 ) ] ),
        scenario( 'system:rating', 0, 3, [ ( 4, None ), ( 7, 2 ) ] ),
        scenario( 'system:similar files', 1, 0, [ ( 0, '03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b' ) ] ),
        scenario( 'system:similar files', 1, 0, [ ( 0, '03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b\n5ec0ffee5ec0ffee5ec0ffee5ec0ffee5ec0ffee5ec0ffee5ec0ffee5ec0ffee' ), ( 1, 8 ) ] ),
        scenario( 'system:similar files', 1, 0, [ ( 0, 'd41d8cd98f00b204e9800998ecf8427e' ) ] ),
        scenario( 'system:similar files', 0, 0, [ ( 2, '03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b' ) ] ),
        scenario( 'system:similar files', 0, 0, [ ( 3, '0f0f0f0f0f0f0f0f' ), ( 4, 6 ) ] ),
        scenario( 'system:similar files', 0, 0, [ ( 2, '03d67e1677d7723a590c345fb438c585cc818ffdad77cd8f2824f8c9e85e276b' ), ( 3, '0f0f0f0f0f0f0f0f\nabcdef0123456789' ) ] ),
        scenario( 'system:file viewing statistics', 0, 1, [ ( 1, None ), ( 5, 2 ), ( 7, 3 ), ( 9, 4 ), ( 11, 5 ), ( 13, 6 ) ] ),
        # the modified and archived pages' absolute dates, as import and last viewed above
        scenario( 'system:time', 1, 1, [ ( 1, None ), ( 4, '2011-06-04' ), ( 5, '13:05' ) ] ),
        scenario( 'system:time', 1, 1, [ ( 3, None ), ( 4, '1999-12-31' ) ] ),
        scenario( 'system:time', 3, 1, [ ( 1, None ), ( 4, '2011-06-04' ), ( 5, '13:05' ) ] ),
        scenario( 'system:time', 3, 1, [ ( 3, None ), ( 4, '1999-12-31' ) ] ),
    ]

    for p in offered:

        if p.GetValue() is None and p.GetType() in ClientGUISearch.FLESH_OUT_SYSTEM_PRED_TYPES:

            out[ 'editors' ].append( { 'text' : p.ToString(), 'pages' : qt( lambda: editor( p ) ) } )



    out[ 'stored' ] = [ { 'text' : text, 'serialised' : serialised } for ( text, serialised ) in sorted( STORED.items() ) ]

    services = []

    for service in controller.services_manager.GetServices():

        num_stars = service.GetNumStars() if hasattr( service, 'GetNumStars' ) else None
        allow_zero = service.AllowZero() if hasattr( service, 'AllowZero' ) else None

        services.append( { 'name' : service.GetName(), 'type' : service.GetServiceType(), 'key' : service.GetServiceKey().hex(), 'num_stars' : num_stars, 'allow_zero' : allow_zero } )


    out[ 'services' ] = services
    out[ 'default_view_canvases' ] = [ 'media' ]

    return out


def main():

    import hydrus_driver
    import record_api

    db_dir = record_api.unpack_fixture( 'basic' )

    result = hydrus_driver.run_client( db_dir, record )

    with open( OUT, 'w' ) as f:

        json.dump( result, f, indent = 1, sort_keys = True, ensure_ascii = False )
        f.write( '\n' )


    print( f'wrote {OUT}' )


if __name__ == '__main__':

    main()
