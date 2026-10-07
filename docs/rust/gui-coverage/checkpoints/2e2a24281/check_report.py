"""Inspect only the staged report supplied on the command line, offline."""
import sys,json,hashlib
from pathlib import Path
from playwright.sync_api import sync_playwright
html=Path(sys.argv[1]);out=Path(sys.argv[2]);out.mkdir(parents=True,exist_ok=True)
leaf='audit-media-menu-database-fix-missing-file-archived-times'
errors=[];requests=[]
with sync_playwright() as pw:
 browser=pw.chromium.launch(executable_path='/usr/bin/chromium',args=['--no-sandbox'])
 page=browser.new_page(viewport={'width':1365,'height':900})
 page.on('pageerror',lambda e:errors.append(str(e)))
 page.route('**/*',lambda route:(requests.append(route.request.url),route.abort()))
 page.set_content(html.read_text(),wait_until='load')
 data=page.locator('#coverage-data').text_content();data=json.loads(data)
 assert len(data['reference']['nodes'])==1812
 assert len(data['native']['nodes'])==1799
 assert len(data['reference']['overnight_progress']['completed_feature_ids'])==374
 assert leaf in data['reference']['overnight_progress']['completed_feature_ids']
 page.locator('#overnight-completions').click()
 assert page.locator('#match-count').inner_text()=='374 matching · 1812 inventoried'
 page.locator('#search').fill('fix missing file archived times')
 page.locator(f'[data-select="{leaf}"]').click()
 assert 'cancellation' in page.locator('#details').inner_text().lower()
 page.screenshot(path=str(out/'report-desktop.png'),full_page=True)
 for width,height in [(1365,900),(390,844)]:
  page.set_viewport_size({'width':width,'height':height})
  assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'),width
  if width==390:page.screenshot(path=str(out/'report-narrow.png'),full_page=True)
 page.locator('#reset').click()
 assert page.locator('#match-count').inner_text()=='1812 matching · 1812 inventoried'
 page.locator('#native-tab').click()
 assert page.locator('#match-count').inner_text()=='1799 matching · 1799 inventoried'
 page.locator('#reference-tab').click()
 assert not errors,errors
 assert not requests,requests
 browser.close()
(out/'browser-check.json').write_text(json.dumps({'html_sha256':hashlib.sha256(html.read_bytes()).hexdigest(),'signed_off':374,'reference_count':1812,'native_count':1799,'selected_leaf':leaf,'viewports':[[1365,900],[390,844]],'page_errors':errors,'requests':requests,'delivery':'Exact staged HTML loaded offline using set_content; source links not followed.','passed':True},indent=2)+'\n')
print('Staged report browser checks passed.')
