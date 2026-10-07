"""Compare exact-source OR frames with the prior validated blank-field baseline."""
import hashlib,json,sys
from pathlib import Path
from PIL import Image,ImageChops
baseline=Path(sys.argv[1]);native=Path(sys.argv[2]);out=Path(sys.argv[3])
rows={}
for name in ('or-connector-ascii-reopened-native.png','or-connector-fox-reopened-native.png'):
 a=Image.open(baseline/name).convert('RGB');b=Image.open(native/name).convert('RGB');assert a.size==b.size==(900,900)
 box=ImageChops.difference(a,b).getbbox()
 ink=sum(all(c<180 for c in b.getpixel((x,y))) for y in range(502,524) for x in range(640,690))
 rows[name]={'sha256':hashlib.sha256((native/name).read_bytes()).hexdigest(),'changed_pixel_bounds':box,'field_dark_pixels':ink}
 if 'ascii' in name: assert box is None,'ASCII frame changed; inspect before proceeding'
 else:
  assert box is not None and 628<=box[0]<box[2]<=887 and 499<=box[1]<box[3]<=530,box
  assert ink>=10,ink
out.write_text(json.dumps({'source_commit':'88e9851e6ab8ecec08eb03f0d5cd3a2f8b7d2339','scope':'Pixel comparison supplements direct fresh image review; does not claim Qt pixel parity.','frames':rows,'passed':True},indent=2)+'\n')
print(json.dumps(rows,indent=2))
