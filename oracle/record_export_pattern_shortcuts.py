#!/usr/bin/env python3
"""Record the real shared export-pattern shortcut menu and its clipboard actions.

Drive the button used by both export-folder and manual-export filename boxes.
Capture clickable heading, separators, labels/tooltips and each triggered payload;
opening and selecting the menu never changes the owner's filename phrase.
"""
import json
import os
import sys
import tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)


def record(session):
    c = session.controller
    def work():
        from qtpy import QtWidgets as QW
        from hydrus.client.gui import ClientGUICore as CGC
        from hydrus.client.gui.widgets import ClientGUICommon as G
        owner = QW.QWidget(c.gui)
        phrase = QW.QLineEdit('keep {hash}', owner)
        button = G.ExportPatternButton(owner)
        captured, copied = [], []
        old_popup, old_pub = CGC.core().PopupMenu, c.pub
        CGC.core().PopupMenu = lambda widget, menu: captured.append(menu)
        def pub(topic, *args, **kwargs):
            if topic == 'clipboard' and args[0] == 'text': copied.append(args[1])
            else: old_pub(topic, *args, **kwargs)
        c.pub = pub
        try:
            button._Hit()
            actions = captured[0].actions()
            result = {'button': button.text(), 'phrase_before': phrase.text(), 'menu': [], 'copied': []}
            for action in actions:
                result['menu'].append({'label': action.text(), 'enabled': action.isEnabled(), 'separator': action.isSeparator(), 'tooltip': action.toolTip()})
                if action.isEnabled() and not action.isSeparator():
                    action.trigger()
            result['copied'] = copied
            result['phrase_after'] = phrase.text()
            owner.resize(500,80);owner.show();QW.QApplication.processEvents()
            owner.grab().save(os.path.join(HERE,'fixtures/export_pattern_shortcuts_editor.png'))
            return result
        finally:
            CGC.core().PopupMenu, c.pub = old_popup, old_pub
            owner.deleteLater()
    return c.CallBlockingToQt(c.gui, work)


def main():
    import hydrus_driver
    if len(sys.argv)>1 and sys.argv[1]=='--child':
        import record_api
        output=sys.argv[2]
        result=hydrus_driver.run_client(record_api.unpack_fixture('basic'),record)
        with open(output,'w') as f:json.dump(result,f)
        return
    with tempfile.TemporaryDirectory() as directory:
        output=os.path.join(directory,'result.json')
        hydrus_driver.run_in_subprocess(os.path.abspath(__file__),'--child',output)
        with open(output) as f:result=json.load(f)
    with open(os.path.join(HERE,'fixtures/export_pattern_shortcuts.json'),'w') as f:
        json.dump(result,f,indent=1,ensure_ascii=False);f.write('\n')
    print('wrote export_pattern_shortcuts.json')
if __name__=='__main__':main()
