#!/usr/bin/env python3
"""Record the reference's Tags > sibling/parent sync > review panel
(`ReviewTagDisplayMaintenancePanel`) on the basic client: the message, the status
line under each combination of the two sync switches, the tabs and the one chosen
first, and each tab's progress text once its asynchronous refresh has published.
"""
import json, os, sys, tempfile, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

def record(session):
    controller = session.controller
    gui = controller.gui
    state = {}
    def build():
        from hydrus.client.gui.metadata import ClientGUITagDisplayMaintenanceReview as R
        panel = R.ReviewTagDisplayMaintenancePanel(gui)
        state['panel'] = panel
        options = controller.new_options
        result = {'message': panel._message.text(), 'listbook': options.GetBoolean('use_listbook_for_tag_service_panels'), 'status_by_switches': []}
        for idle in (True, False):
            for active in (True, False):
                options.SetBoolean('tag_display_maintenance_during_idle', idle)
                options.SetBoolean('tag_display_maintenance_during_active', active)
                panel._UpdateStatusText()
                result['status_by_switches'].append({'idle': idle, 'active': active, 'text': panel._sync_status.text(), 'object_name': panel._sync_status.objectName()})
        return result
    result = controller.CallBlockingToQt(gui, build)
    time.sleep(2.0)
    def read():
        panel = state['panel']
        tabs = panel._tag_services
        out = []
        for i in range(tabs.count()):
            page = tabs.widget(i)
            out.append({'name': tabs.tabText(i) if hasattr(tabs, 'tabText') else str(i), 'siblings_and_parents': page._siblings_and_parents_st.text(), 'progress': page._progress._st.text() if hasattr(page._progress, '_st') else None, 'work_button_visible': not page._go_faster_button.isHidden(), 'work_button': page._go_faster_button.text()})
        current = tabs.currentIndex()
        panel.deleteLater()
        return out, current
    tabs, current = controller.CallBlockingToQt(gui, read)
    result['tabs'] = tabs
    result['current_tab'] = current
    return result

def main():
    import hydrus_driver
    if len(sys.argv) > 1 and sys.argv[1] == '--child':
        import record_api
        out = sys.argv[2]
        result = hydrus_driver.run_client(record_api.unpack_fixture('basic'), record)
        with open(out, 'w') as f: json.dump(result, f)
        return
    with tempfile.TemporaryDirectory() as work:
        out = os.path.join(work, 'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__), '--child', out)
        with open(out) as f: result = json.load(f)
    with open(os.path.join(HERE, 'fixtures/tag_sync_review.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('wrote tag_sync_review.json')
if __name__ == '__main__': main()
