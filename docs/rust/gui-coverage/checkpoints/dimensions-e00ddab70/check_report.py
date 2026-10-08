"""Check the exact supplied offline report and explicit verification union."""
import sys,json,hashlib
from pathlib import Path
from playwright.sync_api import sync_playwright
html=Path(sys.argv[1]);out=Path(sys.argv[2]);total=int(sys.argv[3]);out.mkdir(parents=True,exist_ok=True)
leaf='audit-options-predicate-dimensions-presets-1080p';errors=[];requests=[]
with sync_playwright() as pw:
 browser=pw.chromium.launch(executable_path='/usr/bin/chromium',args=['--no-sandbox'])
 page=browser.new_page(viewport={'width':1365,'height':900})
 page.on('pageerror',lambda e:errors.append(str(e)))
 page.route('**/*',lambda route:(requests.append(route.request.url),route.abort()))
 page.set_content(html.read_text(),wait_until='load')
 data=json.loads(page.locator('#coverage-data').text_content());goal=data['reference']['goal_verification']
 assert len(data['reference']['nodes'])==1812 and len(data['native']['nodes'])==1799
 assert goal['implementation_completions']==376 and goal['verified_goal_leaves']==total
 assert goal['historical_verifications']==total-376
 assert len(set(goal['verified_feature_ids']))==total
 assert len(data['reference']['overnight_progress']['completed_feature_ids'])==376
 assert str(total)+' of 1274' in page.locator('#overnight-note').inner_text()
 page.locator('#overnight-completions').click()
 assert page.locator('#match-count').inner_text()==f'{total} matching · 1812 inventoried'
 if total==376:page.locator('#reset').click()
 page.locator('#search').fill('1080p')
 page.locator(f'[data-select="{leaf}"]').click()
 if total>376:
  # CSS uppercases rendered headings; check semantic heading text directly.
  assert 'Explicit historical verification' in page.locator('#details h3').all_text_contents()
  assert 'original approval is retained' in page.locator('#details').inner_text()
 page.screenshot(path=str(out/'report-desktop.png'),full_page=True)
 for width,height in [(1365,900),(390,844)]:
  page.set_viewport_size({'width':width,'height':height})
  assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'),width
  if width==390:page.screenshot(path=str(out/'report-narrow.png'),full_page=True)
 page.locator('#reset').click()
 assert page.locator('#match-count').inner_text()=='1812 matching · 1812 inventoried'
 page.locator('#native-tab').click()
 assert page.locator('#match-count').inner_text()=='1799 matching · 1799 inventoried'
 assert not errors,errors
 assert not requests,requests
 browser.close()
(out/'browser-check.json').write_text(json.dumps({'html_sha256':hashlib.sha256(html.read_bytes()).hexdigest(),'verified_goal_leaves':total,'implementation_completions':376,'historical_verifications':total-376,'viewports':[[1365,900],[390,844]],'selected_leaf':leaf,'page_errors':errors,'external_requests':requests,'passed':True},indent=2)+'\n')
print('Report checks passed:',total,'verified leaves')
