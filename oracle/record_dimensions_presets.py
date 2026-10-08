"""Record all nine real Qt dimensions preset buttons and cancellation.

Run on a fresh private basic database. Drive Button.click in the actual modal
FleshOutPredicates consumer; record accepted ordered predicates and live recent
history. This is control-signal activation, not physical pointer input. The PNG
shows the actual dialog before acceptance. No reference behavior is replaced.
"""
import json,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'oracle'))
sys.path.insert(0,str(ROOT))
OUT=ROOT/'oracle'/'fixtures'

def record(session):
 def qt():
  from qtpy import QtCore as QC, QtWidgets as QW
  from hydrus.client.gui.search import ClientGUISearch as Search, ClientGUIPredicatesSingle as Single
  from hydrus.client.search import ClientSearchPredicate as P
  types=[P.PREDICATE_TYPE_SYSTEM_WIDTH,P.PREDICATE_TYPE_SYSTEM_HEIGHT,P.PREDICATE_TYPE_SYSTEM_RATIO,P.PREDICATE_TYPE_SYSTEM_NUM_PIXELS]
  options=session.controller.new_options
  labels=['system:ratio = 16:9','system:ratio = 9:16','system:ratio = 4:3','system:ratio is square','system:ratio is portrait','system:ratio is landscape','1080p','720p','4k']
  cases=[]
  for label in labels+[None]:
   for pred in options.GetRecentPredicates(types): options.RemoveRecentPredicate(pred)
   observation={}
   def activate():
    dialog=QW.QApplication.activeModalWidget()
    try:
     assert dialog is not None,'No active modal dialog'
     panel=dialog.findChild(Search.FleshOutPredicatePanel)
     assert panel is not None,'No actual predicate panel'
     buttons=panel.findChildren(Single.StaticSystemPredicateButton)
     observation['labels']=[b._predicates_button.text() for b in buttons]
     if label==labels[0]:
      assert dialog.grab().save(str(OUT/'dimensions_presets.png'))
     if label is None: dialog.reject()
     else:
      matches=[b for b in buttons if b._predicates_button.text()==label]
      assert len(matches)==1,(label,observation['labels'])
      matches[0]._predicates_button.click()
    except Exception as e:
     observation['error']=repr(e)
     if dialog is not None: dialog.reject()
   QC.QTimer.singleShot(300,activate)
   result=Search.FleshOutPredicates(session.controller.gui,[P.Predicate(P.PREDICATE_TYPE_SYSTEM_DIMENSIONS)])
   assert 'error' not in observation,observation
   cases.append({'label':label,'predicates':[p.ToString() for p in result],'serialised':[p.GetSerialisableTuple() for p in result],'recent':[p.ToString() for p in options.GetRecentPredicates(types)],'observed_labels':observation['labels']})
  return {'scope':'Actual Qt button activation and real FleshOutPredicates acceptance, cancellation and live recent-history consumer.','cases':cases}
 return session.controller.CallBlockingToQt(session.controller.gui,qt)

if __name__=='__main__':
 import hydrus_driver,record_api
 if '--child' in sys.argv:
  result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
  (OUT/'dimensions_presets.json').write_text(json.dumps(result,indent=2)+'\n')
 else:
  hydrus_driver.run_in_subprocess(__file__,'--child')
