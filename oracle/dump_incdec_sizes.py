#!/usr/bin/env python3
"""Dump the reference's inc/dec rating rectangle sizes (`GetIncDecSize`).

For several box heights (whole and fractional) and rating numbers, the width
and height of the rectangle the reference draws.

Usage: QT_QPA_PLATFORM=offscreen python oracle/dump_incdec_sizes.py
       (writes fixtures/incdec_sizes.json)
"""

import json
import os
import sys

HERE = os.path.dirname( os.path.abspath( __file__ ) )

sys.path.insert( 0, os.path.dirname( HERE ) )

OUT = os.path.join( HERE, 'fixtures', 'incdec_sizes.json' )


def main():

    from hydrus.client.gui import ClientGUIRatings

    rows = []

    for height in [ 2, 12, 16, 17, 18, 30, 255 ]:

        for number in [ None, 0, 1, 99, 999, 1000, 1234, 99999, 123456, 1234567, 99999999 ]:

            size = ClientGUIRatings.GetIncDecSize( height, number )

            rows.append( [ height, number, size.width(), size.height() ] )

    with open( OUT, 'w' ) as f:

        json.dump( { 'rows' : rows }, f, indent = 1 )
        f.write( '\n' )

    print( f'wrote {OUT}: {len( rows )} rows' )


if __name__ == '__main__':

    main()
