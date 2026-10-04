#!/usr/bin/env python3
"""Record real Tag Editing default/remember controls and ManageTags tab consumers.

The options chooser disables while remembering. Real notebook tab changes update
preferences immediately when enabled, even when tag drafts are cancelled.
"""
import json
import os
import sys
import tempfile
import time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, 'fixtures', 'tag_dialog_defaults.json')

def record(session):
    from hydrus.client import ClientConstants as CC, ClientLocation
    from hydrus.client.gui.panels.options.TagEditingPanel import TagEditingPanel
    from hydrus.client.gui.metadata.ClientGUIManageTags import ManageTagsPanel
    from hydrus.client.media import ClientMediaSingle
    from hydrus.core import HydrusConstants as HC
    c = session.controller
    qt = lambda f: c.CallBlockingToQt(c.gui, f)
    name = lambda key: c.services_manager.GetName(key) if c.services_manager.ServiceExists(key) else 'missing service'
    services = qt(lambda: c.services_manager.GetServices((HC.LOCAL_TAG,)))
    keys = {s.GetName(): s.GetServiceKey() for s in services}
    def preferences():
        return {'remember': c.new_options.GetBoolean('save_default_tag_service_tab_on_change'), 'service': name(c.new_options.GetKey('default_tag_service_tab'))}
    manifest = json.load(open(os.path.join(HERE, 'fixtures', 'legacy_db', 'basic.manifest.json')))
    hashes = [bytes.fromhex(f['hash']) for f in manifest['files']]
    results = c.Read('media_results', hashes)
    media = [ClientMediaSingle.MediaSingle(m) for m in results if CC.LOCAL_FILE_SERVICE_KEY in m.GetLocationsManager().GetCurrent()][:1]
    controls = []
    def options(action, remember=None, service=None):
        p = TagEditingPanel(c.gui, c.new_options)
        if remember is not None and p._save_default_tag_service_tab_on_change.isChecked() != remember:
            p._save_default_tag_service_tab_on_change.click()
        if service is not None:
            p._default_tag_service_tab.SetValue(keys[service])
        controls.append({'action': action, 'remember': p._save_default_tag_service_tab_on_change.isChecked(), 'service': name(p._default_tag_service_tab.GetValue()), 'default_enabled': p._default_tag_service_tab.isEnabled(), 'choices': [p._default_tag_service_tab.itemText(i) for i in range(p._default_tag_service_tab.count())]})
        p.UpdateOptions()
        p.deleteLater()
    qt(lambda: options('initial'))
    initial = qt(preferences)
    qt(lambda: options('fixed_my_tags', False, 'my tags'))
    events = []
    def opened():
        p = qt(lambda: ManageTagsPanel(c.gui, ClientLocation.LocationContext.STATICCreateSimple(CC.LOCAL_FILE_SERVICE_KEY), CC.TAG_PRESENTATION_SEARCH_PAGE_MANAGE_TAGS, media))
        # Real CallAfterQtSafe selects the initial service and connects notebook signals.
        time.sleep(0.3)
        return p
    def snapshot(p, action):
        return {'action': action, 'selected': name(p._tag_services.currentWidget().GetServiceKey()), 'preferences': preferences()}
    def choose(p, service):
        for i in range(p._tag_services.count()):
            if p._tag_services.widget(i).GetServiceKey() == keys[service]:
                p._tag_services.setCurrentIndex(i)
                return
        raise RuntimeError('missing runtime service '+service)
    p = opened(); events.append(qt(lambda: snapshot(p, 'open_fixed')))
    qt(lambda: choose(p, 'downloader tags')); events.append(qt(lambda: snapshot(p, 'change_fixed')))
    qt(p.deleteLater)
    p = opened(); events.append(qt(lambda: snapshot(p, 'reopen_after_fixed_cancel'))); qt(p.deleteLater)
    qt(lambda: options('remember_my_tags', True))
    p = opened(); events.append(qt(lambda: snapshot(p, 'open_remember')))
    qt(lambda: choose(p, 'downloader tags')); events.append(qt(lambda: snapshot(p, 'change_remember')))
    qt(p.deleteLater)
    p = opened(); events.append(qt(lambda: snapshot(p, 'reopen_after_remember_cancel'))); qt(p.deleteLater)
    qt(lambda: options('stop_remembering', False))
    p = opened(); qt(lambda: choose(p, 'my tags')); events.append(qt(lambda: snapshot(p, 'change_after_disable'))); qt(p.deleteLater)
    qt(lambda: c.new_options.SetKey('default_tag_service_tab', b'missing tag service'))
    p = opened(); events.append(qt(lambda: snapshot(p, 'missing_service_fallback'))); qt(p.deleteLater)
    return {'initial_preferences': initial, 'controls': controls, 'events': events}

def child(out):
    import hydrus_driver
    import record_api
    result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
    with open(out, 'w') as f: json.dump(result, f)

def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--child': child(sys.argv[2]); return
    import hydrus_driver
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, 'out.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', p)
        with open(p) as f: result = json.load(f)
    with open(OUT, 'w') as f: json.dump(result, f, indent=2, ensure_ascii=False); f.write('\n')
    print('wrote', OUT)
if __name__ == '__main__': main()
