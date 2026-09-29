#!/usr/bin/env python3
"""Record the reference implementation's tag cleaning on awkward inputs.

Usage: python oracle/dump_tag_cleaning.py > oracle/fixtures/tag_cleaning.json
"""
import json
import os
import random
import sys

sys.path.insert( 0, os.path.join( os.path.dirname( __file__ ), '..' ) )

from hydrus.core import HydrusTags

cases = [
    '', ' ', 'blue eyes', 'Blue Eyes', '  blue   eyes  ', 'blue_eyes', 'character:samus aran',
    'Character:Samus Aran', 'character : samus aran', ':D', '::D', ':weird:stuff', ':', '::', ':::',
    'a:b:c', 'series:', ':samus', 'system:inbox', 'system:system:tag', '-tag', '--tag', '-system:tag',
    'system:-tag', 'creator:-tag', '- tag', '\ttab\ttag\n', 'new\nline', 'bom﻿tag', '﻿start',
    'zero​width', 'blue_eyes‌', '‌‍', 'ㅤhangul filler', '한국어ㅤ태그', 'ÉCOLE', 'straße',
    'ΣΊΣΥΦΟΣ', 'İstanbul', 'ǅ', 'emoji 🎉 tag', 'nbsp tag', 'ideographic　space', 'em space',
    'control\u0007char', 'del\u007fchar', 'rtl‮text', 'privateuse', 'x' * 1100, 'ns:' + 'y' * 1100,
    '  :  ', ' : tag', 'namespace:  ', 'a: :b', 'weird:​', 'title:hello: world', '\u0085nel', 'ﬁ ligature',
    'Ⅻ roman', 'ＦＵＬＬＷＩＤＴＨ', '1', '001', ':1', 'rating:5/5', 'ns:sub:sub2', 'system', 'SYSTEM:everything',
    'meta:-', '-', '--', 'system:', 'system:-', '\x00null', 'tag⁦isolate⁩', 'a‍', '‍b',
    'café‌', 'multi  :  colon : parts', ' leading space', 'trailing space ', 'MiXeD:CaSe:TaG',
]

rng = random.Random( 688 )
alphabet = list( 'ab :-_\t\n' ) + [ '​', '‌', 'ㅤ', 'É', 'ß', 'Σ', '﻿', ' ', '한', 'system:' ]
for _ in range( 400 ):
    cases.append( ''.join( rng.choice( alphabet ) for _ in range( rng.randint( 0, 12 ) ) ) )

out = []
for case in cases:
    try:
        cleaned = HydrusTags.CleanTag( case )
        ok = HydrusTags.TagOK( case )
        out.append( { 'input': case, 'clean': cleaned, 'ok': ok, 'split': list( HydrusTags.SplitTag( cleaned ) ) } )
    except Exception as e:
        out.append( { 'input': case, 'error': str( e ) } )

json.dump( out, sys.stdout, ensure_ascii = True, indent = 0 )
print()
